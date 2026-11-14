#!/usr/bin/env python3
"""The multi-tap reverb (types 2..11, module `0xC02BB44C`) on its **real instance**.

`docs/research/fx-reverb-leslie.md` s11, s12 called the instance words (line bases, lengths, table
pointers) "zero in the image and never sent". That is true only for the two **organ** units (rows
10, 11 of the module's state table, DM `0xB0F08`, `0xB0F74`). The ten **piano and synth** units
own rows of 108 words at DM `0xC1AC8 + 108 k` (core 1, k = 0..4) and `0xC1A28 + 108 k` (core 2),
and those rows are **static initial DM of the FX image**: smoothing pole `c[-32]`, the delay line
bases (`0x08004000 ..` core 1, `0x08020000 ..` core 2, 5000 words each), the tap table pointers
`c[0]`, `c[1]`, the two 16 word tap tables (`c[44 .. 75]`: PM list pointers `0x9438D`, `0x9439E`,
the FIR pointer `0xE0C46`), the FIR pointer and the output scales `c[42]`, `c[43]` = 1.0. The ARM
adds only the per-type words (`fx-type-words.csv`, kind synth) and the `kRvbAmt` pair.

This tool lays out the instance of core 2 row 5 (synth unit L6), applies the type words and the
amount words, and runs the wrapper `0x120FDB` (4 frames per call, in place at `IN_ADDR`) in the
`sharc-fx` oracle:

    python tools/fx_rvbmulti.py layout SRAM/core2_sram.bin
    python tools/fx_rvbmulti.py responses SRAM/core2_sram.bin --calls 6000
"""

from __future__ import annotations

import argparse
import csv
import struct
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_oracle as fo
import fx_rvbcore_check as chk
import fx_rvbreal as rr

# VERIFIED (state table `0xC0548A78` of the module, read from the settled harness: rows
# `{target, base, size, tag}` with `size >> 4 = 76` words and stride 108 words): core 1 rows 0..4
# at `0xC1AC8 + 108 k`, core 2 rows 5..9 at `0xC1A28 + 108 k`; the ARM type words of the CSV are
# addressed in core 1 row 0 (unit L0), so they are shifted onto the row under test.
ROW_REF = 0xC1AC8
ROW = 0xC1A28
ROW_WORDS = 108
CTX = ROW + 0x20
PC = 0x120FDB  # wrapper: three fs/4 biquads, hold, core 0x120D8A, mix (fx-reverb-leslie s11)
END = 0x1FFFFF  # never reached: the routine's RTS returns here
IN_ADDR = chk.IN_ADDR
DATA = Path(__file__).resolve().parents[1] / "docs/research/data"
TYPES = (2, 3, 4, 5, 7, 8, 9, 10, 11)  # types with a captured block (0, 1: FwReverb; 6: none)
KIND = "synth"


def amount_words(value: int) -> tuple[int, int]:
    """`(c[-31], c[-30])` of `kRvbAmt` `value` (send, dry) from the captured table points."""
    out = {}
    with open(DATA / "fx-param-fits.csv", newline="") as f:
        for r in csv.DictReader(f):
            if r["slot"] == "Rvb" and r["type"] == "2" and r["list"] == "0xc02bb44c":
                if r["id"] == "Edt_Effects_kRvbAmt" and r["ctx"] in ("-31", "-30"):
                    pts = dict(p.split(":") for p in r["samples"].split(";"))
                    out[int(r["ctx"])] = int(pts[str(value)], 16)
    return out[-31], out[-30]


def type_words(rvb_type: int) -> dict[int, int]:
    """The ARM's words of `rvb_type` for the synth kind, moved onto the row under test."""
    out: dict[int, int] = {}
    with open(DATA / "fx-type-words.csv", newline="") as f:
        for r in csv.DictReader(f):
            if (r["slot"], r["type"], r["kinds"]) == ("Rvb", str(rvb_type), KIND):
                a = int(r["dm_word"], 16)
                if ROW_REF <= a < ROW_REF + ROW_WORDS:
                    a += ROW - ROW_REF
                out[a] = int(r["word"], 16)
    return out


def layout(image: str, rvb_type: int, amount: int = 127) -> dict[int, int]:
    """Initial DM of the instance: image row, PM lists of the type, amount words."""
    mem = {a: w for a, w in rr.image_words(image, ROW, ROW_WORDS).items() if w}
    mem.update(type_words(rvb_type))
    send, dry = amount_words(amount)
    mem[CTX - 31], mem[CTX - 30] = send, dry
    mem[CTX - 29] = send  # smoothed state already settled
    return mem


def run_oracle(image: str, binary: Path, frames, mem0):
    """Oracle text for `0x120FDB` on `frames` (8 words per call at `IN_ADDR`)."""
    sets = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M1": 1, "M2": 2, "M9": 1, "M10": 2}
    rt = fo.Routine("rvbmulti", PC, END, sets, lambda *_: None, lambda *_: {}, mem0)
    dumps = {IN_ADDR + k for k in range(8)}
    cases = [{IN_ADDR + k: w for k, w in enumerate(f)} for f in frames]
    return fo.run_oracle(image, binary, rt, cases, dumps)


def response(image: str, binary: Path, rvb_type: int, calls: int, kind: str, amount: int = 127):
    """`(input, output)` float64 pairs `(L, R)` per frame, the oracle on impulse or noise."""
    import random

    import numpy as np

    rng = random.Random(11)
    mem0 = layout(image, rvb_type, amount)
    if kind == "impulse":
        frames = [[0] * 8 for _ in range(calls)]
        frames[0][0] = frames[0][1] = fb.bits(1.0)
    else:
        frames = [[fb.bits(rng.uniform(-0.25, 0.25)) for _ in range(8)] for _ in range(calls)]
    got = fb.parse_dumps(run_oracle(image, binary, frames, mem0))
    words = np.array(
        [[got[k][IN_ADDR + j] for j in range(8)] for k in range(calls)], dtype=np.uint32
    )
    out = words.view(np.float32).reshape(-1, 2).astype(np.float64)
    inp = np.array(frames, dtype=np.uint32).view(np.float32).reshape(-1, 2).astype(np.float64)
    return inp, out


def row_report(image: str) -> list[tuple[int, int]]:
    """Non-zero words of the row as `(ctx offset, word)`: the static instance."""
    raw = Path(image).read_bytes()
    out = []
    for k in range(ROW_WORDS):
        w = struct.unpack_from("<I", raw, 4 * (ROW + k))[0]
        if w:
            out.append((k - 0x20, w))
    return out


ROWS = {1: [ROW_REF + ROW_WORDS * k for k in range(5)], 2: [ROW + ROW_WORDS * k for k in range(5)]}


def instance_rows(image: str, core: int) -> list[tuple[int, int, int, int]]:
    """`(core, row, ctx offset, word)` of every non-zero static word of the five rows of `core`
    (the DM of a core 1 image for `core` 1, of a core 2 image for `core` 2)."""
    raw = Path(image).read_bytes()
    out = []
    for row, base in enumerate(ROWS[core]):
        for k in range(ROW_WORDS):
            w = struct.unpack_from("<I", raw, 4 * (base + k))[0]
            if w:
                out.append((core, row, k - 0x20, w))
    return out


def golden_rows(image: str, binary: Path, calls: int = 3000) -> list[dict]:
    """Derived numbers of the oracle output for every type: impulse at amount 127 and noise at
    amount 63 (hash of the output words, non-zero count, first call with output)."""
    import fx_rvbcore_golden as gold

    rows = []
    for t in TYPES:
        for kind, amount in (("impulse", 127), ("noise", 63)):
            frames = (gold.impulse_words if kind == "impulse" else gold.noise_words)(calls)
            got = fb.parse_dumps(run_oracle(image, binary, frames, layout(image, t, amount)))
            flat = [w for k in range(calls) for w in (got[k][IN_ADDR + j] for j in range(8))]
            live = [any(got[k][IN_ADDR + j] & 0x7FFFFFFF for j in range(8)) for k in range(calls)]
            nz = [k for k, on in enumerate(live) if on]
            rows.append({
                "type": t, "kind": kind, "amount": amount, "calls": calls,
                "hash": f"{gold.fnv(flat):#018x}",
                "nonzero": sum(1 for w in flat if w & 0x7FFFFFFF),
                "first": nz[0] if nz else -1,
            })  # fmt: skip
    return rows


def responses_row(image: str, binary: Path, t: int, calls: int) -> str:
    import numpy as np

    _, imp = response(image, binary, t, calls, "impulse")
    h = imp[:, 0] + imp[:, 1]
    h[0:8] = 0
    nz = np.flatnonzero(np.abs(h) > 0)
    h = h[nz[0] :] if len(nz) else h  # drop the pre-delay: decay is measured from the first echo
    t60, edt = rr.t60_s(h, 48000)
    dens = [round(v, 2) for _, v in rr.echo_density(h, 48000)]
    ni, no = response(image, binary, t, calls, "noise")
    bands = [round(v, 1) for v in rr.band_db(ni[:, 0], no[:, 0], 48000, rr.BANDS)]
    first = int(nz[0]) // 4 if len(nz) else -1
    return f"{t:>4}  {first:>6}  {t60:5.2f}  {edt:5.2f}  {dens}  {bands}"


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("cmd", choices=["layout", "responses", "golden", "instance"])
    p.add_argument("image")
    p.add_argument("--calls", type=int, default=6000)
    p.add_argument("--type", type=int, action="append", help="restrict `responses` to a type")
    p.add_argument("--core", type=int, default=2, choices=[1, 2])
    p.add_argument("--bin")
    a = p.parse_args(argv)
    if a.cmd == "instance":
        w = csv.writer(sys.stdout, lineterminator="\n")
        w.writerow(["core", "row", "ctx", "word"])
        w.writerows((c, r, o, f"{v:#010x}") for c, r, o, v in instance_rows(a.image, a.core))
        return 0
    if a.cmd == "layout":
        print(" ".join(f"{c}:{w:#x}" for c, w in row_report(a.image)))
        return 0
    binary = fb.find_binary(a.bin)
    if a.cmd == "golden":
        cols = ["type", "kind", "amount", "calls", "hash", "nonzero", "first"]
        w = csv.DictWriter(sys.stdout, cols)
        w.writeheader()
        w.writerows(golden_rows(a.image, binary, a.calls))
        return 0
    print("type   first    T60     EDT   echo density 0.1/0.2/0.4/0.8 s   wet/in dB per band")
    for t in a.type or TYPES:
        print(responses_row(a.image, binary, t, a.calls), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

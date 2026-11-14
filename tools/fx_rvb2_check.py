#!/usr/bin/env python3
"""Reverb types 2..11 (module `0xC02BB44C`): instance, oracle comparison, responses.

The 4-frame wrapper `0x120FDB` (calls the packed multi-tap core `0x120D8A`) runs in the `sharc-fx`
oracle on an **instance built from the best evidence** (`docs/research/fx-reverb-leslie.md` s13):
the per-type words of `docs/research/data/fx-type-words.csv`, the pole / send / dry / output words
of the type 0, 1 neighbour, the table pointers `c[0] c[1]` read from the code, and delay lines in
SDRAM of the length of the type 0, 1 lines. Every word that is not a measurement is `EMPIRICAL`
or `ASSUMED` in [`INSTANCE`](#) below.

    python tools/fx_rvb2_check.py check SRAM/core2_sram.bin --type 2 --calls 40
    python tools/fx_rvb2_check.py responses SRAM/core2_sram.bin --calls 6000
"""

from __future__ import annotations

import argparse
import csv
import random
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_model_rvb2_wrap as mdl
import fx_oracle as fo
from fx_pe import M32, Machine, parse_traces

PC, END = 0x120FDB, 0x1FFFFF
C = 0xB0F28  # VERIFIED: ctx of `0xC02BB44C` (organ unit), `fx-type-words.csv`
IN_ADDR = 0xB1000
CSV = Path(__file__).resolve().parents[1] / "docs/research/data/fx-type-words.csv"
LINE1, LINE2 = 0x08040000, 0x08060000
LEN1 = LEN2 = 5000  # ASSUMED: the line size of the type 0, 1 instances (5000 words)
POLE = 0x3F7DC91D  # EMPIRICAL: smoothing pole of the type 0, 1 neighbour `c[0x11]`
FIR = 0xB1C26  # EMPIRICAL: the seven words `0xB1C26 .. 0xB1C2C` are the 7-tap FIR


def type_words(rvb_type: int, kind: str = "organ") -> dict[int, int]:
    mem: dict[int, int] = {}
    with open(CSV, newline="") as f:
        for r in csv.DictReader(f):
            if (r["slot"], r["type"], r["kinds"]) == ("Rvb", str(rvb_type), kind):
                mem[int(r["dm_word"], 16)] = int(r["word"], 16)
    return mem


ADVANCE = 2  # ASSUMED: the write pointer advances one pair (2 words) per call


def instance(rvb_type: int, send: float = 1.0, dry: float = 0.0) -> dict[int, int]:
    """Initial DM of the instance for `rvb_type` (`kRvbAmt` as `send`, `dry`)."""
    sb, db = fb.bits(send), fb.bits(dry)
    c = {
        -32: POLE, -31: sb, -30: db, -29: sb, -28: db,  # smoothing pole, targets, state
        -24: LINE2, -23: LEN2, -22: LINE2, -15: LINE1, -14: LEN1, -13: LINE1,
        -6: LINE2, -4: LINE1, -2: 0, 0: 0xB0F74, 1: 0xB0F84, 42: fb.bits(1.0), 43: fb.bits(1.0),
    }  # fmt: skip
    mem = {C + o: w & M32 for o, w in c.items()}
    for base in (0xB0F74, 0xB0F84):  # the two tables behind `c[0]`, `c[1]`
        mem[base - 28] = 0x95013 if base == 0xB0F74 else 0x95024  # I10: PM list
        mem[base - 17] = FIR
        mem[base - 19] = 0
        mem[base - 32] = mem[base - 30] = ADVANCE  # M6 (line 2 step), M5 (line 1 step)
    mem.update(type_words(rvb_type))
    return mem


def run_oracle(image: str, binary: Path, frames, mem0, trace=True):
    sets = {"I0": IN_ADDR, "I1": C, "M0": 0, "M2": 2, "M9": 1}
    rt = fo.Routine("rvb2", PC, END, sets, lambda *_: None, lambda *_: {}, mem0)
    dumps = {IN_ADDR + k for k in range(8)}
    cases = [{IN_ADDR + k: w for k, w in enumerate(f)} for f in frames]
    return fo.run_oracle(image, binary, rt, cases, dumps, trace=trace)


def entry_machine(row, mem, trace):
    ir = {f"{k}{n}": row[k][n] for k in "imlb" for n in range(16)}
    m = Machine(mem, ir, trace)
    m.x, m.y = list(row["r"]), list(row["s"])
    return m


class ImageMem(dict):
    """Model memory: the written words, else the initial words of the flat SRAM image (the oracle's
    DM / PM), else zero."""

    def __init__(self, image: str, init):
        super().__init__(init)
        self.raw = Path(image).read_bytes()

    def __missing__(self, a):
        w = (
            int.from_bytes(self.raw[4 * a : 4 * a + 4], "little")
            if 4 * a + 4 <= len(self.raw)
            else 0
        )
        self[a] = w
        return w


def check(image: str, binary: Path, rvb_type: int, calls: int, seed: int = 3):
    rng = random.Random(seed)
    mem0 = instance(rvb_type, 0.7, 0.4)
    frames = [[fb.bits(rng.uniform(-0.5, 0.5)) for _ in range(8)] for _ in range(calls)]
    text = run_oracle(image, binary, frames, mem0)
    got = fb.parse_dumps(text)
    traces = parse_traces(text)
    mem = ImageMem(image, mem0)
    bad = total = 0
    first: list = []
    for k, fr in enumerate(frames):
        rows = traces[k]
        mem.update({IN_ADDR + j: w for j, w in enumerate(fr)})
        m = entry_machine(rows[0], mem, rows)
        try:
            mdl.wrap4(m)
        except AssertionError as e:
            return bad, total, [("trace", k, str(e))]
        for j in range(8):
            total += 1
            g, w = got[k][IN_ADDR + j], mem[IN_ADDR + j]
            if g != w and (g | w) & 0x7FFFFFFF:
                bad += 1
                if len(first) < 4:
                    first.append((k, j, hex(g), hex(w)))
    return bad, total, first


def response(image: str, binary: Path, rvb_type: int, calls: int, kind: str):
    """Oracle `(input, output)` samples (L, R at 48 kHz) of an impulse or noise."""
    import numpy as np

    rng = random.Random(11)
    mem0 = instance(rvb_type)
    if kind == "impulse":
        frames = [[0] * 8 for _ in range(calls)]
        frames[0][0] = frames[0][1] = fb.bits(1.0)
    else:
        frames = [[fb.bits(rng.uniform(-0.25, 0.25)) for _ in range(8)] for _ in range(calls)]
    got = fb.parse_dumps(run_oracle(image, binary, frames, mem0, trace=False))
    words = np.array(
        [[got[k][IN_ADDR + j] for j in range(8)] for k in range(calls)], dtype=np.uint32
    )
    out = words.view(np.float32).reshape(-1, 2).astype(np.float64)
    inp = np.array(frames, dtype=np.uint32).view(np.float32).reshape(-1, 2).astype(np.float64)
    return inp, out


def responses(image: str, binary: Path, calls: int, types: list[int]) -> int:
    import fx_rvbreal as rr
    import numpy as np

    for t in types:
        _, imp = response(image, binary, t, calls, "impulse")
        h = imp[:, 0] + imp[:, 1]
        h[0:8] = 0
        try:
            t60, edt = rr.t60_s(h, 48000)
        except Exception:  # noqa: BLE001
            t60 = edt = float("nan")
        print(f"type {t}: T60 {t60:.2f} s, EDT {edt:.2f} s, peak {np.abs(imp).max():.4f}")
        print("  echo density", [(a_, round(b_, 2)) for a_, b_ in rr.echo_density(h, 48000)])
        ni, no = response(image, binary, t, calls, "noise")
        print(
            "  wet/in dB per band",
            [round(v, 1) for v in rr.band_db(ni[:, 0], no[:, 0], 48000, rr.BANDS)],
        )
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("cmd", choices=["check", "responses"])
    p.add_argument("image")
    p.add_argument("--type", type=int, default=2)
    p.add_argument("--calls", type=int, default=20)
    p.add_argument("--bin")
    p.add_argument("--types", type=int, nargs="*", default=[2, 3, 4, 5, 7, 8, 9, 10, 11])
    a = p.parse_args(argv)
    if a.cmd == "responses":
        return responses(a.image, fb.find_binary(a.bin), a.calls, a.types)
    bad, total, first = check(a.image, fb.find_binary(a.bin), a.type, a.calls)
    print(f"check type {a.type}: {total - bad}/{total} words identical {first}")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())

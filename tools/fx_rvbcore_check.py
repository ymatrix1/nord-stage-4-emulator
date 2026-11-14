#!/usr/bin/env python3
"""Run the reverb core `0x123F61` through the `sharc-fx` oracle and compare it with
`tools/fx_model_rvbcore.py` instruction by instruction and word for word.

The oracle calls the 2-frame wrapper `0x12435C` (it loads `R0`, `R4` from `I0`, calls the core and
mixes), so one call exercises the core on carried state. The context is the type 0 word set plus a
**consistent pointer layout** (the firmware's pointer words are not in the boot log, see
`docs/research/fx-reverb-leslie.md` s7): line A base `LA` length `LEN_A`, line B `LB` / `LEN_B`,
the all-pass buffer `AP` (length `c[0x20] = 1570`), a loop state array `ST`, and a coefficient
list at `PMT` (random values: the model has to follow the routine, not the data).

    python tools/fx_rvbcore_check.py check SRAM/core2_sram.bin --calls 8
    python tools/fx_rvbcore_check.py impulse SRAM/core2_sram.bin --calls 400
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
import fx_model_rvbcore as core
import fx_oracle as fo
from fx_pe import M32, Machine, Missing, parse_traces

WRAP, END = 0x12435C, 0x1FFFFF
IN_ADDR, CTX = 0xB1000, 0xB0CAE
LA, LB, AP, ST, PMT = 0xB5000, 0xB6000, 0xB8000, 0xB7000, 0xB9000
LEN_A, LEN_B, LEN_AP = 2658, 4068, 1570  # ASSUMED (A, B: the tap offsets), VERIFIED (c[0x20])
CSV = Path(__file__).resolve().parents[1] / "docs/research/data/fx-type-words.csv"


def type_words(rvb_type: int = 0) -> dict[int, int]:
    """The captured type words of the organ `Rvb` module as `{address: word}`."""
    mem = {}
    with open(CSV, newline="") as f:
        for r in csv.DictReader(f):
            if (r["slot"], r["type"], r["kinds"]) == ("Rvb", str(rvb_type), "organ"):
                mem[CTX + int(r["ctx"])] = int(r["word"], 16)
    return mem


def layout(rvb_type: int = 0, seed: int = 1, short: bool = False) -> dict[int, int]:
    """Type words plus the pointer, length and state words the routine needs.

    `short` shrinks the two delay lines to 16 / 24 words so that the output is non-zero within a
    few calls (golden vectors); the all-pass buffer and taps keep the firmware values."""
    rng = random.Random(seed)
    c = {17: fb.bits(0.99), 18: 0x3E19B500, 19: 0x3F7D197C, 20: 0x3E19B500, 21: fb.bits(0.3),
         22: fb.bits(1.0), 23: fb.bits(1.0), 32: LEN_AP, 39: LA, 40: LEN_A, 41: LA, 44: LB,
         45: LEN_B, 46: LB, 50: AP, 52: AP, -13: ST, -12: PMT}  # fmt: skip
    mem = type_words(rvb_type)
    mem.update({CTX + o: w for o, w in c.items()})
    mem.update({PMT + k: fb.bits(rng.uniform(-0.6, 0.6)) for k in range(128)})
    if short:
        mem.update({CTX + 40: 16, CTX + 43: (-16) & 0xFFFFFFFF, CTX + 45: 24,
                    CTX + 48: (-24) & 0xFFFFFFFF})  # fmt: skip
    return mem


def entry_machine(row: dict, mem: dict[int, int], trace: list) -> Machine:
    """A machine in the state of the oracle `row` (registers and DAG files)."""
    ir = {f"{k}{n}": row[k][n] for k in "imlb" for n in range(16)}
    m = Machine(mem, ir, trace)
    m.x, m.y = list(row["r"]), list(row["s"])
    return m


def run_model(frame: list[int], mem: dict[int, int]) -> tuple[int, int, int, int]:
    """The core on one frame pair `(L0, R0, L1, R1)` (words) with no oracle: the DAG registers
    the pass context provides (`M1 M2 M7 M9 M10 M15`, read from the oracle trace) and `I1`.
    `mem` is updated; returns the words of `R12`, `S12`, `R14`, `S14`."""
    ir = {f"{k}{n}": 0 for k in "imlb" for n in range(16)}
    ir.update({"i0": IN_ADDR, "i1": CTX, "m1": 1, "m2": 2, "m7": M32, "m9": 1, "m10": 2,
               "m15": M32})  # fmt: skip
    m = Machine(mem, ir)
    m.x[0], m.y[0], m.x[4], m.y[4] = frame
    core.run(m)
    return core.wd(m.x[12]), core.wd(m.y[12]), core.wd(m.x[14]), core.wd(m.y[14])


def golden_words(calls: int = 12, seed: int = 1) -> list[tuple[int, int, int, int]]:
    """Output words of `calls` carried model calls on random frames (no firmware needed).

    The model was checked word for word against the oracle with `check` (every instruction's
    registers and every written memory word); these values pin it for `tools/tests`."""
    rng = random.Random(seed)
    mem = Missing(layout(seed=seed, short=True), default=0)
    return [run_model([fb.bits(rng.uniform(-1, 1)) for _ in range(4)], mem) for _ in range(calls)]


def run_calls(image: str, binary: Path, frames: list[list[int]], mem0: dict[int, int]):
    """Oracle text for `frames` (4 words per call) on the initial memory `mem0`."""
    sets = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1}
    rt = fo.Routine("rvbcore", WRAP, END, sets, lambda *_: None, lambda *_: {}, mem0)
    regions = (
        (IN_ADDR, 4), (CTX - 0x20, 0x60), (LA, mem0[CTX + 40] + 8), (LB, mem0[CTX + 45] + 8),
        (AP, 2 * LEN_AP + 8), (ST, 0x180),
    )  # fmt: skip
    dumps = {a + k for a, n in regions for k in range(n)}
    cases = [{IN_ADDR + k: w for k, w in enumerate(f)} for f in frames]
    return fo.run_oracle(image, binary, rt, cases, dumps, trace=True)


def model_calls(text: str, frames, mem0):
    """Run the model per call from the oracle rows; yields `(call, machine, entry row)`."""
    mem = Missing(mem0, default=0)
    traces = parse_traces(text)
    for k, fr in enumerate(frames):
        rows = traces[k]
        i0 = next(i for i, r in enumerate(rows) if r["pc"] == core.PC)
        i1 = next(i for i, r in enumerate(rows) if r["pc"] == core.END)
        mem.update({IN_ADDR + j: w for j, w in enumerate(fr)})
        m = entry_machine(rows[i0], mem, rows[i0 : i1 + 1])
        core.run(m)
        yield k, m, rows[i1]


def check(
    image: str, binary: Path, calls: int, seed: int = 1, short: bool = False
) -> tuple[int, int, list]:
    """`(differing words, words compared, first failures)` over `calls` carried calls."""
    rng = random.Random(seed)
    mem0 = layout(seed=seed, short=short)
    frames = [[fb.bits(rng.uniform(-1, 1)) for _ in range(4)] for _ in range(calls)]
    text = run_calls(image, binary, frames, mem0)
    got = fb.parse_dumps(text)
    bad = total = 0
    first: list = []
    for k, m, _ in model_calls(text, frames, mem0):
        for a in sorted(m.written):
            total += 1
            w = m.mem[a]
            if a in got[k] and got[k][a] & 0x7FFFFFFF != w & 0x7FFFFFFF and got[k][a] != w:
                bad += 1
                if len(first) < 4:
                    first.append((k, hex(a), hex(got[k][a]), hex(w)))
    return bad, total, first


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("cmd", choices=["check", "golden"])
    p.add_argument("image", nargs="?")
    p.add_argument("--calls", type=int, default=8)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--bin")
    p.add_argument("--short", action="store_true", help="16 / 24 word delay lines")
    a = p.parse_args(argv)
    if a.cmd == "golden":
        for row in golden_words(a.calls, a.seed):
            print(", ".join(f"{w:#010x}" for w in row))
        return 0
    bad, total, first = check(a.image, fb.find_binary(a.bin), a.calls, a.seed, a.short)
    print(f"check: {total - bad}/{total} words identical {first}")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Check `tools/fx_model_rvbmulti.py` against a `sharc-fx --trace` of the wrapper `0x120FDB`.

The oracle runs the wrapper (4 frames per call, in place at `IN_ADDR`) on the real instance of
`tools/fx_rvbmulti.py` (image row, the ARM type words, the `kRvbAmt` pair). The model starts every
call from the register files of the first trace row and carries its own memory (the static image
under the overlay, SDRAM lines zero). Before every instruction the R, S, I, M, L and B files are
compared; the first mismatch is reported with its pc. `outputs` compares only the output words of
long runs (the model carries its registers across calls).

    python tools/fx_rvbmulti_check.py check SRAM/core2_sram.bin --type 2 --calls 6
    python tools/fx_rvbmulti_check.py outputs SRAM/core2_sram.bin --type 2 --calls 600
"""

from __future__ import annotations

import argparse
import random
import struct
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_leslie_ops as ops
import fx_rvbmulti as rm
from fx_pe import parse_traces

IN_ADDR = rm.IN_ADDR


class Mem(ops.LazyMem):
    """Static image words under the overlay; addresses beyond the image (SDRAM lines) are zero."""

    def __missing__(self, key: int) -> int:
        k = key & ops.M32
        if 4 * k + 4 > len(self.image):
            return 0
        return struct.unpack_from("<I", self.image, 4 * k)[0]


def frames_noise(calls: int, seed: int = 5, amp: float = 0.25) -> list[list[int]]:
    rng = random.Random(seed)
    return [[fb.bits(rng.uniform(-amp, amp)) for _ in range(8)] for _ in range(calls)]


def entry_machine(row: dict, mem, trace) -> ops.LMachine:
    ir = {f"{k}{n}": row[k][n] for k in "imlb" for n in range(16)}
    m = ops.LMachine(mem, ir, trace)
    m.x, m.y = list(row["r"]), list(row["s"])
    return m


def oracle(image: str, binary: Path, rvb_type: int, frames, amount: int, trace: bool) -> str:
    mem0 = rm.layout(image, rvb_type, amount)
    import fx_oracle as fo
    from fx_rvbmulti import CTX, END, PC

    sets = {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M1": 1, "M2": 2, "M9": 1, "M10": 2}
    rt = fo.Routine("rvbmulti", PC, END, sets, lambda *_: None, lambda *_: {}, mem0)
    dumps = {IN_ADDR + k for k in range(8)}
    cases = [{IN_ADDR + k: w for k, w in enumerate(f)} for f in frames]
    return fo.run_oracle(image, binary, rt, cases, dumps, trace=trace)


def check(image: str, binary: Path, rvb_type: int, calls: int, amount: int = 63):
    """`(verified steps, total steps, first mismatch or "")` over `calls` carried calls."""
    import fx_model_rvbmulti as model

    frames = frames_noise(calls)
    traces = parse_traces(oracle(image, binary, rvb_type, frames, amount, True))
    mem = Mem(Path(image).read_bytes(), rm.layout(image, rvb_type, amount))
    done = total = 0
    for k in range(calls):
        rows = traces[k]
        total += len(rows)
        mem.update({IN_ADDR + j: w for j, w in enumerate(frames[k])})
        m = entry_machine(rows[0], mem, rows)
        try:
            model.wrapper(m)
        except IndexError:  # the oracle stops at the final `rts`: one step past the trace
            if len(m.pcs) != len(rows) + 1:
                return done + len(m.pcs), total, f"call {k}: trace ended at {m.pcs[-1]:#x}"
            m.pcs.pop()
        except AssertionError as exc:
            return done + len(m.pcs), total, f"call {k}: {exc}"
        done += len(m.pcs)
    return done, total, ""


def outputs(image: str, binary: Path, rvb_type: int, calls: int, amount: int = 63):
    """`(calls matched, first mismatch)`: model registers carried across calls, words compared."""
    import fx_model_rvbmulti as model

    frames = frames_noise(calls)
    text = oracle(image, binary, rvb_type, frames[:1], amount, True)
    first = parse_traces(text)[0][0]
    dumps = fb.parse_dumps(oracle(image, binary, rvb_type, frames, amount, False))
    mem = Mem(Path(image).read_bytes(), rm.layout(image, rvb_type, amount))
    m = entry_machine(first, mem, None)
    for k in range(calls):
        mem.update({IN_ADDR + j: w for j, w in enumerate(frames[k])})
        m.ir.update({"i0": IN_ADDR, "i1": rm.CTX, "m0": 0, "m1": 1, "m2": 2, "m9": 1, "m10": 2})
        model.wrapper(m)
        for j in range(8):
            if mem[IN_ADDR + j] != dumps[k][IN_ADDR + j]:
                a, b = mem[IN_ADDR + j], dumps[k][IN_ADDR + j]
                return k, f"call {k} word {j}: model {a:#x} oracle {b:#x}"
    return calls, ""


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("cmd", choices=["check", "outputs"])
    ap.add_argument("image")
    ap.add_argument("--type", type=int, default=2)
    ap.add_argument("--calls", type=int, default=4)
    ap.add_argument("--bin")
    a = ap.parse_args(argv)
    binary = fb.find_binary(a.bin)
    if a.cmd == "check":
        done, total, err = check(a.image, binary, a.type, a.calls)
        print(f"{done} of {total} steps verified" + (f"; {err}" if err else ""))
        return 1 if err else 0
    n, err = outputs(a.image, binary, a.type, a.calls)
    print(f"{n} of {a.calls} calls identical" + (f"; {err}" if err else ""))
    return 1 if err else 0


if __name__ == "__main__":
    sys.exit(main())

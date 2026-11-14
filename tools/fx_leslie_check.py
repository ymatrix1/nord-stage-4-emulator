#!/usr/bin/env python3
"""Check `tools/fx_model_leslie.py` against a `sharc-fx --trace` of the Leslie processor.

The oracle runs `0x1256AD` once per 2 frames with the boot log replayed and the rotor running
(`DM(0xB2F21) = 0x7FFFFFFF`, speed words as the ARM sends them); the model starts every call from
the register files of the first trace row and carries its own memory. Before every instruction
the R, S, I, M, L and B files are compared; the first mismatch is reported with its pc.

    python tools/fx_leslie_check.py PNO_core1_sram.bin pno_core1.script --calls 8
    python tools/fx_leslie_gen.py pno1.txt > tools/fx_model_leslie.py     # regenerate the model
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_leslie_ops as ops
import fx_leslie_run as lr
from fx_pe import parse_traces

LIMIT = 0xB2F21  # the rotor phase limit word; 0 freezes the rotor
LIMIT_VALUE = 0x7FFFFFFF
#: speed words as the ARM re-sends them (slow); the check can scale them to reach the wrap.
SPEED = {0xB2F25: 0x22B00, 0xB2F26: 0x26000, 0xB2FA1: 0x15D00}
MULT = [1]


def rotor_words() -> dict[int, int]:
    """Words that start the rotor: the limit and the (scaled) speed words."""
    return {LIMIT: LIMIT_VALUE, **{a: v * MULT[0] for a, v in SPEED.items()}}


def oracle(pno1: Path, script: Path, calls: int, binary: Path, trace: bool = True) -> str:
    """Oracle output of `calls` calls with the rotor running (`trace` rows and the IO dumps)."""
    extra = [t for a, v in rotor_words().items() for t in ("--poke", f"0:{a:#x}={v:#x}")]
    extra += ["--trace"] if trace else []
    with tempfile.NamedTemporaryFile("w", suffix=".args", delete=False) as f:
        f.write("\n".join(lr.build_args(script, calls, extra)))
    out = subprocess.run(
        [str(binary), str(pno1), f"@{f.name}"], capture_output=True, text=True, check=True
    )
    return out.stdout


def input_words(calls: int) -> list[list[int]]:
    """The four words poked at `IO` before each call (the seeded noise of `fx_leslie_run`)."""
    pokes = lr.noise_pokes(calls)
    words: list[list[int]] = [[] for _ in range(calls)]
    for tgt in pokes[1::2]:
        k, rest = tgt.split(":", 1)
        v = float(rest.split("=f")[1])
        words[int(k)].append(ops.wd(ops.fx_pe.fb.bits(v)))
    return words


def entry_machine(row: dict, mem, trace: list) -> ops.LMachine:
    ir = {f"{k}{n}": row[k][n] for k in "imlb" for n in range(16)}
    m = ops.LMachine(mem, ir, trace)
    m.x, m.y = list(row["r"]), list(row["s"])
    return m


def check(pno1: Path, script: Path, calls: int, binary: Path, model) -> tuple[int, int, str]:
    """`(verified steps, total steps, first mismatch or "")` over `calls` calls."""
    traces = parse_traces(oracle(pno1, script, calls, binary))
    overlay = lr_boot(script)
    overlay.update(rotor_words())
    mem = ops.LazyMem(pno1.read_bytes(), overlay)
    inputs = input_words(calls)
    done = total = 0
    for k in range(calls):
        rows = traces[k]
        total += len(rows)
        for j, w in enumerate(inputs[k]):
            mem[lr.IO + j] = w
        m = entry_machine(rows[0], mem, rows)
        m.callees[0x1237F8] = model.callee
        try:
            model.main(m)
        except IndexError:  # the oracle stops before the final `rts`: one step past the trace
            if len(m.pcs) != len(rows) + 1:
                return done + len(m.pcs), total, f"call {k}: trace ended at {m.pcs[-1]:#x}"
            m.pcs.pop()
        except AssertionError as exc:
            return done + len(m.pcs), total, f"call {k}: {exc}"
        done += len(m.pcs)
    return done, total, ""


def lr_boot(script: Path) -> dict[int, int]:
    out = {}
    for line in script.read_text(encoding="utf-8").splitlines():
        p = line.split()
        if len(p) == 3:
            out[int(p[1], 16)] = int(p[2], 16)
    return out


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("pno_core1", type=Path)
    ap.add_argument("script", type=Path)
    ap.add_argument("--calls", type=int, default=4)
    ap.add_argument("--outputs", action="store_true", help="long run, outputs only")
    ap.add_argument("--speed-mult", type=int, default=1)
    ap.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx.exe"))
    ns = ap.parse_args(argv)
    MULT[0] = ns.speed_mult
    import fx_model_leslie as model

    if ns.outputs:
        n, err = check_outputs(ns.pno_core1, ns.script, ns.calls, ns.binary, model)
        print(f"{n} of {ns.calls} calls identical" + (f"; {err}" if err else ""))
        return 1 if err else 0
    done, total, err = check(ns.pno_core1, ns.script, ns.calls, ns.binary, model)
    print(f"{done} of {total} steps verified" + (f"; {err}" if err else ""))
    return 1 if err else 0


def check_outputs(pno1: Path, script: Path, calls: int, binary: Path, model) -> tuple[int, str]:
    """Long run without a trace: the model carries its registers from the end of the previous call
    (the first call starts from the trace row of a one call oracle run) and every call's `IO`
    words are compared with the oracle dumps. Returns `(calls matched, first mismatch)`."""
    first = parse_traces(oracle(pno1, script, 1, binary))[0][0]
    dumps = fb.parse_dumps(oracle(pno1, script, calls, binary, trace=False))
    overlay = lr_boot(script)
    overlay.update(rotor_words())
    mem = ops.LazyMem(pno1.read_bytes(), overlay)
    inputs = input_words(calls)
    m = entry_machine(first, mem, None)
    for k in range(calls):
        for j, w in enumerate(inputs[k]):
            mem[lr.IO + j] = w
        m.ir.update({"i0": lr.IO, "i1": lr.STATE, "m0": 0, "m1": 1, "m2": 2, "m9": 1})
        m._row = 0
        m.callees[0x1237F8] = model.callee
        model.main(m)
        for j in range(4):
            if mem[lr.IO + j] != dumps[k][lr.IO + j]:
                return (
                    k,
                    f"call {k} word {j}: model {mem[lr.IO + j]:#x} oracle {dumps[k][lr.IO + j]:#x}",
                )
    return calls, ""


if __name__ == "__main__":
    sys.exit(main())

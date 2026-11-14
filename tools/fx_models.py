#!/usr/bin/env python3
"""Registry and command line of the FX routine models checked against the `sharc-fx` oracle.

    cargo build -p nord-sharc --release
    python tools/fx_models.py check delay SRAM/core2_sram.bin --scenarios 1200
    python tools/fx_models.py trace delay SRAM/core2_sram.bin     # per-instruction state compare
    python tools/fx_models.py digest delay                        # golden digest (no image)

`SRAM/core2_sram.bin` is the FX Core 2 flat SRAM image (`tools/ns4_dsp_ldr.py --sram`).
"""

from __future__ import annotations

import argparse
import hashlib
import random
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_model_allpass
import fx_model_apan
import fx_model_delay
import fx_model_lfo
import fx_model_resonant
import fx_model_resonant4
import fx_oracle as fo
from fx_pe import M32

MODULES = {
    "delay": fx_model_delay,
    "lfo": fx_model_lfo,
    "allpass": fx_model_allpass,
    "apan": fx_model_apan,
    "resonant": fx_model_resonant,
    "resonant4": fx_model_resonant4,
}
GOLDEN_SEED, GOLDEN_SCENARIOS = 20260101, 8


def routines() -> dict[str, fo.Routine]:
    return {name: mod.routine() for name, mod in MODULES.items()}


def digest(rt: fo.Routine) -> str:
    """SHA-256 of the words a fixed set of scenarios makes the model write (derived data only)."""
    rng = random.Random(GOLDEN_SEED)
    h = hashlib.sha256()
    for _ in range(GOLDEN_SCENARIOS):
        mem, written = fo.model_run(rt, rt.scenario(rng))
        for a in sorted(written):
            h.update(f"{a:x}={mem[a] & M32:08x};".encode())
    return h.hexdigest()


def find_binary(explicit: str | None) -> Path:
    if explicit:
        return Path(explicit)
    root = Path(__file__).resolve().parents[1]
    for name in ("sharc-fx.exe", "sharc-fx"):
        p = root / "target" / "release" / name
        if p.exists():
            return p
    raise SystemExit("sharc-fx not built: cargo build -p nord-sharc --release")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for cmd in ("check", "trace"):
        sp = sub.add_parser(cmd)
        sp.add_argument("routine", choices=sorted(MODULES))
        sp.add_argument("image")
        sp.add_argument("--scenarios", type=int, default=300)
        sp.add_argument("--seed", type=int, default=1)
        sp.add_argument("--bin")
    dg = sub.add_parser("digest")
    dg.add_argument("routine", choices=sorted(MODULES))
    args = p.parse_args(argv)
    rt = routines()[args.routine]
    if args.cmd == "digest":
        print(digest(rt))
        return 0
    binary = find_binary(args.bin)
    rng = random.Random(args.seed)
    if args.cmd == "trace":
        case = rt.scenario(rng)
        rows = fo.trace_call(args.image, binary, rt, case)
        fo.model_run(rt, case, rows)
        print(f"{rt.name}: {len(rows)} instructions, registers and DAG state identical before each")
        return 0
    bad, n, fails = fo.compare(args.image, binary, rt, rng, args.scenarios)
    print(f"{rt.name} {rt.pc:#x}: {n - bad}/{n} scenarios bit-identical")
    for f in fails:
        print("  first differences:", f)
    return 0 if bad == 0 else 2


if __name__ == "__main__":
    sys.exit(main())

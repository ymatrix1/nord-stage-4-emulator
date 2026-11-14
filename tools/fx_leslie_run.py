#!/usr/bin/env python3
"""Run the Leslie processor `0x1256AD` of PNO core 1 in the `sharc-fx` oracle with the boot state.

The routine is called once per 2 frames (1,885 steps) with the boot log replayed into DM
(`tools/arm_bulk_tables.py capture`, `pno_core1.script`), the signal pair in place at `I0` and the
unit state `I1 = 0xB2F2A` (the row word `0xB2F2A`, the only address whose words `+0xB..+0xD` are
the three table pointers `0xB0276`, `0xB02F7`, `0xB0378`). Pass context: `0x121007` booted to
`0x1212E7` like `sharc-pass`.

    python tools/fx_leslie_run.py PNO_core1_sram.bin pno_core1.script --calls 400 [--poke 0:A=V ...]

It prints `dump ADDR word float` rows (`0xB6000..3` is the in-place signal pair). Evidence and
limits: `docs/research/fx-leslie.md` s4.
"""

from __future__ import annotations

import argparse
import random
import subprocess
import sys
import tempfile
from pathlib import Path

#: VERIFIED (listing `0x12A99..0x12AAB`): the row calls `I8 = DM(0xB2F29)` / `DM(0xB2F2B)` with
#: `I1 = DM(0xB2F2A)` / `DM(0xB2F2C)`; the routine itself is `0x1256AD..0x126366`.
ROUTINE, END, STATE, IO = 0x1256AD, 0x126366, 0xB2F2A, 0xB6000
PASS_DEVS = ("0x28ac0380=1", "0x3108d008=5", "0x3108c06c=0x400", "0x3108c00c=4")


def boot_pokes(script: Path) -> list[str]:
    """`--poke 0:ADDR=V` arguments of the boot log words (`pass address value` lines)."""
    out: list[str] = []
    for line in script.read_text(encoding="utf-8").splitlines():
        p = line.split()
        if len(p) == 3:
            out += ["--poke", f"0:{int(p[1], 16):#x}={int(p[2], 16):#x}"]
    return out


def noise_pokes(calls: int, seed: int = 3, amp: float = 0.3) -> list[str]:
    """One seeded uniform noise block (4 words) per call at `IO`."""
    rng = random.Random(seed)
    out: list[str] = []
    for k in range(calls):
        for j in range(4):
            out += ["--poke", f"{k}:{IO + j:#x}=f{rng.uniform(-amp, amp)}"]
    return out


def build_args(script: Path, calls: int, extra: list[str]) -> list[str]:
    args = ["--entry", "0x121007"]
    for d in PASS_DEVS:
        args += ["--dev", d]
    args += ["--flag", "1=1", "--trigger", "0xB00E1", "--stop-at", "0x1212E7", "--max", "3000000"]
    args += ["--call", hex(ROUTINE), "--until", hex(END), "--calls", str(calls)]
    for reg, v in (("I0", IO), ("I1", STATE), ("M0", 0), ("M1", 1), ("M2", 2), ("M9", 1)):
        args += ["--set", f"{reg}={v:#x}"]
    args += boot_pokes(script) + noise_pokes(calls) + extra
    return [*args, "--dump", f"{IO:#x}:4"]


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("pno_core1", type=Path)
    ap.add_argument("script", type=Path)
    ap.add_argument("--calls", type=int, default=400)
    ap.add_argument("--binary", type=Path, default=Path("target/release/sharc-fx.exe"))
    ns, extra = ap.parse_known_args(argv)
    with tempfile.NamedTemporaryFile("w", suffix=".args", delete=False) as f:
        f.write("\n".join(build_args(ns.script, ns.calls, extra)))
    out = subprocess.run(
        [str(ns.binary), str(ns.pno_core1), f"@{f.name}"], capture_output=True, text=True
    )
    sys.stdout.write(out.stdout)
    return out.returncode


if __name__ == "__main__":
    sys.exit(main())

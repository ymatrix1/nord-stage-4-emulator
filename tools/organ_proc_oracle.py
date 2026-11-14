#!/usr/bin/env python3
"""Oracle chords of the Farfisa (`0x122B85`) and B3 Bass (`0x122296`) processors.

Builds the replay script of the corrected state of `docs/research/organ-farfisa-bass.md` s7 and
runs the complete PNO core 1 pass (`sharc-pass`):

* the full ARM boot log (`tools/arm_bulk_tables.py capture`, `pno_core1.script`) **without** the two
  sentinel words `DM 0xB2ECF`, `0xB2FF0` (`docs/research/arm-boot-state.md` s4),
* the model install (`tools/organ_fb_pass.py script`, the first `INSTALL_WORDS[model]` words after
  the boot log of a `--keys` free capture),
* the key-down words: Farfisa from `docs/research/data/keydown-organ-models.csv` (three weight
  words of 1/48 and three companion words per key, `farfisa_keydown`), Bass from the ARM key-down
  of `organ_fb_pass.py` (`DM 0x951F1..` weights, already part of the capture).

    python tools/organ_proc_oracle.py build farfisa BOOT.script CAPTURE.script OUT.script \
        --keys 60,64,67
    python tools/organ_proc_oracle.py run SRAM/core1_sram.bin OUT.script \
        --exe target/release/sharc-pass

`build` needs a capture made with `organ_fb_pass.py script --model N --keys 60` (the install words
are its first `INSTALL_WORDS[N]` lines after the boot log).
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import organ_fb_pass as fb

# VERIFIED (keydown-organ-models.csv, model 2, notes 36, 60, 84): the weight words of key `n` are
# `DM(0x95FE4 + n + off)` for these offsets, value 1/48 (float 0x3CAAAA80); each is accompanied
# by one bias word `DM(0xC2D58 + k)` = -0.00958 (0xBC1D5580). Companions do not depend on the key.
FARFISA_WEIGHT_BASE = 0x95FE4
FARFISA_OFFSETS = ((0x00, 0xC2D58), (0x62, 0xC2D59), (0xD0, 0xC2D5C))
FARFISA_WEIGHT = 0x3CAAAA80
FARFISA_BIAS = 0xBC1D5580
# VERIFIED (arm-boot-state.md s4): the boot words that gate the B3 and Bass chains off.
SENTINELS = (0xB2ECF, 0xB2FF0)
# VERIFIED (organ-farfisa-bass.md s2): messages of the model install (after the boot log).
INSTALL_WORDS = {2: 526, 5: 337}
BOOT_WORDS = 29_760 - len(SENTINELS)


def farfisa_keydown(keys: list[int]) -> list[str]:
    """`0 ADDR VALUE` script lines of the key-down of `keys` (Farfisa, model 2)."""
    out = []
    for key in keys:
        for off, bias in FARFISA_OFFSETS:
            out.append(f"0 {FARFISA_WEIGHT_BASE + key + off:#x} {FARFISA_WEIGHT:#010x}")
            out.append(f"0 {bias:#x} {FARFISA_BIAS:#010x}")
    return out


def strip_sentinels(lines: list[str]) -> list[str]:
    """The boot log without the writes to `SENTINELS`."""
    return [ln for ln in lines if int(ln.split()[1], 16) not in SENTINELS]


def build_script(model: int, boot: list[str], capture: list[str], keys: list[int]) -> list[str]:
    """Boot log (no sentinels) + model install + key-down words for `keys`.

    `capture` is a `organ_fb_pass.py script` of `--model model` (ctor + boot + install + key-down
    of its own keys); only its install part is used. For model 5 (Bass) the key-down of `keys` is
    taken from the capture itself, so `capture` must have been made with the same keys.
    """
    base = strip_sentinels(boot)
    install = capture[len(boot) : len(boot) + INSTALL_WORDS[model]]
    if model == 2:
        return base + install + farfisa_keydown(keys)
    return base + capture[len(boot) :]


def chord_lines(lane, count: int = 6) -> list[tuple[float, float]]:
    """`(Hz, dB below the strongest)` of the strongest spectral lines of a lane series."""
    return [(ln.hz, ln.level_db) for ln in fb.top_lines(lane, count)]


def cmd_build(a: argparse.Namespace) -> int:  # pragma: no cover - file plumbing
    model = {"farfisa": 2, "bass": 5}[a.model]
    boot = a.boot.read_text().splitlines()
    cap = a.capture.read_text().splitlines()
    keys = [int(k) for k in a.keys.split(",")]
    lines = build_script(model, boot, cap, keys)
    a.out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(lines)} words -> {a.out}")
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("model", choices=("farfisa", "bass"))
    b.add_argument("boot", type=Path)
    b.add_argument("capture", type=Path)
    b.add_argument("out", type=Path)
    b.add_argument("--keys", default="60,64,67")
    r = sub.add_parser("run")
    r.add_argument("image")
    r.add_argument("script", type=Path)
    r.add_argument("--exe", default="sharc-pass")
    r.add_argument("--passes", type=int, default=3000)
    ns = ap.parse_args(argv)
    if ns.cmd == "build":
        return cmd_build(ns)
    return fb.main(["run", ns.image, str(ns.script), "--exe", ns.exe, "--passes", str(ns.passes)])


if __name__ == "__main__":
    sys.exit(main())

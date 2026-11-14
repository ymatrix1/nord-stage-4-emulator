#!/usr/bin/env python3
"""Decode the Synth arpeggiator / gate pattern stored in `.ns4y` layer blocks and scan a backup.

The `0x32` Layer block holds the pattern as four fields (block-relative bit offsets, MSB first;
derived from the ns4decode map ids `424-3`, `432-7`, `433-3`, `437-3`, `441-3` with
``bit = (byte - 1) * 8 + (bit - 1) - 3000 - block_base``):

| field | offset | width | meaning |
|---|---|---|---|
| `pattern_on` | 138 | 1 | `Edt_Syn_kArpPtnEnable` |
| `length` | 206 | 4 | steps = raw + 2 (`Edt_Syn_kArpPtnLen` 1..15 = steps - 1) |
| `accent` | 210 | 32 | 16 x 2 bit, step n = bits `2n..2n+1` from the LSB; `1` = accent |
| `gate` | 242 | 32 | 16 x 2 bit; ns4decode names `0 E`, `1 q`, `2 Q`, `3 .` (`3` = rest) |
| `pan` | 274 | 32 | 16 x 2 bit; `0 C`, `1 L`, `2 R` |

The factory presets themselves (`Edt_Syn_kArpPtnPreset`, 0..28) are not stored in a patch, only the
resulting words. Evidence and the firmware search: `docs/research/synth-arp-patterns.md`.

    python tools/ns4_arp_patterns.py "<backup>.ns4b"
"""

from __future__ import annotations

import argparse
import collections
import sys
import zipfile
from pathlib import Path

import ns4_bitscan as bs

PATTERN_ON = 138
LENGTH = (206, 4)
ACCENT = (210, 32)
GATE = (242, 32)
PAN = (274, 32)
GATE_CHARS = "EqQ."
ACCENT_CHARS = ".>.."
PAN_CHARS = "CLR."
LAYER_NAMES = "ABC"


def _field(block: str, spec: tuple[int, int]) -> int:
    start, width = spec
    return int(block[start : start + width], 2)


def digits(word: int, chars: str, count: int = 16) -> str:
    """Spells the first `count` 2-bit steps of `word` (step 0 = least significant pair)."""
    return "".join(chars[(word >> (2 * i)) & 3] for i in range(count))


def decode_pattern(block: str) -> dict | None:
    """Pattern of one 408-bit layer block, or `None` when the pattern mode is off."""
    if block[PATTERN_ON] != "1":
        return None
    steps = _field(block, LENGTH) + 2
    return {
        "steps": steps,
        "gate": digits(_field(block, GATE), GATE_CHARS, steps),
        "accent": digits(_field(block, ACCENT), ACCENT_CHARS, steps),
        "pan": digits(_field(block, PAN), PAN_CHARS, steps),
    }


def scan(presets: dict[str, bytes]) -> list[tuple[str, str, dict]]:
    """Every enabled pattern of a `name -> synth payload` map as `(preset, layer, pattern)`."""
    rows = []
    for name, payload in sorted(presets.items()):
        for i, block in enumerate(bs.synth_blocks(payload, "layer")):
            pattern = decode_pattern(block)
            if pattern:
                rows.append((name, LAYER_NAMES[i], pattern))
    return rows


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("path", type=Path, help=".ns4b backup or directory of .ns4y files")
    ns = ap.parse_args(argv)
    try:
        rows = scan(bs.load_presets(ns.path))
    except (OSError, zipfile.BadZipFile, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    count = collections.Counter((p["steps"], p["gate"], p["accent"], p["pan"]) for _, _, p in rows)
    print(f"{len(rows)} enabled patterns, {len(count)} distinct")
    for (steps, gate, acc, pan), n in count.most_common():
        print(f"{n:3d}x  steps {steps:2d}  gate {gate}  accent {acc}  pan {pan}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

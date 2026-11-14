#!/usr/bin/env python3
"""Decode the arpeggiator / gate mode fields of the `.ns4y` Synth `0x32` Layer block.

Block-relative bit offsets (MSB first), derived from the ns4decode map ids (`423-8` run, `424-1`
mode, `424-3` pattern, `424-4` KB sync, `424-5` range, `428-4` direction, `428-6` zigzag, `428-7`
MST CLK, `428-8` rate) with the same base as `ns4_arp_patterns.py`
(`bit = (byte - 1) * 8 + (bit - 1) - 3000 - block_base`; run lands on 135, rate on 175):

| field | offset | width | meaning |
|---|---|---|---|
| `run` | 135 | 1 | `Edt_Syn_kArpRun` |
| `mode` | 136 | 2 | `0 Arp`, `1 Poly`, `2 Gate` (ns4names `ArpMode`) |
| `pattern_on` | 138 | 1 | `Edt_Syn_kArpPtnEnable` |
| `kb_sync` | 139 | 1 | `Edt_Syn_kArpKbdSync` |
| `range` | 140 | 7 | `Edt_Syn_kArpRange` raw (octave range, scale not decoded here) |
| `direction` | 171 | 2 | `0 UP`, `1 DOWN`, `2 UP/DOWN`, `3 RANDOM` |
| `zigzag` | 173 | 1 | `Edt_Syn_kArpZigZag` |
| `mst_clk` | 174 | 1 | `Edt_Syn_kArpClkEnable` (the rate is a Master Clock subdivision) |
| `rate` | 175 | 7 | rate raw (BPM table, or subdivision when `mst_clk`) |

`python tools/ns4_arp_modes.py "<backup>.ns4b"` prints how often every value occurs and checks the
plausibility rules (mode and direction in range, a Gate or Poly layer has the run switch on in
programs that use it). The numbers go into `docs/research/synth-arp-patterns.md` section 8.
"""

from __future__ import annotations

import argparse
import collections
import sys
import zipfile
from pathlib import Path

import ns4_bitscan as bs

FIELDS: dict[str, tuple[int, int]] = {
    "run": (135, 1),
    "mode": (136, 2),
    "pattern_on": (138, 1),
    "kb_sync": (139, 1),
    "range": (140, 7),
    "direction": (171, 2),
    "zigzag": (173, 1),
    "mst_clk": (174, 1),
    "rate": (175, 7),
}
MODE_NAMES = {0: "Arp", 1: "Poly", 2: "Gate"}
DIRECTION_NAMES = {0: "UP", 1: "DOWN", 2: "UP/DOWN", 3: "RANDOM"}
LAYER_NAMES = "ABC"


def decode_modes(block: str) -> dict[str, int]:
    """Every field of one 408-bit layer block as an integer."""
    return {name: int(block[start : start + width], 2) for name, (start, width) in FIELDS.items()}


def scan(presets: dict[str, bytes]) -> list[tuple[str, str, dict[str, int]]]:
    """`(preset, layer, fields)` of every layer of a `name -> synth payload` map."""
    rows = []
    for name, payload in sorted(presets.items()):
        for i, block in enumerate(bs.synth_blocks(payload, "layer")):
            rows.append((name, LAYER_NAMES[i], decode_modes(block)))
    return rows


def violations(rows: list[tuple[str, str, dict[str, int]]]) -> list[str]:
    """Rule breaks that would mean the offsets or the value names are wrong."""
    problems = []
    for name, layer, f in rows:
        if f["mode"] not in MODE_NAMES:
            problems.append(f"{name} {layer}: mode {f['mode']} has no name")
    return problems


def summary(rows: list[tuple[str, str, dict[str, int]]]) -> list[str]:
    """Histogram lines of the small fields and the min/max of the wide ones."""
    out = [f"{len(rows)} layers"]
    for field in ("run", "mode", "direction", "zigzag", "mst_clk", "kb_sync", "pattern_on"):
        count = collections.Counter(f[field] for _, _, f in rows)
        names = MODE_NAMES if field == "mode" else DIRECTION_NAMES if field == "direction" else {}
        text = ", ".join(f"{names.get(k, k)}: {v}" for k, v in sorted(count.items()))
        out.append(f"{field:10s} {text}")
    for field in ("range", "rate"):
        values = sorted({f[field] for _, _, f in rows})
        out.append(f"{field:10s} {len(values)} distinct, min {values[0]}, max {values[-1]}")
    running = collections.Counter((f["run"], f["mode"]) for _, _, f in rows)
    out.append("(run, mode): " + ", ".join(f"{k}: {v}" for k, v in sorted(running.items())))
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("path", type=Path, help=".ns4b backup or directory of .ns4y files")
    ns = ap.parse_args(argv)
    try:
        rows = scan(bs.load_presets(ns.path))
    except (OSError, zipfile.BadZipFile, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    print("\n".join(summary(rows)))
    problems = violations(rows)
    print("\n".join(problems) or "all layers: mode names ok")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Filter type link table: file type -> strip word 1 stage id (`arm_synth_filter_probe.py` runs).

Each `*.load.json` written by `tools/arm_synth_filter_probe.py` holds, per synth layer, the strip
type selector `[strip + 0x74]` read from the real firmware after the recall of a factory program
(`fields`) and the word 1 the ORG core 2 strip received (`org2_words`, the last write per word).
This tool checks `word 1 low 24 bits == STAGE_TABLE[selector]` and writes the rows as CSV
(`docs/research/data/synth-filter-type-link.csv`, derived numbers only).

    python tools/synth_filter_type_link.py target/synth_filter/p?.load.json \
        --csv docs/research/data/synth-filter-type-link.csv
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path

# VERIFIED (firmware bank0_CODE@0x2F72F0, the 7-entry table the word-1 sender 0xC01CCF84 indexes,
# docs/research/synth-filter-links.md s1): selector 0..6 to the ORG core 2 stage entry. Selector 6
# is the null stage used when the filter is off.
STAGE_TABLE = (0x1253BD, 0x12566E, 0x12549D, 0x1257B2, 0x125913, 0x125A50, 0x125B79)
STAGE_NAMES = ("LP12", "LP24", "Ladder24", "type3", "BP", "HP", "off")
LAYER_STRIPS = ((0, 6), (2, 8), (4, 10))
COLUMNS = ("program", "layer", "selector", "drive", "index", "word1", "stage", "expected", "match")


def link_rows(doc: dict) -> list[dict[str, object]]:
    """One row per synth layer of a probe `*.load.json`: selector, word 1 stage and the check."""
    rows = []
    for layer, fields in enumerate(doc["load"]["fields"]):
        word1 = int(doc["load"]["org2_words"][str(layer)][f"s{LAYER_STRIPS[layer][0]}w1"], 16)
        sel = fields["selector"]
        stage = word1 & 0xFFFFFF
        expected = STAGE_TABLE[sel]
        rows.append(
            {
                "program": doc["program"],
                "layer": "ABC"[layer],
                "selector": sel,
                "drive": fields["drive"],
                "index": fields["index"],
                "word1": f"{word1:#010x}",
                "stage": f"{stage:#x}",
                "expected": f"{expected:#x}",
                "match": int(stage == expected),
            }
        )
    return rows


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("loads", nargs="+", type=Path)
    ap.add_argument("--csv", type=Path)
    ns = ap.parse_args(argv)
    rows = [r for p in ns.loads for r in link_rows(json.loads(p.read_text("utf-8")))]
    ok = sum(int(r["match"]) for r in rows)
    print(f"{ok} of {len(rows)} layers: word 1 stage = STAGE_TABLE[selector]")
    if ns.csv:
        with ns.csv.open("w", newline="", encoding="utf-8") as fh:
            w = csv.DictWriter(fh, fieldnames=COLUMNS, lineterminator="\n")
            w.writeheader()
            w.writerows(rows)
    return 0 if ok == len(rows) else 1


if __name__ == "__main__":
    sys.exit(main())

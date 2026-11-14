#!/usr/bin/env python3
"""Summarise the ARM -> DSP messages of the synth edit ids (`Edt_Syn*`, `Edt_SynCom*`).

`arm_edit_capture.py` drives every value of an edit id through the real layer client of the booted
host firmware and logs the DSP messages (`rows_csv` format). The synth ids do not talk to the SY
image: they write the **channel strips** `DM(0xE0000 + 62 k)` of the Core2 stage engine (ORG core 1,
ORG core 2 and PNO core 2 receive the same words) plus the 32-word common block of each layer at
`DM(0xE02E8 + 32 L)`. This tool folds the capture into one row per (layer, id, DM word):

    uv run --with unicorn --with capstone python tools/arm_edit_capture.py \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
        --cache target/settled.snap --filter "Edt_Syn" --layers 4 --csv syn_l4.csv
    python tools/synth_strips.py syn_l4.csv --out docs/research/data/synth-edit-messages.csv

Evidence and reading: `docs/research/synth-voice-control.md`.
"""

from __future__ import annotations

import argparse
import csv
import io
import struct
import sys
from collections import defaultdict
from pathlib import Path

# VERIFIED (`docs/research/dsp-rx-protocol.md` section 3, ARM check `cmp r3, #0xb`): 12 channel
# strips of 62 words at DM(0xE0000 + 62 k); the common blocks start behind them at 0xE02E8.
STRIP_BASE = 0xE0000
STRIP_WORDS = 62
STRIP_COUNT = 12
COMMON_BASE = STRIP_BASE + STRIP_COUNT * STRIP_WORDS  # 0xE02E8
COMMON_WORDS = 32
# EMPIRICAL (`arm_edit_capture.py` run, `arm_organ_messages.md` section 10): the dispatcher clients
# 4, 5, 6 are the synth layers A, B, C.
LAYER_NAMES = {4: "A", 5: "B", 6: "C"}


def word_location(dm: int) -> tuple[str, int, int]:
    """`(kind, strip_or_layer, word)` of a DM word: kind `strip`, `common` or `other`."""
    if STRIP_BASE <= dm < COMMON_BASE:
        k, w = divmod(dm - STRIP_BASE, STRIP_WORDS)
        return "strip", k, w
    if COMMON_BASE <= dm < COMMON_BASE + 3 * COMMON_WORDS:
        k, w = divmod(dm - COMMON_BASE, COMMON_WORDS)
        return "common", k, w
    return "other", 0, dm


def as_float(word: int) -> float:
    """The IEEE float32 whose bit pattern is `word`."""
    return struct.unpack("<f", struct.pack("<I", word & 0xFFFFFFFF))[0]


def parse_rows(text: str) -> list[dict[str, str]]:
    """Rows of an `arm_edit_capture.rows_csv` file."""
    return list(csv.DictReader(io.StringIO(text)))


def summarize(rows: list[dict[str, str]]) -> list[dict[str, str]]:
    """One row per (layer, id name, chip set, DM word) with the value -> word dependence.

    `value_dependent` is `yes` when the word changes with the edit value; `samples` lists up to
    five `value:word` pairs (the first word seen for each value).
    """
    groups: dict[tuple[str, str, str, int], dict[int, set[int]]] = defaultdict(dict)
    chips: dict[tuple[str, str, int], set[str]] = defaultdict(set)
    for r in rows:
        if r.get("stage") == "error":
            continue
        layer = LAYER_NAMES.get(int(r["layer"]), r["layer"])
        dm = int(r["dm_word"], 0)
        chip = f"{r['chip']}{r['core']}"
        chips[(layer, r["name"], dm)].add(chip)
        key = (layer, r["name"], r["id"], dm)
        groups[key].setdefault(int(r["value"]), set()).add(int(r["w2"], 0))
    out = []
    for (layer, name, eid, dm), by_value in sorted(
        groups.items(), key=lambda kv: (kv[0][0], kv[0][2], kv[0][3])
    ):
        kind, k, w = word_location(dm)
        words = {v: sorted(ws) for v, ws in by_value.items()}
        distinct = {tuple(ws) for ws in words.values()}
        values = sorted(words)
        step = max(1, len(values) // 5)
        picks = values[::step][:5]
        out.append(
            {
                "layer": layer,
                "id": eid,
                "name": name,
                "chips": " ".join(sorted(chips[(layer, name, dm)])),
                "dm_word": f"{dm:#x}",
                "where": f"{kind} {k} word {w}" if kind != "other" else "other",
                "value_dependent": "yes" if len(distinct) > 1 else "no",
                "samples": ";".join(f"{v}:{words[v][0]:#010x}" for v in picks),
                "float_first": f"{as_float(words[values[0]][0]):.9g}",
            }
        )
    return out


def summary_csv(summary: list[dict[str, str]]) -> str:
    """CSV text of `summarize` output."""
    if not summary:
        return ""
    buf = io.StringIO()
    writer = csv.DictWriter(buf, fieldnames=list(summary[0]), lineterminator="\n")
    writer.writeheader()
    writer.writerows(summary)
    return buf.getvalue()


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("capture", type=Path, nargs="+", help="rows_csv files of arm_edit_capture")
    ap.add_argument("--out", type=Path)
    ns = ap.parse_args(argv)
    rows: list[dict[str, str]] = []
    for p in ns.capture:
        rows += parse_rows(p.read_text(encoding="utf-8"))
    text = summary_csv(summarize(rows))
    if ns.out:
        ns.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Knob -> cutoff word law of the ARM: `Edt_Syn_kFltFreq` (0x134) in the settled firmware.

`tools/arm_edit_capture.py` fed the edit values 0..127 as plain integers, but the layer value cells
hold Q16.16 numbers (`(x + 0x8000) >> 16` in the apply code, `0xC0017B7C` passes the cell pointer
to the cutoff sender `0xC01C56CC`), so the old capture saw position 0 for every value and
reported the id as value independent. With `knob << 16` the sender writes the common-block word 1
of the layer (`DM(0xE02E9)`) that the Core2 pass sums into the strip cutoff word:

    uv run --with unicorn --with capstone python tools/arm_synth_freq_law.py \
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
        --cache target/settled.snap --csv docs/research/data/synth-filter-freq-law.csv

Result (`docs/research/synth-stage-models-10.md` s1): the top byte of the word is the table index
`i = knob - 52` up to knob 117 (steeper above, clipped by the pass at `0x6A000000`); the sender
reads no filter type, so the law is the same for every `kFltType`.
"""

from __future__ import annotations

import argparse
import csv
import io
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

FREQ_ID = 0x134  # VERIFIED: docs/research/data/synth-edit-messages.csv (Edt_Syn_kFltFreq)
COMMON_WORD1 = 0xE02E9  # DM word of common block 0 word 1 (layer A), ORG core 2
LAYER_A = 4  # dispatcher client of synth layer A (tools/synth_strips.py)
KNOB_OFFSET = 52  # VERIFIED run: top byte of the word = knob - 52 for knob 0..117


def word_index(word: int) -> int:
    """Signed table index (top byte) of a Q31 cutoff word."""
    byte = (word >> 24) & 0xFF
    return byte - 256 if byte >= 128 else byte


def law_csv(rows: list[tuple[int, int]]) -> str:
    """CSV text `knob,common_word1` of `(knob, word)` rows."""
    buf = io.StringIO()
    writer = csv.writer(buf, lineterminator="\n")
    writer.writerow(["knob", "common_word1"])
    for knob, word in rows:
        writer.writerow([knob, f"{word:#x}"])
    return buf.getvalue()


def read_law(text: str) -> dict[int, int]:
    """`{knob: word}` of a CSV written by `law_csv`."""
    return {int(r["knob"]): int(r["common_word1"], 0) for r in csv.DictReader(io.StringIO(text))}


def sweep(code: Path, sram: Path, cache: Path | None) -> list[tuple[int, int]]:  # pragma: no cover
    """Dispatches `kFltFreq` for knob 0..127 from the settled state and reads the word."""
    import arm_edit_capture as ec
    import arm_emu as ae
    import arm_steady as ast

    emu, blob = ec.settled_emulator(code, sram, cache)
    if not any(b <= ec.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(ec.SCRATCH, ae.PAGE)
    store = next(s for s in ec.first_store_per_layer(ec.find_stores(emu)) if s.layer == LAYER_A)
    rows = []
    for knob in range(128):
        ast.restore(emu, blob)
        emu.install_dsp_ports()
        during, later, _err = ec.apply_value(emu, store.address, FREQ_ID, knob << 16, 3000)
        words = [m.word2 for m in during + later if m.dm_word == COMMON_WORD1]
        rows.append((knob, words[-1]))
    return rows


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("code", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--csv", type=Path)
    ns = ap.parse_args(argv)
    text = law_csv(sweep(ns.code, ns.sram, ns.cache))
    if ns.csv:
        ns.csv.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())

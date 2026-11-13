#!/usr/bin/env python3
"""Probe which strip word 0 (oscillator stage) the synth Wave edit selects in the ARM harness.

For a grid of `(kMode, kOscBank, kOscCat, kOscWave)` the tool writes mode, bank and category into
the value cells of synth layer A, dispatches `Edt_Syn_kOscWave` through the real layer client
(`arm_edit_capture.apply_value`) and records every strip word 0 / 1 / 59 / 60 the ARM sends:

    uv run --with unicorn python tools/synth_wave_probe.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap \
        --csv docs/research/data/synth-wave-probe.csv

Result (`docs/research/synth-strip-path.md` s6): word 0 is `0x061215FB` for every grid point, so the
oscillator stage of a preset is not chosen by these edit ids in the harness state.
"""

from __future__ import annotations

import argparse
import itertools
import sys
from pathlib import Path

# VERIFIED (`docs/research/synth-voice-control.md` s4): the strip words the oscillator edits write.
STRIP_WORDS = {0xE0000: "w0", 0xE0001: "w1", 0xE003B: "w59", 0xE003C: "w60"}
# VERIFIED (`docs/research/arm-edit-params.csv`): edit ids of the oscillator selection.
IDS = {"mode": 0xFA, "bank": 0x125, "cat": 0x126, "wave": 0x127}
DEFAULT_GRID = {
    "mode": [0, 1],
    "bank": [0, 1, 2, 3],
    "cat": [0, 3, 7],
    "wave": [0, 6, 127],
}


def fold_words(messages: list[tuple[int, int]]) -> dict[str, str]:
    """Last value per strip word name of `(dm_word, value)` messages, hex strings."""
    out: dict[str, str] = {}
    for dm, value in messages:
        name = STRIP_WORDS.get(dm)
        if name:
            out[name] = f"{value & 0xFFFFFFFF:#010x}"
    return out


def distinct_values(rows: list[dict[str, str]], word: str = "w0") -> set[str]:
    """Distinct values of one strip word over all grid rows (empty cells skipped)."""
    return {r[word] for r in rows if r.get(word)}


def rows_csv(rows: list[dict[str, str]]) -> str:
    cols = ["mode", "bank", "cat", "wave", "w0", "w1", "w59", "w60"]
    out = [",".join(cols)]
    out += [",".join(str(r.get(c, "")) for c in cols) for r in rows]
    return "\n".join(out) + "\n"


def run_grid(args: argparse.Namespace) -> list[dict[str, str]]:  # pragma: no cover - needs unicorn
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import arm_edit_capture as c
    import arm_emu as ae
    import arm_steady as ast

    emu, blob = c.settled_emulator(args.image, args.sram, args.cache)
    stores = c.first_store_per_layer(c.find_stores(emu))
    store = next(s for s in stores if s.layer == 4)
    if not any(b <= c.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(c.SCRATCH, ae.PAGE)
    rows = []
    for mode, bank, cat, wave in itertools.product(*DEFAULT_GRID.values()):
        ast.restore(emu, blob)
        emu.install_dsp_ports()
        for name, v in (("mode", mode), ("bank", bank), ("cat", cat)):
            emu.write32(c.value_cell(emu, store.address, IDS[name]), v)
        during, later, _err = c.apply_value(emu, store.address, IDS["wave"], wave, args.window)
        words = fold_words([(m.dm_word, m.word2) for m in during + later])
        rows.append({"mode": mode, "bank": bank, "cat": cat, "wave": wave, **words})
        print(rows[-1], flush=True)
    return rows


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--window", type=int, default=100_000)
    ap.add_argument("--csv", type=Path)
    ns = ap.parse_args(argv)
    rows = run_grid(ns)
    if ns.csv:
        ns.csv.write_text(rows_csv(rows), encoding="utf-8")
    print(f"distinct word 0 values: {sorted(distinct_values(rows))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

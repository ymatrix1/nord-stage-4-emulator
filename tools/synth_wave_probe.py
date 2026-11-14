#!/usr/bin/env python3
"""Probe which strip stage (oscillator type) the synth Wave edit selects in the ARM harness.

For a grid of `(kMode, kOscBank, kOscCat, kOscWave)` the tool writes mode, bank and category into
the value cells of synth layer A, dispatches `Edt_Syn_kOscWave` through the real layer client
(`arm_edit_capture.apply_value`) and records the strip words the ARM sends plus the **pending type
index** of the strip object (`[strip + 0x98]`, copied to `[strip + 0xC8]` by `0xC01CE7D8`, which the
harness does not reach, so word 0 itself stays constant). `STAGE_TABLE[type]` is the stage id:

    uv run --with unicorn python tools/synth_wave_probe.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap \
        --csv docs/research/data/synth-wave-probe.csv

Result (`docs/research/synth-wave-stages.md`): bank 0 (ANALOG) category `c`, wave `w` gives the type
index in `data/synth-wave-types.csv`; word 0 sent in the harness stays `0x061215FB` (type 2).
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
# VERIFIED (`docs/research/pno-module-chain.md` s5): stage ids of `bank0_CODE@0x2FA050[type]`,
# 74 entries (derived addresses of DSP routines, no firmware bytes).
STAGE_TABLE = (
    0x120CD0, 0x121183, 0x1215FB, 0x121B99, 0x121B99, 0x121B99, 0x125275, 0x1212D3, 0x12169D,
    0x121AB5, 0x12179A, 0x121867, 0x1219C3, 0x1232A5, 0x125275, 0x125275, 0x121C54, 0x121C54,
    0x121C54, 0x121C54, 0x121C54, 0x121C54, 0x121D6D, 0x121E9D, 0x124985, 0x124985, 0x124985,
    0x12255D, 0x1226DD, 0x122A32, 0x122C5C, 0x122C5C, 0x122C5C, 0x1239BE, 0x123C7C, 0x123FE9,
    0x122F1F, 0x1230DF, 0x123427, 0x1235C9, 0x125275, 0x12377D, 0x120DAB, 0x120F55, 0x12145E,
    0x122417, 0x121FD8, 0x122030, 0x122151, 0x124269, 0x12453F, 0x12453F, 0x12453F, 0x124269,
    0x1247B6, 0x124B6A, 0x1247B6, 0x124B6D, 0x125281, 0x12528D, 0x125299, 0x1252A5, 0x1252B1,
    0x1252BD, 0x1252C9, 0x1252D5, 0x1252E1, 0x1252ED, 0x1252F9, 0x125305, 0x125311, 0x12531D,
    0x125329, 0x125335,
)  # fmt: skip
MARKER_TYPE = 0x39  # `kMode = 0`: pending marker that triggers the apply call `0xC01CE7D8`
DEFAULT_TYPE = 2  # the type index an out-of-range wave falls back to
TYPE_OFFSET = 0x98  # pending type index inside the strip object
SENDER = 0xC01CC85C  # strip word 0 sender: `r0` = strip object
# kMode 1 = oscillator modes (bank 0 ANALOG), kOscBank 0..3, category 0..7, wave 0..13.
DEFAULT_GRID = {
    "mode": [1],
    "bank": [0],
    "cat": list(range(8)),
    "wave": list(range(14)),
}


def stage_of(type_index: int) -> int | None:
    """Stage id of a type index; None outside the table and for the state marker 0x39."""
    if type_index == MARKER_TYPE or not 0 <= type_index < len(STAGE_TABLE):
        return None
    return STAGE_TABLE[type_index]


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
    cols = ["mode", "bank", "cat", "wave", "type", "stage", "w0", "w1", "w59", "w60"]
    out = [",".join(cols)]
    out += [",".join(str(r.get(c, "")) for c in cols) for r in rows]
    return "\n".join(out) + "\n"


def run_grid(args: argparse.Namespace) -> list[dict[str, str]]:  # pragma: no cover - needs unicorn
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import arm_edit_capture as c
    import arm_emu as ae
    import arm_steady as ast
    import unicorn

    emu, blob = c.settled_emulator(args.image, args.sram, args.cache)
    stores = c.first_store_per_layer(c.find_stores(emu))
    store = next(s for s in stores if s.layer == 4)
    if not any(b <= c.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(c.SCRATCH, ae.PAGE)
    obj: list[int] = []
    emu.uc.hook_add(
        unicorn.UC_HOOK_CODE,
        lambda uc, *_: obj.append(uc.reg_read(unicorn.arm_const.UC_ARM_REG_R0)),
        begin=SENDER,
        end=SENDER,
    )
    rows = []
    for mode, bank, cat, wave in itertools.product(*DEFAULT_GRID.values()):
        ast.restore(emu, blob)
        emu.install_dsp_ports()
        for name, v in (("mode", mode), ("bank", bank), ("cat", cat)):
            emu.write32(c.value_cell(emu, store.address, IDS[name]), v)
        during, later, _err = c.apply_value(emu, store.address, IDS["wave"], wave, args.window)
        words = fold_words([(m.dm_word, m.word2) for m in during + later])
        t = emu.read32(obj[-1] + TYPE_OFFSET) if obj else -1
        st_id = stage_of(t)
        rows.append(
            {
                "mode": mode,
                "bank": bank,
                "cat": cat,
                "wave": wave,
                "type": t,
                "stage": f"{st_id:#x}" if st_id else "",
                **words,
            }
        )
        print(rows[-1], flush=True)
    return rows


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--window", type=int, default=2_000)
    ap.add_argument("--csv", type=Path)
    ns = ap.parse_args(argv)
    rows = run_grid(ns)
    if ns.csv:
        ns.csv.write_text(rows_csv(rows), encoding="utf-8")
    print(f"distinct word 0 values: {sorted(distinct_values(rows))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

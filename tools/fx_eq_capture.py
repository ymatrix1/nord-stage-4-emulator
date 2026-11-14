#!/usr/bin/env python3
"""Full-grid ARM capture of the Amp EQ ids (`kAmpTreble`, `kAmpMid`, `kAmpBass`, `kAmpMidFreq`).

`fx_clients.py --slots` samples each id at 11 to 13 values with the others at their default. The
firmware EQ needs two more things: the id at **every** value (the mid sweep has 128 steps) and the
**joint** setting of mid gain and mid frequency (the series show both ids writing the same words).
This tool drives the units of the booted firmware (`docs/research/fx-contexts.md` s2) and writes
one row per DSP word that the last message of a setting left in a `(chip, core, path, dm_word)`.

    uv run --with unicorn --with capstone python tools/fx_eq_capture.py \\
        target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \\
        --cache target/settled.snap --units 4 --csv eq-capture.csv

Output columns: `unit,kind,mode,gain,freq,dm_word,word` where `mode` is `treble`, `bass`, `mid`
(gain sweep at the default frequency), `freq` (frequency sweep at gain 0) or `joint` (frequency
first, then gain). Evidence and limits: `docs/research/fx-eq-words.md`.
"""

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))

#: Edit ids of the four EQ controls.
IDS = {
    "treble": "Edt_Effects_kAmpTreble",
    "bass": "Edt_Effects_kAmpBass",
    "mid": "Edt_Effects_kAmpMid",
    "freq": "Edt_Effects_kAmpMidFreq",
}
GAIN_RANGE = range(-60, 61)
FREQ_RANGE = range(0, 128)
JOINT_GAINS = (-60, -30, 30, 60)
JOINT_FREQS = (0, 32, 96, 127)
FIELDS = ["unit", "kind", "mode", "gain", "freq", "dm_word", "word"]


def joint_grid() -> list[tuple[int, int]]:
    """`(freq, gain)` pairs of the joint capture, frequency applied first."""
    return [(f, g) for f in JOINT_FREQS for g in JOINT_GAINS]


def last_words(messages) -> dict[int, int]:  # noqa: ANN001
    """Last word written to each DSP address by a list of messages (later messages win)."""
    out: dict[int, int] = {}
    for m in messages:
        out[m.dm_word] = m.word2
    return out


def rows_for(unit, kind, mode, gain, freq, words):  # noqa: ANN001
    """CSV rows of one setting."""
    return [
        {
            "unit": unit,
            "kind": kind,
            "mode": mode,
            "gain": gain,
            "freq": freq,
            "dm_word": f"{a:#x}",
            "word": f"{w:#010x}",
        }
        for a, w in sorted(words.items())
    ]


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the booted firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--params", type=Path, default=Path("docs/research/arm-edit-params.csv"))
    ap.add_argument("--units", default="4")
    ap.add_argument("--window", type=int, default=300_000)
    ap.add_argument("--csv", type=Path, required=True)
    ns = ap.parse_args(argv)
    import arm_edit_capture as cap
    import arm_steady as ast
    import fx_clients as fc

    emu, blob = fc.fx_snapshot(ns.image, ns.sram, ns.cache)
    all_params = cap.load_params(ns.params, ".")
    markers = {p.name: p.edit_id for p in all_params if p.name in fc.KIND_MARKERS}
    units = fc.find_units(emu, markers)
    wanted = {int(x, 0) for x in ns.units.split(",")}
    by_name = {p.name: p for p in cap.load_params(ns.params, "kAmp")}
    out: list[dict] = []

    def apply(unit, steps):  # noqa: ANN001, ANN202
        ast.restore(emu, blob)
        emu.install_dsp_ports()
        msgs = []
        for name, v in steps:
            p = by_name[IDS[name]]
            during, later, _err = cap.apply_value(
                emu, unit.address, p.edit_id, fc.cell_word(p, v), ns.window
            )
            msgs += during + later
        return last_words(msgs)

    for u in (u for u in units if u.index in wanted):
        for mode in ("treble", "bass", "mid"):
            for g in GAIN_RANGE:
                out += rows_for(u.index, u.kind, mode, g, "", apply(u, [(mode, g)]))
        for f in FREQ_RANGE:
            out += rows_for(u.index, u.kind, "freq", 0, f, apply(u, [("freq", f)]))
        for f, g in joint_grid():
            out += rows_for(u.index, u.kind, "joint", g, f, apply(u, [("freq", f), ("mid", g)]))
        print(f"unit {u.index} {u.kind}: {len(out)} rows", flush=True)
        with ns.csv.open("w", encoding="utf-8", newline="") as fh:
            w = csv.DictWriter(fh, fieldnames=FIELDS)
            w.writeheader()
            w.writerows(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())

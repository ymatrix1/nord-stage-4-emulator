#!/usr/bin/env python3
"""Capture the ARM -> DSP messages of organ and synth key events (zones enabled by `arm_zones`).

With the program sections enabled (`arm_zones.enable_sections`) a note-on on MIDI channel 1 runs the
organ second-family key-down `0xC00BF890` and the synth voice event `0xC01EC690`. This tool
isolates one layer family (the other zones are switched off with `arm_zones.force_zone`), installs
an organ model (`arm_model_paths.install_model`), sends note-on / note-off through the UART2 model
and writes the DSP messages as CSV (derived values only) and as `sharc-org --msg-script` files.

    uv run --with unicorn python tools/arm_zone_capture.py organ target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap --models 0 1 3 \
        --csv docs/research/data/keydown-organ-keys.csv

Subcommands: `organ`, `synth`, `flags` (Vox key-flag rows), `scripts` (replay scripts),
`velocity` (velocity curve table).
Evidence and tables: `docs/research/keydown-zones.md`.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

import arm_emu as ae
import arm_keydown as kd
import arm_model_paths as mp
import arm_steady as ast
import arm_zones as az

CSV_HEADER = "kind,model,note,velocity,phase,tick,chip,core,path,dm_word,value"
ORGAN_LAYERS = {0, 1}
SYNTH_LAYERS = {4, 5, 6}
# VERIFIED (run): the UI order of `Edt_Org_kModel` 0..5 (`arm_model_paths.NAMES_ADDRESS`).
MODEL_NAMES = ("B3", "Vox", "Farfisa", "Pipe1", "Pipe2", "B3 Bass")
# VERIFIED (run, `keydown-zones.md` section 4): the Vox engine key flag table is
# `[engine + 0x294 + 0x14 * row + slot]` (96 rows, 20 slots); the weight queue drain `0xC0224360`
# loops over the engine in `r5`.
FLAG_OFFSET, FLAG_ROW_STRIDE, FLAG_ROWS, FLAG_SLOTS = 0x294, 0x14, 96, 20
LEVEL_OFFSET, TAPER_OFFSET = 0xD18, 0xA14
WEIGHT_DRAIN = 0xC0224360
SETTLE_TICKS = 10


@dataclass
class Setup:
    """The zone-enabled rig plus the layer objects used to isolate one layer family."""

    rig: kd.Rig
    bases: list[int]
    stores: dict[int, object]
    blob: bytes = b""

    @property
    def emu(self) -> ae.ArmEmu:
        return self.rig.emu

    def keep(self, layers: set[int]) -> None:
        """Enable the zones of `layers` only (layers 0..6); the others stay silent."""
        for i, base in enumerate(self.bases[:7]):
            az.force_zone(self.emu, base, i in layers)

    def reset(self) -> None:
        ast.restore(self.emu, self.blob)
        self.emu.install_dsp_ports()
        ast.enable_real_timer(self.emu)
        self.emu.clear_messages()

    def freeze(self) -> None:
        self.blob = ast.snapshot(self.emu)


def prepare(code: Path, sram: Path | None, cache: Path | None) -> Setup:
    rig = kd.build_rig(code, sram, cache)
    az.enable_sections(rig.emu)
    setup = Setup(rig, az.layer_bases(rig.emu), az.first_stores(rig.emu))
    rig.tick(2)
    return setup


def rows_for(
    kind: str, model: int, note: int, velocity: int, phase: str, msgs: list[tuple[int, ae.Message]]
) -> list[str]:
    return [
        f"{kind},{model},{note},{velocity},{phase},{t},{m.chip},{m.core},{m.path},"
        f"{m.dm_word:#x},{m.word2:#010x}"
        for t, m in msgs
    ]


def install_organ_model(setup: Setup, model: int) -> list[tuple[int, ae.Message]]:
    """Apply `Edt_Org_kModel = model` on organ layer A; returns the install burst."""
    emu = setup.emu
    emu.clear_messages()
    mp.install_model(emu, setup.stores[0], model, ticks=0)  # type: ignore[arg-type]
    burst = [(0, m) for m in emu.messages]
    burst += setup.rig.tick(SETTLE_TICKS)
    return burst


def key_run(
    setup: Setup,
    kind: str,
    model: int,
    note: int,
    velocity: int,
    on_ticks: int = 120,
    off_ticks: int = 120,
    notes: tuple[int, ...] = (),
) -> tuple[list[str], list[tuple[int, ae.Message]], list[tuple[int, ae.Message]]]:
    """Note-on (and the extra `notes`), `on_ticks`, note-off, `off_ticks` from the frozen state."""
    setup.reset()
    rig = setup.rig
    for n in (note, *notes):
        rig.send(kd.note_on(n, velocity))
    on = rig.tick(on_ticks)
    for n in (note, *notes):
        rig.send(kd.note_off(n))
    off = rig.tick(off_ticks)
    rows = rows_for(kind, model, note, velocity, "on", on) + rows_for(
        kind, model, note, velocity, "off", off
    )
    return rows, on, off


def sweep_points(
    notes: tuple[int, ...], velocities: tuple[int, ...], fixed_note: int, fixed_velocity: int
) -> list[tuple[int, int]]:
    """Notes at the fixed velocity, then velocities at the fixed note (no duplicates)."""
    points = [(n, fixed_velocity) for n in notes]
    points += [(fixed_note, v) for v in velocities if (fixed_note, v) not in points]
    return points


def engine_flags(emu: ae.ArmEmu, engine: int) -> list[tuple[int, int]]:
    """`(row, slot)` of every non-zero key flag byte of a voice engine."""
    raw = emu.read_bytes(engine + FLAG_OFFSET, FLAG_ROWS * FLAG_ROW_STRIDE)
    return [
        (r, s) for r in range(FLAG_ROWS) for s in range(FLAG_SLOTS) if raw[r * FLAG_ROW_STRIDE + s]
    ]


def engine_levels(emu: ae.ArmEmu, engine: int) -> list[int]:
    """The 20 level slots (`+0xD18 + 8 j`), 23-bit fixed point: weight = value / 2**23."""
    return [emu.read32(engine + LEVEL_OFFSET + 8 * j) for j in range(FLAG_SLOTS)]


def find_engine(setup: Setup) -> int:
    """Voice engine that the weight queue drain serves: hook `0xC0224360`, play one note."""
    found: list[int] = []

    def hook(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        found.append(setup.emu.reg(5))

    h = setup.emu.uc.hook_add(setup.emu._u.UC_HOOK_CODE, hook, begin=WEIGHT_DRAIN, end=WEIGHT_DRAIN)
    setup.reset()
    setup.rig.send(kd.note_on(60, 100))
    setup.rig.tick(20)
    setup.emu.uc.hook_del(h)
    if not found:
        raise SystemExit("weight drain 0xC0224360 was not reached")
    return found[0]


def flag_table(setup: Setup, engine: int, notes: tuple[int, ...]) -> list[str]:
    """CSV `note,row,slot` of the key flag rows set by a key-down of each note."""
    out = ["note,row,slot"]
    for note in notes:
        setup.reset()
        setup.rig.send(kd.note_on(note, 100))
        setup.rig.tick(4)
        out += [f"{note},{r},{s}" for r, s in engine_flags(setup.emu, engine)]
    return out


def msg_script(blocks: list[tuple[int, str, list[tuple[int, ae.Message]]]], core: int = 1) -> str:
    """`sharc-org --msg-script` text: `PASS DM VALUE  # comment` per message of `core`."""
    lines = []
    for pass_no, label, msgs in blocks:
        for t, m in msgs:
            if m.chip == "PNO" and m.core == core:
                lines.append(f"{pass_no} {m.dm_word:#x} {m.word2:#010x}  # {label} +{t} ms")
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("command", choices=["organ", "synth", "flags", "scripts", "velocity"])
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--models", type=int, nargs="*", default=[0, 1, 3])
    ap.add_argument("--csv", type=Path)
    ap.add_argument("--out", type=Path, help="output directory of `scripts`")
    ap.add_argument("--short", action="store_true", help="five points only (survey of a model)")
    ns = ap.parse_args(argv)
    notes = (
        21,
        24,
        28,
        33,
        36,
        40,
        45,
        48,
        52,
        57,
        60,
        61,
        64,
        69,
        72,
        76,
        81,
        84,
        88,
        93,
        96,
        100,
    )
    notes += (105, 108)
    velocities = (1, 16, 32, 48, 64, 80, 100, 127)
    if ns.short:
        notes, velocities = (36, 60, 84), (32, 127)
    setup = prepare(ns.image, ns.sram, ns.cache)
    lines: list[str] = []
    if ns.command == "velocity":
        text = "\n".join(az.velocity_csv(setup.emu)) + "\n"
        ns.csv.write_text(text, encoding="utf-8")
        return 0
    if ns.command == "organ":
        setup.keep(ORGAN_LAYERS)
        for model in ns.models:
            install_organ_model(setup, model)
            setup.freeze()
            for note, vel in sweep_points(notes, velocities, 60, 100):
                lines += key_run(setup, "organ", model, note, vel)[0]
                print(f"model {model} note {note} velocity {vel}: {len(lines)} rows", flush=True)
    elif ns.command == "synth":
        for layers, kind in (({4}, "synthA"), ({5}, "synthB"), ({6}, "synthC")):
            setup.keep(layers)
            setup.freeze()
            for note, vel in sweep_points(notes, velocities, 60, 100):
                lines += key_run(setup, kind, 0, note, vel, 60, 60)[0]
                print(f"{kind} note {note} velocity {vel}: {len(lines)} rows", flush=True)
    elif ns.command == "flags":
        setup.keep(ORGAN_LAYERS)
        install_organ_model(setup, 1)
        setup.freeze()
        engine = find_engine(setup)
        print(f"Vox engine {engine:#x}")
        lines = flag_table(setup, engine, notes)
        if ns.csv:
            ns.csv.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print("\n".join(lines[:5]))
        return 0
    else:
        ns.out.mkdir(parents=True, exist_ok=True)
        for model, name in ((0, "b3"), (1, "vox"), (3, "pipe")):
            setup.keep(ORGAN_LAYERS)
            setup.reset()
            install = install_organ_model(setup, model)
            setup.freeze()
            _rows, on, off = key_run(setup, "organ", model, 60, 100, notes=(64, 67))
            text = msg_script(
                [(0, "model install", install), (1, "C4 E4 G4 on", on), (2, "off", off)]
            )
            (ns.out / f"keydown-{name}-c4e4g4.script").write_text(text, encoding="utf-8")
        setup.keep(SYNTH_LAYERS)
        setup.freeze()
        _rows, on, off = key_run(setup, "synth", 0, 60, 100, 60, 60, notes=(64, 67))
        text = msg_script([(1, "C4 E4 G4 on", on), (2, "off", off)], core=2)
        (ns.out / "keydown-synth-c4e4g4.script").write_text(text, encoding="utf-8")
        return 0
    if ns.csv:
        ns.csv.write_text(CSV_HEADER + "\n" + "\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(lines)} rows")
    return 0


if __name__ == "__main__":
    sys.exit(main())

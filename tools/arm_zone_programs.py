#!/usr/bin/env python3
"""Factory programs through the firmware zone path: split state and `kKbZone` codes to key ranges.

For every `.ns4p` the tool decodes the split state (`split_enabled`, three point flags,
positions, fade widths) and the seven layer codes, writes them like the edit ids
(`Edt_Com_kSplit*` `0xE7..0xF0`, `Edt_Org / Pno / Syn_kKbZone` `0x161 / 0x14D / 0xF4`) into
the booted firmware, reads the zone object of every layer (range structs at `+4` / `+0x14`)
and the per-note gain of `0xC00C84B8`, and compares
them with the model of `arm_zone_map.py`:

    uv run --with unicorn python tools/arm_zone_programs.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap "factory_programs/Program"

Evidence: `docs/research/zone-key-ranges.md`.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import arm_program_routing as apr
import arm_zone_map as zm

# VERIFIED (arm-edit-params.csv): split edit ids and the per-family `kKbZone` ids.
ID_ENABLED, ID_FLAG, ID_POS, ID_WIDTH = 0xE7, 0xE8, 0xEB, 0xEE
KBZONE_ID = {"organ": 0x161, "piano": 0x14D, "synth": 0xF4}
# Layer index (dispatcher order) to family, and the field offsets of `cbin/kb_zone.rs` FIELDS.
# VERIFIED (arm-edit-params.csv): `Edt_Org / Pno / Syn_kOctaveShift` (-6..6).
OCTAVE_ID = {"organ": 0x162, "piano": 0x14E, "synth": 0xF5}
LAYER_FAMILY = ("organ", "organ", "piano", "piano", "synth", "synth", "synth")
CODE_FIELDS = (
    (45, 144, 8), (45, 392, 8), (180, 104, 8), (180, 200, 8),
    (327, 248, 58), (327, 656, 58), (327, 1064, 58),
)  # fmt: skip


def decode_zone_state(payload: bytes) -> dict:
    """Split state and layer codes of an 820-byte program payload (global block from bit 8)."""
    rd = apr.read_bits
    enabled = bool(rd(payload, 8, 1))
    flags = tuple(bool(rd(payload, 9 + i, 1)) for i in range(3))
    pos = tuple(rd(payload, 12 + 4 * i, 4) for i in range(3))
    widths = tuple(rd(payload, 24 + 2 * i, 2) for i in range(3))
    codes = tuple(rd(payload, sec * 8 + blk + fld, 4) for sec, blk, fld in CODE_FIELDS)
    return {"enabled": enabled, "flags": flags, "pos": pos, "widths": widths, "codes": codes}


def model_ranges(state: dict) -> list:
    """Model `(lower, upper)` per layer; positions above 10 and widths above 2 clamp."""
    pos = tuple(min(p, 10) for p in state["pos"])
    widths = tuple(min(w, 2) for w in state["widths"])
    return [
        zm.zone_range(min(c, 11), state["enabled"], state["flags"], pos, widths)
        for c in state["codes"]
    ]


# VERIFIED (disassembly): the split state object `[0xC059B334]` takes the `Edt_Com_kSplit*`
# ids in the method `0xC01AC844(this, _, _, &id, value on the stack)` (vtable word
# `0xC02B78E8`); `0xE7` and the flag ids store `value == 1` as a byte at `+0xD0 / +0xD1..+0xD3`,
# the position ids the word at `+0xD4 + 4 k`, the width ids the word at `+0xE0 + 4 k`.
COM_HANDLER = 0xC01AC844


def dispatch_com(emu, cfg: int, edit_id: int, value: int) -> None:  # pragma: no cover - firmware
    """Calls `0xC01AC844` with `value` as the fifth argument (first word on the stack)."""
    import arm_edit_capture as aec

    saved = [emu.reg(n) for n in range(16)]
    cpsr = emu.uc.reg_read(emu._u.arm_const.UC_ARM_REG_CPSR)
    sp = (saved[13] - 0x800) & ~7
    emu.write32(aec.SCRATCH, edit_id)
    emu.write32(sp, value)
    emu.set_reg(13, sp)
    emu.set_reg(14, ae_sentinel())
    for i, a in enumerate((cfg, 0, 0, aec.SCRATCH)):
        emu.set_reg(i, a)
    try:
        emu.uc.emu_start(COM_HANDLER, ae_sentinel(), count=100_000)
    finally:
        for n in range(16):
            emu.set_reg(n, saved[n])
        emu.uc.reg_write(emu._u.arm_const.UC_ARM_REG_CPSR, cpsr)


def ae_sentinel() -> int:  # pragma: no cover - firmware
    import arm_emu as ae

    return ae.SENTINEL


def run_program(emu, stores, bases, state: dict) -> list:  # pragma: no cover - needs the firmware
    """Dispatches `state`, returns `[(lower, upper, gains)]` read from the zone objects."""
    import arm_edit_capture as aec
    import arm_steady as ast
    import arm_zones as az

    cfg = emu.read32(0xC059B334)
    dispatch_com(emu, cfg, ID_ENABLED, int(state["enabled"]))
    for i in range(3):
        dispatch_com(emu, cfg, ID_FLAG + i, int(state["flags"][i]))
        dispatch_com(emu, cfg, ID_POS + i, min(state["pos"][i], 10))
        dispatch_com(emu, cfg, ID_WIDTH + i, min(state["widths"][i], 2))
    out = []
    for layer, code in enumerate(state["codes"]):
        store = stores[layer]
        ident = KBZONE_ID[LAYER_FAMILY[layer]]
        # The edit-id dispatch calls the range update only when it sees a change (observed: not
        # when the split state is written twice with the same value), so write the code cell and
        # call the update `0xC010ABA4(store, 0, 0)` itself; it reads the code through the layer.
        emu.write32(aec.value_cell(emu, store.address, ident), code)
        ast.call_preserving(emu, 0xC010ABA4, store.address, 0, 0)
        z = az.zone_address(emu, bases[layer])
        lo = tuple(emu.read32(z + 4 + 4 * i) for i in range(4))
        up = tuple(emu.read32(z + 0x14 + 4 * i) for i in range(4))
        gains = [
            (
                ast.call_preserving(emu, 0xC00C84B8, z, 0, n),
                ast.call_preserving(emu, 0xC00C84B8, z, 1, n),
            )
            for n in range(21, 109)
        ]
        out.append((lo, up, gains))
    return out


def octave_check(emu, stores, bases, state: dict) -> int:  # pragma: no cover - needs the firmware
    """Zone ranges of every layer for octave shifts -2..2 and a transpose; returns the number of
    layers whose range changes (0: the key range is a function of the physical key only)."""
    import arm_edit_capture as aec
    import arm_steady as ast
    import arm_zones as az

    changed = 0
    base = run_program(emu, stores, bases, state)
    for layer, store in sorted(stores.items()):
        if layer >= len(LAYER_FAMILY):
            continue
        ident = OCTAVE_ID[LAYER_FAMILY[layer]]
        for octave in (-2, -1, 1, 2):
            aec.apply_value(emu, store.address, ident, octave & 0xFFFFFFFF, window=100_000)
            ast.call_preserving(emu, 0xC010ABA4, store.address, 0, 0)
            z = az.zone_address(emu, bases[layer])
            now = (
                tuple(emu.read32(z + 4 + 4 * i) for i in range(4)),
                tuple(emu.read32(z + 0x14 + 4 * i) for i in range(4)),
            )
            changed += now != (base[layer][0], base[layer][1])
        aec.apply_value(emu, store.address, ident, 0, window=100_000)
    return changed


def compare(want: tuple, got: tuple) -> bool:
    """`want` = model `(lower, upper)`; `got` = `(lo struct, up struct, gains)` from the ARM."""
    (wl, wu), (gl, gu, gains) = want, got
    keys = (
        wl[0] == gl[0],
        wu[0] == gu[0],
        zm.FADE_KEYS[wl[1]] == gl[1],
        zm.FADE_KEYS[wu[1]] == gu[1],
    )
    if not all(keys):
        return False
    for note, (g_lo, g_up) in zip(range(21, 109), gains, strict=True):
        if g_lo != zm.bound_gain(wl, False, note) or g_up != zm.bound_gain(wu, True, note):
            return False
    return True


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("programs", type=Path, help="directory of .ns4p files (searched recursively)")
    ap.add_argument("--cache", type=Path)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--octave-check", action="store_true", help="octave shifts leave ranges alone")
    ns = ap.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import arm_keydown as kd
    import arm_zones as az

    emu = kd.build_rig(ns.image, ns.sram, ns.cache).emu
    import arm_edit_capture as aec

    if not any(b <= aec.SCRATCH <= e for b, e, _ in emu.uc.mem_regions()):
        emu.uc.mem_map(aec.SCRATCH, 0x1000)
    az.enable_sections(emu)
    stores, bases = az.first_stores(emu), az.layer_bases(emu)
    files = sorted(ns.programs.rglob("*.ns4p"))[: ns.limit or None]
    bad = 0
    if ns.octave_check:
        for path in files:
            state = decode_zone_state(apr.load_payload(path))
            moved = octave_check(emu, stores, bases, state)
            print(f"{path.stem:28s} octave changes {moved}")
            bad += moved
        print("octave range changes:", bad)
        return int(bad > 0)
    for path in files:
        state = decode_zone_state(apr.load_payload(path))
        want, got = model_ranges(state), run_program(emu, stores, bases, state)
        ok = all(compare(w, g) for w, g in zip(want, got, strict=True))
        bad += not ok
        if not ok or state["enabled"]:
            print(f"{path.stem:28s} {'ok' if ok else 'MISMATCH'} {state}")
    print(f"programs {len(files)} mismatches {bad}")
    return int(bad > 0)


if __name__ == "__main__":
    sys.exit(main())

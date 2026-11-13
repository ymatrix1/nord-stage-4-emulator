#!/usr/bin/env python3
"""Layer zones and virtual MIDI channels of the booted host firmware: why a note reaches a layer.

`arm_keydown.py` brings a MIDI note-on to the layer dispatch loop `0xC00D1DAC`, but only the piano
key handler ran. Three gates decide whether a layer's key handler runs (evidence and tables:
`docs/research/keydown-zones.md`):

1. **Channel map.** The note-on handler `0xC00D1F48` turns the MIDI channel into a *virtual
   channel* `v` with `v = [map + 4 * channel_index]` (`map = manager + 0x3C`, 17 words); `v = 0`
   drops the event. The default map is `{ch1: 1 (global), ch16: 16, index 16: 2}`.
2. **Mask test.** The loop `0xC00D1EBC` accepts a layer client when `event & [client + 0x28] ==
   event` (64 bits). The event mask is `bit(22 + 3) | bit(51 + 9) | bit(16 + 0) | bit(27 + v)`.
3. **Zone enable byte.** The layer virtual call `0xC00C8740` (type `0x27`, note-on) reaches
   `0xC00C859C`, which returns unless the zone object `[client + 4 * (0x16C0 + slot + 12 cls) + 4]`
   has a non-zero byte at `+0` (`0xC00C82D4` writes it). The organ and synth layer-enable handlers
   compute that byte from the program section table at `[[0xC055B144]] + 0x288` (3 sections of
   `0x28` bytes: `(enable, layer mask, focus)` view at `+0`, scene A at `+0xC`); the harness
   leaves the organ and synth sections at 0.

    uv run --with unicorn python tools/arm_zones.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

import arm_boot as ab
import arm_edit_capture as aec
import arm_emu as ae
import arm_keydown as kd

# VERIFIED (run, `keydown-zones.md` section 2): the loop that tests the client mask, the shared
# virtual call (vtable slot 7 of every layer class), the zone-enable method (slot 0 of the second
# vtable of each layer class), the global event descriptor and the note-on handler.
MASK_TEST_PC = 0xC00D1EBC
LAYER_EVENT = 0xC00C8740
ZONE_ENABLE = 0xC00C82D4
EVENT_TABLE = 0xC0571CAC
NOTE_ON_HANDLER = 0xC00D1F48
NOTE_ON_EVENT = 0x27
CLIENT_MASK_OFFSET = 0x28
# VERIFIED (run): per-layer slot index `[client + 0x5AB4]` (organ / piano A 0, B 2; synth A 0, B 2,
# C 4) and the zone pointer table `[client + 4 * (0x16C0 + slot + 12 cls) + 4]`; `cls` 1 = notes.
SLOT_OFFSET = 0x5AB4
ZONE_TABLE = 0x16C0
NOTE_CLASS = 1
# VERIFIED (run): zone object fields: `+0` enable byte, `+4 - +8` first key, `+0x14 + +0x18`
# last key (21 and 108 for every layer: A0..C8), `+0x30 + 4 note` key state (-1 = key up).
ZONE_FIRST, ZONE_FIRST_MINUS, ZONE_LAST, ZONE_LAST_PLUS, ZONE_KEYS = 4, 8, 0x14, 0x18, 0x30
# VERIFIED (run): the layer clients of the dispatcher and the program setup object.
CLIENT_OFFSET = aec.CLIENT_OFFSET
SETUP_GLOBAL = 0xC055B144
SECTION_VIEW, SECTION_SCENE_A = 0x288, 0x294
SECTION_STRIDE = 0x28
SECTIONS = {"organ": 0, "piano": 1, "synth": 2}
# VERIFIED (run): layer mask of an enabled section: bit 0 layer A, bit 1 layer B, bit 2 layer C.
SECTION_MASK = {"organ": 3, "piano": 3, "synth": 7}
# VERIFIED (run): virtual channels. Client masks have bit `27 + v` for these `v`: global 1,
# section 6 / 7 / 8, layer 9 .. 15 (organ A B, piano A B, synth A B C) and 16 (all).
GLOBAL_CHANNEL, ALL_CHANNEL = 1, 16
SECTION_CHANNEL = {"organ": 6, "piano": 7, "synth": 8}
LAYER_CHANNEL = {0: 9, 1: 10, 2: 11, 3: 12, 4: 13, 5: 14, 6: 15}
CHANNEL_MAP_OFFSET, CHANNEL_MAP_WORDS = 0x3C, 17
# VERIFIED (run): per layer enable id the handler of which calls `ZONE_ENABLE`
# (`Edt_OrgCom_kLayerEnableOrg_I`, `Edt_PnoCom_kLayerEnablePno_I`, `Edt_SynCom_kLayerEnableSyn_I`).
LAYER_ENABLE_ID = {0: 0xAE, 1: 0xB0, 2: 0xB4, 3: 0xB6, 4: 0xA1, 5: 0xA3, 6: 0xA3}
# VERIFIED (run): the velocity curve table: signed 16-bit entries, index `curve * 128 + vel`, nine
# curves 0..8; `0xC00D1F48` uses curve `[manager + 0x134]` (6 by default; 1 and 2 for the two
# channel profile types that `[profile + 4]` selects) and passes the entry on as the key velocity.
VELOCITY_TABLE = 0xC02B3128 + 0x18
VELOCITY_CURVES = 9
CURVE_SELECT_OFFSET = 0x134


def event_mask(kind: int, cls: int, aux: int, channel: int) -> int:
    """64-bit event mask of `0xC00D1DAC` for the descriptor `{kind, cls, aux, channel}`."""
    return (1 << (22 + kind)) | (1 << (51 + cls)) | (1 << (16 + aux)) | (1 << (27 + channel))


def client_channels(mask: int) -> list[int]:
    """Virtual channels `1..16` whose bit `27 + v` is set in a client mask."""
    return [v for v in range(1, 17) if mask >> (27 + v) & 1]


def accepts(client_mask: int, event: int) -> bool:
    """The loop test of `0xC00D1EBC`."""
    return client_mask & event == event


def layer_bases(emu: ae.ArmEmu) -> list[int]:
    """Layer client objects in dispatcher order (0, 1 organ; 2, 3 piano; 4..6 synth; 7..9 ext)."""
    g = emu.read32(ab.DISPATCHER_GLOBAL)
    return [emu.read32(g + 0x150 + 4 * i) - CLIENT_OFFSET for i in range(emu.read32(g + 0x14C))]


def client_mask(emu: ae.ArmEmu, base: int) -> int:
    return emu.read32(base + CLIENT_MASK_OFFSET) | emu.read32(base + CLIENT_MASK_OFFSET + 4) << 32


def zone_address(emu: ae.ArmEmu, base: int, cls: int = NOTE_CLASS) -> int:
    slot = emu.read32(base + SLOT_OFFSET)
    return emu.read32(base + 4 * (ZONE_TABLE + slot + 12 * cls) + 4)


def zone_enabled(emu: ae.ArmEmu, base: int) -> bool:
    return emu.read_bytes(zone_address(emu, base), 1) != b"\0"


def zone_range(emu: ae.ArmEmu, base: int) -> tuple[int, int]:
    """First and last key of the zone object."""
    z = zone_address(emu, base)
    return (
        emu.read32(z + ZONE_FIRST) - emu.read32(z + ZONE_FIRST_MINUS),
        emu.read32(z + ZONE_LAST) + emu.read32(z + ZONE_LAST_PLUS),
    )


def force_zone(emu: ae.ArmEmu, base: int, enabled: bool) -> None:
    """Write the zone enable byte directly (test isolation: leave one layer sounding)."""
    emu.write_bytes(zone_address(emu, base), bytes([int(enabled)]))


def setup_object(emu: ae.ArmEmu) -> int:
    """The program setup object `[[0xC055B144]]` (sections at `+0x288` / `+0x294`)."""
    return emu.read32(emu.read32(SETUP_GLOBAL))


def set_section(emu: ae.ArmEmu, section: str, enabled: bool = True) -> None:
    """Write `(enable, layer mask, focus 0)` of a program section into the view and scene A.

    A section entry is `0x28` bytes: the view the layer handlers read at `+0` (`0xC00F8A50` fills
    it from scene A at `+0xC`, or scene B at `+0x18` when `[setup + 0x314] == 1`)."""
    base = setup_object(emu) + SECTION_VIEW + SECTION_STRIDE * SECTIONS[section]
    mask = SECTION_MASK[section] if enabled else 1
    for table in (SECTION_VIEW, SECTION_SCENE_A):
        emu.write32(base + table - SECTION_VIEW, int(enabled))
        emu.write32(base + table - SECTION_VIEW + 4, mask)
        emu.write32(base + table - SECTION_VIEW + 8, 0)


def first_stores(emu: ae.ArmEmu) -> dict[int, aec.Store]:
    """First store object of every layer client, keyed by layer index."""
    out: dict[int, aec.Store] = {}
    for s in aec.find_stores(emu):
        out.setdefault(s.layer, s)
    return out


def refresh_zone(emu: ae.ArmEmu, store: aec.Store, layer: int, window: int = 400_000) -> None:
    """Dispatch the layer enable id (value 1) so that the handler recomputes the zone byte."""
    aec.apply_value(emu, store.address, LAYER_ENABLE_ID[layer], 1, window=window)


def enable_sections(
    emu: ae.ArmEmu, sections: tuple[str, ...] = ("organ", "piano", "synth")
) -> list[int]:
    """Real route: section table + layer enable handlers. Returns the layers now enabled."""
    stores = first_stores(emu)
    for name in sections:
        set_section(emu, name, True)
    bases = layer_bases(emu)
    for layer in LAYER_ENABLE_ID:
        refresh_zone(emu, stores[layer], layer)
    return [i for i, b in enumerate(bases[:7]) if zone_enabled(emu, b)]


def find_channel_map(rig: kd.Rig) -> int:
    """Address of the 17-word channel map: send one note and read `r0 + 0x3C` at `0xC00D1F48`."""
    found: list[int] = []

    def hook(_uc, _a, _s, _u) -> None:  # noqa: ANN001
        found.append(rig.emu.reg(0) + CHANNEL_MAP_OFFSET)

    h = rig.emu.uc.hook_add(
        rig.emu._u.UC_HOOK_CODE, hook, begin=NOTE_ON_HANDLER, end=NOTE_ON_HANDLER
    )
    rig.send(kd.note_on(0, 1))
    rig.tick(8)
    rig.send(kd.note_off(0))
    rig.tick(8)
    rig.emu.uc.hook_del(h)
    if not found:
        raise SystemExit("note-on handler 0xC00D1F48 was not reached")
    return found[0]


def channel_map(emu: ae.ArmEmu, address: int) -> list[int]:
    return [emu.read32(address + 4 * i) for i in range(CHANNEL_MAP_WORDS)]


def set_virtual_channel(emu: ae.ArmEmu, address: int, midi_channel: int, virtual: int) -> None:
    """Route MIDI channel `midi_channel` (1..16) to virtual channel `virtual` (0 = drop)."""
    emu.write32(address + 4 * (midi_channel - 1), virtual)


def velocity_curve(emu: ae.ArmEmu, curve: int) -> list[int]:
    """The 128 signed 16-bit entries of velocity curve `curve` (0..8) of `0xC00D1F48`."""
    raw = emu.read_bytes(VELOCITY_TABLE + 256 * curve, 256)
    return [int.from_bytes(raw[i : i + 2], "little", signed=True) for i in range(0, 256, 2)]


def velocity_csv(emu: ae.ArmEmu) -> list[str]:
    """`velocity,curve0 .. curve8` rows for velocities 1..127."""
    curves = [velocity_curve(emu, c) for c in range(VELOCITY_CURVES)]
    head = "velocity," + ",".join(f"curve{c}" for c in range(VELOCITY_CURVES))
    return [head] + [f"{v}," + ",".join(str(c[v]) for c in curves) for v in range(1, 128)]


@dataclass
class MaskLog:
    """Entries of the mask test: `(event mask, client address, client mask, accepted)`."""

    rows: list[tuple[int, int, int, bool]]

    @classmethod
    def attach(cls, emu: ae.ArmEmu) -> MaskLog:
        log = cls([])

        def hook(_uc, _a, _s, _u) -> None:  # noqa: ANN001
            event = emu.reg(6) | emu.reg(7) << 32
            client = emu.read32(emu.reg(5))
            mask = client_mask(emu, client)
            log.rows.append((event, client, mask, accepts(mask, event)))

        emu.uc.hook_add(emu._u.UC_HOOK_CODE, hook, begin=MASK_TEST_PC, end=MASK_TEST_PC)
        return log


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - thin CLI
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path, nargs="?")
    ap.add_argument("--cache", type=Path)
    ns = ap.parse_args(argv)
    rig = kd.build_rig(ns.image, ns.sram, ns.cache)
    emu = rig.emu
    bases = layer_bases(emu)
    print("zones before:", [int(zone_enabled(emu, b)) for b in bases[:7]])
    print("enabled layers:", enable_sections(emu))
    print("zone range:", zone_range(emu, bases[0]))
    addr = find_channel_map(rig)
    print(f"channel map {addr:#x}:", channel_map(emu, addr))
    log = MaskLog.attach(emu)
    counts = kd.count_calls(emu)
    rig.send(kd.note_on(60, 100))
    rig.tick(20)
    for event, client, mask, ok in log.rows:
        print(f"event {event:016x} client {client:#x} mask {mask:016x} accepted {ok}")
    print({k: v for k, v in counts.items() if v})
    return 0


if __name__ == "__main__":
    sys.exit(main())

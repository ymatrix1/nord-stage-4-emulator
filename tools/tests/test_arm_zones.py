"""Unit tests for tools/arm_zones.py with synthetic memory and hand-assembled ARM snippets."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_boot as ab
import arm_emu as ae
import arm_zones as az
import pytest

pytest.importorskip("unicorn")
faulthandler.disable()

BX_LR = 0xE12FFF1E
BIG = 0x600000
OBJ = ae.SRAM_BASE + 0x10000


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def emu_with(patches: dict[int, bytes]) -> ae.ArmEmu:
    buf = bytearray(BIG)
    for address, code in patches.items():
        buf[address - ae.LOAD_BASE : address - ae.LOAD_BASE + len(code)] = code
    emu = ae.ArmEmu(bytes(buf), ram_size=BIG)
    emu.uc.mem_map(0, ab.LOW_MEMORY)
    return emu


@pytest.fixture(autouse=True)
def _release_emulators():
    yield
    gc.collect()


def test_event_mask_matches_the_masks_observed_in_the_run() -> None:
    # descriptor `{3, 9, 0, v}` read at `0xC00D1EBC` for MIDI channels routed to virtual channel v
    observed = {
        1: 0x1000000012010000,
        2: 0x1000000022010000,
        6: 0x1000000202010000,
        9: 0x1000001002010000,
        13: 0x1000010002010000,
        16: 0x1000080002010000,
    }
    for v, mask in observed.items():
        assert az.event_mask(3, 9, 0, v) == mask


def test_client_channels_decodes_the_observed_client_masks() -> None:
    assert az.client_channels(0xFFF8081217FFFFFF) == [1, 6, 9, 16]  # organ A
    assert az.client_channels(0xFFF8082217FFFFFF) == [1, 6, 10, 16]  # organ B
    assert az.client_channels(0xFFF8084417FFFFFF) == [1, 7, 11, 16]  # piano A
    assert az.client_channels(0xFFF80C0817FFFFFF) == [1, 8, 15, 16]  # synth C
    assert az.client_channels(0x10000000113FFFFF) == [1]  # the non-layer client: global only


def test_accepts_requires_every_event_bit_in_the_client_mask() -> None:
    event = az.event_mask(3, 9, 0, 1)
    assert az.accepts(0xFFF8081217FFFFFF, event)
    assert not az.accepts(0x10000000113FFFFF, event)  # bit 25 and 28 of the low word missing
    assert not az.accepts(0xFFF8081217FFFFFF, az.event_mask(3, 9, 0, 2))  # virtual channel 2


def build_layer(emu: ae.ArmEmu, base: int, slot: int, zone: int, enable: int) -> None:
    emu.write32(base + az.SLOT_OFFSET, slot)
    emu.write32(base + 4 * (az.ZONE_TABLE + slot + 12 * az.NOTE_CLASS) + 4, zone)
    emu.write_bytes(zone, bytes([enable]))
    emu.write32(zone + az.ZONE_FIRST, 0x15)
    emu.write32(zone + az.ZONE_FIRST_MINUS, 0)
    emu.write32(zone + az.ZONE_LAST, 0x6C)
    emu.write32(zone + az.ZONE_LAST_PLUS, 0)


def test_zone_helpers_follow_the_slot_and_zone_table() -> None:
    emu = emu_with({})
    build_layer(emu, OBJ, slot=2, zone=OBJ + 0x40000, enable=0)
    assert az.zone_address(emu, OBJ) == OBJ + 0x40000
    assert not az.zone_enabled(emu, OBJ)
    assert az.zone_range(emu, OBJ) == (21, 108)
    az.force_zone(emu, OBJ, True)
    assert az.zone_enabled(emu, OBJ)
    az.force_zone(emu, OBJ, False)
    assert emu.read_bytes(OBJ + 0x40000, 1) == b"\0"


def test_layer_bases_subtract_the_client_offset() -> None:
    emu = emu_with({})
    g = OBJ + 0x1000
    emu.write32(ab.DISPATCHER_GLOBAL, g)
    emu.write32(g + 0x14C, 2)
    emu.write32(g + 0x150, OBJ + az.CLIENT_OFFSET)
    emu.write32(g + 0x154, OBJ + 0x2000 + az.CLIENT_OFFSET)
    assert az.layer_bases(emu) == [OBJ, OBJ + 0x2000]


def test_client_mask_joins_the_two_words() -> None:
    emu = emu_with({})
    emu.write32(OBJ + az.CLIENT_MASK_OFFSET, 0x17FFFFFF)
    emu.write32(OBJ + az.CLIENT_MASK_OFFSET + 4, 0xFFF80812)
    assert az.client_mask(emu, OBJ) == 0xFFF8081217FFFFFF


def test_set_section_writes_enable_mask_focus_to_view_and_scene_a() -> None:
    emu = emu_with({})
    setup = OBJ + 0x8000
    emu.write32(az.SETUP_GLOBAL, OBJ + 0x7000)
    emu.write32(OBJ + 0x7000, setup)
    emu.write_bytes(setup + 0x288, bytes(0x80))
    az.set_section(emu, "synth", True)
    base = setup + az.SECTION_VIEW + az.SECTION_STRIDE * 2
    for off in (0, 0xC):  # view, scene A
        assert [emu.read32(base + off + 4 * i) for i in range(3)] == [1, 7, 0]
    assert emu.read32(base + 0x18) == 0  # scene B untouched
    assert emu.read32(setup + az.SECTION_VIEW) == 0  # organ untouched
    az.set_section(emu, "organ", True)
    assert emu.read32(setup + az.SECTION_VIEW + 4) == 3
    az.set_section(emu, "synth", False)
    assert [emu.read32(base + 4 * i) for i in range(3)] == [0, 1, 0]


def test_channel_map_routes_a_midi_channel_to_a_virtual_channel() -> None:
    emu = emu_with({})
    address = OBJ + 0x100
    for i, v in enumerate([1] + [0] * 14 + [16, 2]):
        emu.write32(address + 4 * i, v)
    assert az.channel_map(emu, address)[:2] == [1, 0]
    az.set_virtual_channel(emu, address, 5, 9)
    assert az.channel_map(emu, address)[4] == 9
    assert len(az.channel_map(emu, address)) == az.CHANNEL_MAP_WORDS


def test_velocity_curve_reads_signed_shorts_per_curve() -> None:
    emu = emu_with({})
    for curve in range(3):
        raw = b"".join(struct.pack("<h", (-1) ** curve * (curve * 1000 + i)) for i in range(128))
        emu.write_bytes(az.VELOCITY_TABLE + 256 * curve, raw)
    assert az.velocity_curve(emu, 0)[5] == 5
    assert az.velocity_curve(emu, 1)[127] == -1127
    assert len(az.velocity_curve(emu, 2)) == 128


class FakeRig:
    """Only what `find_channel_map` uses: `emu`, `send`, `tick`."""

    def __init__(self, emu: ae.ArmEmu, manager: int) -> None:
        self.emu, self.manager, self.sent = emu, manager, []

    def send(self, data: bytes) -> None:
        self.sent.append(data)
        if len(self.sent) == 1:
            self.emu.call(az.NOTE_ON_HANDLER, self.manager)

    def tick(self, count: int = 1) -> list:
        return []


def test_find_channel_map_hooks_the_note_on_handler_and_adds_the_offset() -> None:
    emu = emu_with({az.NOTE_ON_HANDLER: words(BX_LR)})
    rig = FakeRig(emu, OBJ)
    assert az.find_channel_map(rig) == OBJ + az.CHANNEL_MAP_OFFSET
    assert rig.sent[0] == bytes([0x90, 0, 1])
    assert rig.sent[1] == bytes([0x80, 0, 0])


def test_find_channel_map_exits_when_the_handler_is_never_reached() -> None:
    emu = emu_with({})
    rig = FakeRig(emu, OBJ)
    rig.send = lambda data: None  # type: ignore[method-assign]
    with pytest.raises(SystemExit):
        az.find_channel_map(rig)


def test_mask_log_records_event_client_mask_and_verdict() -> None:
    emu = emu_with({az.MASK_TEST_PC: words(BX_LR)})
    client, slot = OBJ + 0x200, OBJ + 0x100
    emu.write32(slot, client)
    emu.write32(client + az.CLIENT_MASK_OFFSET, 0x17FFFFFF)
    emu.write32(client + az.CLIENT_MASK_OFFSET + 4, 0xFFF80812)
    log = az.MaskLog.attach(emu)
    event = az.event_mask(3, 9, 0, 1)
    emu.set_reg(5, slot)
    emu.set_reg(6, event & 0xFFFFFFFF)
    emu.set_reg(7, event >> 32)
    emu.run(az.MASK_TEST_PC, ae.SENTINEL, 1)
    assert log.rows == [(event, client, 0xFFF8081217FFFFFF, True)]


def test_enable_sections_sets_sections_refreshes_layers_and_reports_enabled(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    emu = emu_with({})
    bases = [OBJ + 0x4000 * i for i in range(7)]
    for i, b in enumerate(bases):
        build_layer(emu, b, slot=0, zone=OBJ + 0x80000 + 0x100 * i, enable=0)
    stores = {layer: az.aec.Store(layer, "x", 0x1234 + layer) for layer in range(7)}
    calls: list[tuple[int, int, int]] = []
    sections: list[str] = []
    monkeypatch.setattr(az, "layer_bases", lambda _e: bases)
    monkeypatch.setattr(az, "first_stores", lambda _e: stores)
    monkeypatch.setattr(az, "set_section", lambda _e, name, on: sections.append(name))

    def fake_apply(e, store, eid, value, window=0):  # noqa: ANN001
        calls.append((store, eid, value))
        layer = store - 0x1234
        az.force_zone(e, bases[layer], layer != 2)

    monkeypatch.setattr(az.aec, "apply_value", fake_apply)
    assert az.enable_sections(emu) == [0, 1, 3, 4, 5, 6]
    assert (0x1234, 0xAE, 1) in calls and (0x1234 + 4, 0xA1, 1) in calls
    assert sections == ["organ", "piano", "synth"]
    assert len(calls) == 7


def test_velocity_csv_lists_nine_curves_per_velocity() -> None:
    emu = emu_with({})
    for curve in range(az.VELOCITY_CURVES):
        raw = b"".join(struct.pack("<h", curve * 100 + i) for i in range(128))
        emu.write_bytes(az.VELOCITY_TABLE + 256 * curve, raw)
    rows = az.velocity_csv(emu)
    assert rows[0] == "velocity,curve0,curve1,curve2,curve3,curve4,curve5,curve6,curve7,curve8"
    assert rows[1] == "1,1,101,201,301,401,501,601,701,801"
    assert len(rows) == 128

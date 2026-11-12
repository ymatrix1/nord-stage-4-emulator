"""Unit tests for tools/arm_emu.py with hand-assembled ARM snippets (no firmware needed)."""

from __future__ import annotations

import faulthandler
import gc
import struct

import arm_emu as ae
import pytest

pytest.importorskip("unicorn")
# Unicorn probes guard pages through a Windows structured exception that faulthandler would print.
faulthandler.disable()

BASE = ae.LOAD_BASE
BX_LR = 0xE12FFF1E


def words(*ws: int) -> bytes:
    return b"".join(struct.pack("<I", w & 0xFFFFFFFF) for w in ws)


def image(**at: bytes) -> bytes:
    """Code image with snippets at the given offsets (`s_0x100=...` style keys)."""
    parts = {int(k.split("_")[1], 0): v for k, v in at.items()}
    size = max(o + len(v) for o, v in parts.items())
    buf = bytearray(size)
    for o, v in parts.items():
        buf[o : o + len(v)] = v
    return bytes(buf)


@pytest.fixture(autouse=True)
def _release_emulators():
    """Unicorn instances sit in reference cycles; free them so many tests fit in memory."""
    yield
    gc.collect()


def emu_with(code: bytes) -> ae.ArmEmu:
    return ae.ArmEmu(code, ram_size=0x10000)


# send(r0 = address, r1 = value) to the path 2 port of comm 2 as (address, 0, value)
POKE_COMM2 = words(
    0xE59F2010,  # ldr r2, [pc, #16]
    0xE5820000,  # str r0, [r2]
    0xE3A03000,  # mov r3, #0
    0xE5823000,  # str r3, [r2]
    0xE5821000,  # str r1, [r2]
    BX_LR,
    ae.PORT2_BASE + 0x20,
)

# send(r0 = word1, r1 = word2) to the path 1 port of comm 3
PAIR_COMM3 = words(
    0xE59F2008,  # ldr r2, [pc, #8]
    0xE5820000,  # str r0, [r2]
    0xE5821000,  # str r1, [r2]
    BX_LR,
    ae.PORT1_BASE + 0x30,
)


def test_call_returns_r0() -> None:
    emu = emu_with(image(s_0=words(0xE3A0002A, BX_LR)))  # mov r0, #42 ; bx lr
    assert emu.call(BASE) == 42


def test_call_passes_register_and_stack_arguments() -> None:
    # add r0, r0, r4?  AAPCS: r0..r3 registers, 5th argument on the stack: ldr r0, [sp]
    emu = emu_with(image(s_0=words(0xE59D0000, BX_LR)))
    assert emu.call(BASE, 1, 2, 3, 4, 0x1234) == 0x1234


def test_poke_port_store_becomes_three_word_message() -> None:
    emu = emu_with(image(s_0=POKE_COMM2))
    emu.call(BASE, 0x28B80080, 0xBEEF)
    (m,) = emu.messages
    assert (m.path, m.comm, m.chip, m.core) == (2, 2, "ORG", 2)
    assert (m.word1, m.word2, m.dm_word) == (0x28B80080, 0xBEEF, 0xE0020)
    assert m.pc == BASE + 0x10


def test_core_one_address_maps_to_dm_word() -> None:
    emu = emu_with(image(s_0=POKE_COMM2))
    emu.call(BASE, 0x28307730, 1)
    (m,) = emu.messages
    assert m.core == 1
    assert m.dm_word == 0xC1DCC


def test_pair_port_store_is_path_one_message() -> None:
    emu = emu_with(image(s_0=PAIR_COMM3))
    emu.call(BASE, 0x92406, 0x3F800000)
    (m,) = emu.messages
    assert (m.path, m.comm, m.chip, m.core) == (1, 3, "SY", 0)
    assert (m.word1, m.word2, m.dm_word) == (0x92406, 0x3F800000, 0x92406)


def test_clear_messages_and_message_order() -> None:
    emu = emu_with(image(s_0=POKE_COMM2))
    emu.call(BASE, 0x28800000, 1)
    emu.call(BASE, 0x28800004, 2)
    assert [m.word2 for m in emu.messages] == [1, 2]
    emu.clear_messages()
    assert emu.messages == []


def test_group_port_writes_drops_incomplete_group() -> None:
    p = ae.PORT2_BASE
    writes = [(p, 5, 0), (p, 0, 0), (p, 6, 0), (p, 7, 0)]
    out = ae.group_port_writes(writes)
    assert [(m.word1, m.word2) for m in out] == [(5, 6)]


def test_group_port_writes_pad_off_uses_pairs() -> None:
    p = ae.PORT2_BASE + 0x30  # comm 3: pad off by default
    out = ae.group_port_writes([(p, 1, 0), (p, 2, 0), (p, 3, 0), (p, 4, 0)])
    assert [(m.word1, m.word2) for m in out] == [(1, 2), (3, 4)]


def test_group_port_writes_ignores_foreign_ports() -> None:
    assert ae.group_port_writes([(0x1234, 1, 0)]) == []


def test_message_json_roundtrip() -> None:
    m = ae.Message(2, 2, 0x28B07730, 0x04005500, 0xC01CE10C)
    back = ae.messages_from_jsonl(m.to_json() + "\n\n")
    assert back == [m]
    assert m.to_dict()["dm"] == "0xc1dcc"
    assert m.to_dict()["chip"] == "ORG"


def test_unknown_comm_chip_name() -> None:
    assert ae.Message(2, 9, 0, 0).chip == "comm9"


def test_to_msg_script_filters_chip_and_core() -> None:
    msgs = [
        ae.Message(2, 2, 0x28B07730, 0x10, 0x100),  # ORG core 2
        ae.Message(2, 2, 0x28307730, 0x20, 0x104),  # ORG core 1
        ae.Message(2, 1, 0x28B07730, 0x30, 0x108),  # PNO core 2
        ae.Message(1, 3, 0x92406, 0x40, 0x10C),  # SY path 1
    ]
    org2 = ae.to_msg_script(msgs, "ORG", 2, start_pass=3)
    assert org2 == "3 0xc1dcc 0x00000010  # 0x28b07730 pc 0x100\n"
    assert ae.to_msg_script(msgs, "ORG", 1).startswith("0 0xc1dcc 0x00000020")
    assert ae.to_msg_script(msgs, "SY", 0).startswith("0 0x92406 0x00000040")
    assert ae.to_msg_script(msgs, "FX", 2) == ""


def test_add_stub_replaces_function_and_sets_r0() -> None:
    # main: push {lr}; bl 0x100; pop {pc}
    main = words(0xE52DE004, 0xEB000000 | (((0x100 - (4 + 8)) >> 2) & 0xFFFFFF), 0xE49DF004)
    callee = words(0xE3A00001, BX_LR)  # real body returns 1
    emu = emu_with(image(s_0=main, s_0x100=callee))
    assert emu.call(BASE) == 1
    emu.add_stub(BASE + 0x100, lambda _e: 7)
    assert emu.call(BASE) == 7


def test_stub_can_read_arguments() -> None:
    emu = emu_with(image(s_0x100=words(BX_LR)))
    seen = []
    emu.add_stub(BASE + 0x100, lambda e: seen.append((e.reg(0), e.reg(1))) or 0)
    emu.call(BASE + 0x100, 11, 22)
    assert seen == [(11, 22)]


def test_skip_instruction() -> None:
    emu = emu_with(image(s_0=words(0xE3A00001, 0xE3A00002, BX_LR)))  # mov r0,#1; mov r0,#2
    emu.skip_instruction(BASE + 4)
    assert emu.call(BASE) == 1


def test_guard_null_calls_returns_zero() -> None:
    code = words(
        0xE52DE004,  # push {lr}
        0xE3A03000,  # mov r3, #0
        0xE3A00005,  # mov r0, #5
        0xE12FFF33,  # blx r3  (null call)
        0xE49DF004,  # pop {pc}
    )
    emu = emu_with(image(s_0=code))
    emu.guard_null_calls()
    assert emu.call(BASE) == 0
    assert len(emu.null_calls) == 1


def test_poll_breaker_ends_busy_wait_on_device_register() -> None:
    code = words(
        0xE59F100C,  # ldr r1, [pc, #12]   -> device register
        0xE5912000,  # loop: ldr r2, [r1]
        0xE3520000,  # cmp r2, #0
        0x0AFFFFFC,  # beq loop
        BX_LR,
        0x01C10000,
    )
    emu = emu_with(image(s_0=code))
    emu.enable_poll_breaker(limit=5)
    emu.call(BASE, max_insns=100_000)
    assert emu.poll_hits


def test_bump_new_returns_aligned_distinct_zeroed_blocks() -> None:
    emu = emu_with(image(s_0x100=words(BX_LR)))
    emu.bump_new(BASE + 0x100)
    a = emu.call(BASE + 0x100, 20)
    b = emu.call(BASE + 0x100, 1)
    assert a % 16 == 0
    assert b - a == 32
    assert emu.read_bytes(a, 20) == bytes(20)


def test_register_and_memory_helpers() -> None:
    emu = emu_with(image(s_0=words(BX_LR)))
    emu.write32(ae.SRAM_BASE + 0x40, 0xCAFEBABE)
    assert emu.read32(ae.SRAM_BASE + 0x40) == 0xCAFEBABE
    emu.write_bytes(ae.SRAM_BASE + 0x80, b"abcd")
    assert emu.read_bytes(ae.SRAM_BASE + 0x80, 4) == b"abcd"
    emu.set_reg(5, 0x1_0000_0001)
    assert emu.reg(5) == 1


def test_unmapped_access_is_auto_mapped_and_logged() -> None:
    # ldr r1, [pc, #8]; ldr r0, [r1]; bx lr ; literal
    emu = emu_with(image(s_0=words(0xE59F1004, 0xE5910000, BX_LR, 0x50000000)))
    assert emu.call(BASE) == 0
    assert 0x50000000 in emu.auto_mapped


def test_sram_block_is_loaded_at_sram_base() -> None:
    emu = ae.ArmEmu(image(s_0=words(BX_LR)), sram=b"\x11\x22\x33\x44")
    assert emu.read32(ae.SRAM_BASE) == 0x44332211


def test_static_init_skips_external_calls() -> None:
    init = 0x2A4
    code = image(
        s_0x2A4=words(
            0xE52DE004,  # push {lr}
            0xEB000000 | (((0x400 - (init + 4 + 8)) >> 2) & 0xFFFFFF),  # bl 0x400 (skipped)
            0xE3A02009,  # mov r2, #9
            0xE49DF004,  # pop {pc}
        ),
        s_0x400=words(0xE3A0504D, BX_LR),  # mov r5, #77 ; bx lr
    )
    emu = emu_with(code)
    ae.run_static_init(emu)
    assert emu.reg(2) == 9
    assert emu.reg(5) != 77


def test_static_init_keeps_dsp_ports_working() -> None:
    emu = emu_with(image(s_0=POKE_COMM2, s_0x2A4=words(BX_LR)))
    ae.run_static_init(emu)
    emu.call(BASE, 0x28800010, 3)
    assert [m.word2 for m in emu.messages] == [3]


def test_fake_comm_objects_follow_firmware_layout() -> None:
    emu = emu_with(image(s_0=words(BX_LR)))
    assert emu.read32(ae.PORT_OBJECT_PTR) == ae.FAKE_BASE
    assert emu.read32(ae.FAKE_BASE + 0x40 + 4 * 2) == ae.PORT2_BASE + 0x20
    assert emu.read_bytes(ae.FAKE_BASE + 0x90, 5) == bytes([1, 1, 1, 0, 0])
    obj = emu.read32(ae.COMM_TABLE + 4 * 4)  # comm 3
    assert emu.read32(obj + 0x10) == ae.PORT1_BASE + 0x30

"""Unit tests for tools/arm_sample_probe.py with synthetic `.nsmp4` bytes and a fake emulator."""

from __future__ import annotations

import struct

import arm_sample_probe as sp


def chunk(tag: str, chunk_id: int, payload: bytes) -> bytes:
    return b"\0" + tag.encode() + struct.pack(">II", chunk_id, len(payload)) + payload


def synthetic_nsmp(name: str = "Test Sample") -> bytes:
    head = bytearray(0x3C)
    head[0:4] = b"CBIN"
    head[8:12] = b"nsmp"
    head[0x2C:0x30] = b"NSMP"
    hdr = bytes(10) + name.encode() + b"\0" * 8
    return bytes(head) + chunk("hdr", 11, hdr) + chunk("cat", 7, bytes(8)) + chunk("stk", 3, b"xy")


class FakeEmu:
    """Word memory addressed by byte address."""

    def __init__(self, words: dict[int, int]) -> None:
        self.words = words

    def read32(self, address: int) -> int:
        return self.words.get(address, 0)


def test_is_nsmp_checks_kind_and_magic() -> None:
    data = synthetic_nsmp()
    assert sp.is_nsmp(data)
    assert not sp.is_nsmp(b"CBIN" + bytes(0x40))
    assert not sp.is_nsmp(data[:0x20])


def test_chunks_lists_tags_in_order_and_drops_a_truncated_tail() -> None:
    data = synthetic_nsmp()
    assert [c.tag for c in sp.chunks(data)] == ["hdr", "cat", "stk"]
    assert [c.tag for c in sp.chunks(data[:-1])] == ["hdr", "cat"]
    first = sp.chunks(data)[0]
    assert (first.chunk_id, first.start) == (11, 0x3C + 12)


def test_chunks_stops_at_a_non_tag() -> None:
    data = synthetic_nsmp() + b"\x01junk" + bytes(16)
    assert [c.tag for c in sp.chunks(data)] == ["hdr", "cat", "stk"]


def test_sample_name_reads_the_hdr_payload_after_its_prefix() -> None:
    assert sp.sample_name(synthetic_nsmp("Carina Accordion 1")) == "Carina Accordion 1"
    assert sp.sample_name(bytes(0x40)) == ""


def test_device_image_prepends_header_with_type_word_and_keeps_bytes_from_magic() -> None:
    data = synthetic_nsmp()
    image = sp.device_image(data, sp.LOADER_TYPE_WORDS["nsp"], "x.nsmp")
    assert image[:6] == b"x.nsmp"
    assert struct.unpack("<I", image[0xA4:0xA8])[0] == 0x6E737000
    assert image[0xB0:] == data[0x2C:]
    assert len(image) == 0xB0 + len(data) - 0x2C


def test_loader_type_words_are_the_big_endian_tags() -> None:
    for label, word in sp.LOADER_TYPE_WORDS.items():
        assert struct.pack(">I", word).rstrip(b"\0").decode() == (
            "npno" if label == "onpn" else label
        )


def test_synth_gate_reports_an_empty_slot_list_as_closed() -> None:
    obj = 0x90001000
    words = {sp.SYNTH_LAYER_TABLE + 8: obj, obj + sp.SLOT_LIST_OFFSET: 0x90002000}
    gate = sp.synth_gate(FakeEmu(words), 2)  # type: ignore[arg-type]
    assert (gate.obj, gate.slot_list, gate.first_slot) == (obj, 0x90002000, 0)
    assert not gate.open
    assert gate.counters == (0, 0, 0)


def test_synth_gate_opens_with_a_slot_object_and_reads_counters() -> None:
    obj = 0x90001000
    words = {
        sp.SYNTH_LAYER_TABLE: obj,
        obj + sp.SLOT_LIST_OFFSET: 0x90002000,
        0x90002000: 0x90003000,
        sp.COUNTER_BASE + sp.COUNTER_OFFSETS[1]: 3,
    }
    gate = sp.synth_gate(FakeEmu(words), 0)  # type: ignore[arg-type]
    assert gate.open
    assert gate.counters == (0, 3, 0)


def test_synth_gate_of_a_missing_layer_object_is_all_zero() -> None:
    gate = sp.synth_gate(FakeEmu({}), 9)  # type: ignore[arg-type]
    assert gate.obj == 0 and not gate.open and gate.counters == ()


def test_sample_device_image_has_name_type_word_and_body_from_magic() -> None:
    data = synthetic_nsmp()
    image = sp.sample_device_image(data, "Carina")
    assert len(image) == sp.SAMPLE_HEADER + len(data) - sp.MAGIC_OFFSET
    assert image[:6] == b"Carina"
    word = struct.unpack_from("<I", image, sp.SAMPLE_TYPE_OFFSET)[0]
    assert word == sp.SAMPLE_TYPE_WORD == 0x6E736D70
    assert image[sp.SAMPLE_HEADER : sp.SAMPLE_HEADER + 4] == b"NSMP"


def test_sample_descriptor_puts_the_file_length_at_plus_4_and_plus_0x10() -> None:
    raw = sp.sample_descriptor(3, 5, 1234)
    assert len(raw) == 0x18
    assert struct.unpack_from("<HH", raw, 0) == (3, 5)
    assert struct.unpack_from("<I", raw, 4)[0] == 1234
    assert struct.unpack_from("<I", raw, 0x10)[0] == 1234


def test_directory_entry_holds_the_id_at_plus_4() -> None:
    raw = sp.directory_entry(7, 9)
    assert struct.unpack("<HHH", raw) == (7, 0, 9)

"""Unit tests for tools/arm_store.py: the cluster / record layout of the program volume."""

from __future__ import annotations

import struct
import zlib

import arm_i2c as ai
import arm_store as st
import pytest


def _stream(name: str, body: bytes) -> bytes:
    """Payload stream as the firmware writes it: size, crc32(body), 36-byte header, body."""
    header = name.encode().ljust(24, b"\0") + b"p4sn" + bytes(4) + b"\x11\0\0\0"
    assert len(header) == 36
    return struct.pack("<II", 36 + len(body), zlib.crc32(body)) + header + body


def _volume(stream: bytes) -> bytearray:
    volume = bytearray(b"\xff" * ai.EEPROM_SIZE)
    volume[:8] = struct.pack("<II", st.VOLUME_MAGIC, 1)
    pages = st.pack_clusters(stream)
    volume[st.CLUSTER_BASE : st.CLUSTER_BASE + len(pages)] = pages
    return volume


def test_pack_clusters_numbers_the_pages_and_pads_the_last_one() -> None:
    pages = st.pack_clusters(bytes(range(200)))
    assert len(pages) == 2 * st.CLUSTER_SIZE
    assert pages[:4] == bytes([1, 0, 0, 0])
    assert pages[st.CLUSTER_SIZE : st.CLUSTER_SIZE + 4] == bytes([1, 0, 1, 0])
    assert pages[4 : 4 + st.CLUSTER_PAYLOAD] == bytes(range(st.CLUSTER_PAYLOAD))
    assert pages.endswith(b"\xff")


def test_unpack_record_matches_the_real_store() -> None:
    # A real Store wrote 860 bytes = 36-byte header + 824-byte body in 7 clusters.
    body = bytes(range(256)) * 3 + bytes(56)
    assert len(body) == st.BODY_BYTES
    volume = _volume(_stream("Unnamed", body))
    stream = st.unpack_clusters(bytes(volume))
    assert len(stream) >= 868
    rec = st.unpack_record(stream)
    assert rec == {"size": 860, "crc": zlib.crc32(body), "name": "Unnamed", "crc_ok": True}


def test_unpack_clusters_stops_at_a_free_or_out_of_sequence_cluster() -> None:
    volume = _volume(b"x" * 300)
    assert len(st.unpack_clusters(bytes(volume))) == 3 * st.CLUSTER_PAYLOAD
    volume[st.CLUSTER_BASE + st.CLUSTER_SIZE + 2] = 7  # index 7 where 1 is expected
    assert len(st.unpack_clusters(bytes(volume))) == st.CLUSTER_PAYLOAD
    assert st.unpack_clusters(b"\xff" * 1024) == b""


def test_set_name_rewrites_the_name_without_breaking_the_crc() -> None:
    body = bytes(range(200)) * 4 + bytes(24)
    volume = _volume(_stream("Unnamed", body[: st.BODY_BYTES]))
    st.set_name(volume, "Studio Grand")
    rec = st.unpack_record(st.unpack_clusters(bytes(volume)))
    assert rec["name"] == "Studio Grand" and rec["crc_ok"] is True
    st.set_name(volume, "A")
    assert st.unpack_record(st.unpack_clusters(bytes(volume)))["name"] == "A"
    with pytest.raises(ValueError):
        st.set_name(volume, "x" * st.NAME_BYTES)


def test_a_flipped_body_byte_fails_the_crc() -> None:
    volume = _volume(_stream("P", bytes(st.BODY_BYTES)))
    at = st.CLUSTER_BASE + st.CLUSTER_SIZE * 3 + 10
    volume[at] ^= 1
    assert st.unpack_record(st.unpack_clusters(bytes(volume)))["crc_ok"] is False


def test_image_join_and_split_round_trip() -> None:
    devices = [bytes([i]) * ai.EEPROM_SIZE for i in range(ai.EEPROM_COUNT)]
    assert st.split_image(st.join_image(devices)) == devices
    with pytest.raises(ValueError):
        st.split_image(b"short")


def test_store_key_is_a_valid_panel_key() -> None:
    import arm_keys as ak

    assert st.STORE_KEY in ak.all_keys()

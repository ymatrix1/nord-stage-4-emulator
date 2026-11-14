"""Unit tests for tools/fx_delay_time.py (law, committed table and captured series)."""

from __future__ import annotations

import hashlib
import struct
from pathlib import Path

import fx_delay_time as fd
import pytest

ROOT = Path(__file__).resolve().parents[2]
RUST_TABLE = ROOT / "crates" / "nord-core" / "src" / "fx" / "fw_delay_table.rs"
SERIES = ROOT / "docs" / "research" / "data" / "fx-delay-time.csv"


@pytest.fixture(scope="module")
def table() -> list[int]:
    return fd.parse_rust_table(RUST_TABLE.read_text(encoding="utf-8"))


def fnv64(table: list[int]) -> int:
    h = 0xCBF29CE484222325
    for b in struct.pack(f"<{len(table)}h", *table):
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def test_committed_table_has_the_documented_shape_and_fingerprints(table: list[int]) -> None:
    assert len(table) == fd.TABLE_LEN
    assert all(a >= b for a, b in zip(table, table[1:], strict=False))
    assert (table[0], table[20], table[1499], table[1500]) == (16383, 16256, 2, 0)
    assert fd.table_fingerprint(table) == fd.TABLE_FINGERPRINT
    source = RUST_TABLE.read_text(encoding="utf-8")
    assert f"0x{fnv64(table):016x}" in source
    assert hashlib.sha256(struct.pack("<1501h", *table)).hexdigest() == fd.TABLE_FINGERPRINT


def test_law_reproduces_every_captured_word(table: list[int]) -> None:
    series = fd.read_series(SERIES)
    assert len(series) >= 250
    assert fd.check_series(table, series) == []
    assert series[0] == fd.MAX_SAMPLES and series[16383] == 868


def test_check_series_reports_a_wrong_word(table: list[int]) -> None:
    assert fd.check_series(table, {1024: 46498.0}) == []
    assert fd.check_series(table, {1024: 46499.0}) == [(1024, 46499.0, 46498)]


def test_index_plateaus_exact_entries_and_halves(table: list[int]) -> None:
    assert fd.fractional_index(table, 0) == 1500.0
    assert fd.fractional_index(table, 16256) == fd.fractional_index(table, 16383) == 20.0
    assert fd.fractional_index(table, 1) == 1499.5
    k = next(k for k in range(100, 1400) if table[k + 1] < table[k] < table[k - 1])
    assert fd.fractional_index(table, table[k]) == k


def test_search_follows_the_handler_on_a_synthetic_table() -> None:
    # 1501 entries falling by 10 per index; the coarse (46) and fine steps must agree
    synth = [max(0, 16383 - 10 * k) for k in range(fd.TABLE_LEN)]
    synth[:20] = [16383] * 20
    synth[-1] = 0
    for v in (5003, 12345, 16000):
        k = fd.fractional_index(synth, v)
        i = int(k)
        assert synth[i] >= v >= synth[i + 1], (v, k)
        assert k - i == pytest.approx((synth[i] - v) / 10)


def test_word_is_monotone_clamped_and_in_samples(table: list[int]) -> None:
    words = [fd.delay_samples(table, v) for v in range(0, 16384, 5)]
    assert all(a >= b for a, b in zip(words, words[1:], strict=False))
    assert words[0] == fd.MAX_SAMPLES and words[-1] == 868
    assert fd.delay_samples(table, 16383) == 868
    assert fd.delay_samples(table, 150) == fd.MAX_SAMPLES


def test_echo_delay_and_crossfade() -> None:
    assert fd.echo_delay(20.0) == 22 and fd.echo_delay(20.9) == 22 and fd.echo_delay(2000) == 2002
    # the ARM sequence: snap to the old tap, then ramp to the new one in 1085 calls (45 ms)
    assert fd.crossfade_weight(32768, -32768.0, 1) == 0
    assert fd.crossfade_weight(0, 30.1993, 1) == 30
    assert fd.crossfade_weight(0, 30.1993, 1085) == 32550
    assert fd.crossfade_weight(0, 30.1993, 1100) == 32768
    assert fd.crossfade_weight(0, 1000.0, 11) == 11000


def test_first_echo_reads_left_channel_words() -> None:
    rows = [[0.0] * 4 for _ in range(12)]
    assert fd.first_echo(rows) is None
    rows[11][0] = 0.43  # call 11, first frame: sample 22
    assert fd.first_echo(rows) == 22
    rows[11][0], rows[11][2] = 0.0, 0.43  # second frame of the call: sample 23
    assert fd.first_echo(rows) == 23


def test_rust_table_round_trip_and_oracle_arguments(table: list[int]) -> None:
    src = "pub const FW_DELAY_TABLE: [i16; 3] = [\n" + fd.rust_table([5, -2, 7]) + "\n];"
    assert fd.parse_rust_table(src) == [5, -2, 7]
    ctx = fd.oracle_context(20.0, 0.0, 0x8000, 0.0)
    assert ctx[fd.CTX + 0x15] == fd.f32_bits(20.0) and ctx[fd.CTX + 0x17] == 0x8000
    assert ctx[fd.CTX + 0x14] == 0 and ctx[fd.CTX + 0x21] == 2048
    args = fd.impulse_args("img.bin", ctx, 5)
    assert args[args.index("--calls") + 1] == "5"
    assert args.count("--poke") == 1 + 4 * 4
    assert f"0:{fd.IN_ADDR:#x}={fd.f32_bits(1.0):#x}" in args
    assert "--call" in args and hex(fd.ROUTINE_PC) in args


def test_load_table_reads_the_documented_address(table: list[int]) -> None:
    image = bytearray(0x300000)
    off = fd.TABLE_ADDR - fd.ARM_BASE
    image[off : off + 2 * fd.TABLE_LEN] = struct.pack(f"<{fd.TABLE_LEN}h", *table)
    assert fd.load_table(bytes(image)) == table

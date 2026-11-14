"""Tests for tools/fx_leslie_ring.py: ring pointer pokes and the level readout."""

import fx_leslie_ring as lr


def test_stride_zero_pokes_nothing():
    assert lr.ring_pokes(5, 0) == []


def test_pointers_advance_by_the_stride_and_wrap_in_the_ring():
    p = lr.ring_pokes(3, 4)
    assert len(p) == 3 * 2 * 2
    assert p[1] == "0:0xe3594=0xe3200" and p[3] == "0:0xe319c=0xe2e08"
    assert p[5] == "1:0xe3594=0xe3204"
    wrapped = lr.ring_pokes(300, 4)[-4:]
    assert int(wrapped[1].split("=")[1], 16) == 0xE3200 + (4 * 299) % lr.RING_LEN


def test_rms_db_is_the_amplitude_ratio():
    assert abs(lr.rms_db([0.5, -0.5], [1.0, -1.0]) + 6.0206) < 1e-3

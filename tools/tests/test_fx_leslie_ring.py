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


def test_the_pass_routine_rebases_every_eight_calls_and_moves_the_second_buffer():
    p = lr.ring_pokes(10, lr.PASS_STRIDE, lr.PASS_PERIOD, second=True)
    assert len(p) == 10 * 4 * 2
    by_call = {}
    for tok in p[1::2]:
        key, value = tok.split("=")
        by_call[key] = value
    assert by_call["7:0xe3594"] == "0xe321c"
    assert by_call["8:0xe3594"] == "0xe3200"
    assert by_call["9:0xe3596"] == "0xe35a2"
    assert by_call["9:0xe319e"] == "0xe31aa"


def test_rms_db_is_the_amplitude_ratio():
    assert abs(lr.rms_db([0.5, -0.5], [1.0, -1.0]) + 6.0206) < 1e-3

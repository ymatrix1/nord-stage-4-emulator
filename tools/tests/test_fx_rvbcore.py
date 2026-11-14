"""Tests for tools/fx_model_rvbcore.py, tools/fx_rvbcore_check.py and the circular addressing of
tools/fx_pe.py (no firmware image needed; the oracle comparison is `fx_rvbcore_check.py check`)."""

import hashlib

import fx_model_rvbcore as core
import fx_pe as pe
import fx_rvbcore_check as chk

M = 0xFFFFFFFF


def test_circular_modify_wraps_one_buffer_length_like_the_oracle():
    # base 100, length 4: 103 + 1 wraps to 100, 100 - 1 wraps to 103 (core/dag.rs test)
    assert pe.dag_next(103, 1, 4, 100) == 100
    assert pe.dag_next(100, M, 4, 100) == 103
    assert pe.dag_next(101, 2, 4, 100) == 103
    assert pe.dag_next(103, 2, 4, 100) == 101
    assert pe.dag_next(50, 7, 0, 100) == 57  # L = 0: plain add
    assert pe.dag_next(50, (-7) & M, 0, 100) == 43


def test_machine_mod_uses_the_register_files_and_a_b_write_loads_i():
    m = pe.Machine({}, {"i2": 0, "m2": 2, "m4": (-6) & M})
    m.step(0x10, d={"b2": 0x500, "l2": 8})
    assert m.ir["i2"] == 0x500  # `B2 = ...` also loads `I2`
    assert m.mod(2, 2) == 0x502
    assert m.mod(2, 4) == 0x502  # 0x500 - 6 wraps by one length of 8
    m.step(0x11, d={"i2": 0x506})
    assert m.mod(2, 2) == 0x500


def test_a_signed_zero_is_not_a_trace_mismatch():
    row = {"pc": 5, "r": [0x80000000] + [0] * 15, "s": [0] * 16, "i": [0] * 16}
    m = pe.Machine({}, {}, [row])
    m.x[0] = pe.fl(0)
    m.step(5)  # model +0, oracle -0: equal values


def test_the_model_is_silent_for_silent_input_and_deterministic():
    mem = pe.Missing(chk.layout(seed=1, short=True), default=0)
    for _ in range(5):
        assert chk.run_model([0, 0, 0, 0], mem) == (0, 0, 0, 0)
    assert chk.golden_words(30, 2) == chk.golden_words(30, 2)


def test_golden_words_are_pinned():
    g = chk.golden_words(40, 1)
    assert all(not any(r) for r in g[:14])  # the short line A delays the first output by 14 calls
    assert g[15] == (2208734886, 2208734886, 100477406, 100477406)
    assert g[19] == (2478237677, 2478237677, 359929705, 359929705)
    assert hashlib.sha256(repr(g).encode()).hexdigest()[:16] == "5d602c75f1207c7a"


def test_the_core_reads_the_pointer_words_it_is_given():
    """Line A write pointer `c[41]` advances by one pair (2 words) per call and wraps at `c[40]`."""
    mem = pe.Missing(chk.layout(seed=1, short=True), default=0)
    ptr = chk.CTX + 41
    seen = []
    for _ in range(10):
        chk.run_model([0x3F800000, 0, 0, 0], mem)
        seen.append(mem[ptr] - chk.LA)
    assert seen == [2, 4, 6, 8, 10, 12, 14, 0, 2, 4]  # length 16 words
    assert core.PC == 0x123F61 and core.END == 0x12417D

import struct

import ns4_smp_kernel_fit as k

# Golden words of the SMP kernel table at DM 0xE3400 (VERIFIED against the v1.66 image:
# `python tools/ns4_smp_kernel_fit.py smp.bin` reports 0 mismatches over all 1,536 words).
GOLDEN = {
    (2, 0): 0x7FFFFF00,
    (2, 128): 0x4C6E1C00,
    (2, 64): 0x71622E00,
    (1, 128): -246675456,
    (0, 128): 0x0245DC00,
    (0, 1): 0x00132300,
    (5, 255): 0x00132300,
    (3, 100): 0x3936E200,
    (4, 200): -240518144,
    (1, 1): -6457088,
}


def test_offset_layout():
    assert k.offset(2, 0) == 0.0
    assert k.offset(0, 0) == 2.0
    assert k.offset(5, 0) == -3.0
    assert k.offset(2, 128) == 0.5


def test_sinc_and_hamming():
    assert k.sinc(0.0) == 1.0
    assert abs(k.sinc(2.0)) < 1e-15
    w = k.hamming()
    assert w(0.0) == 1.0
    assert abs(w(3.0) - 0.08) < 1e-15  # 2a - 1 at the edge, not zero
    assert abs(w(1.5) - 0.54) < 1e-15
    assert w(1.2) == w(-1.2)


def test_model_matches_golden_words():
    m = k.model_words()
    assert len(m) == k.WORDS
    for (t, p), word in GOLDEN.items():
        assert m[t * k.PHASES + p] == word, (t, p)


def test_model_structure():
    m = k.model_words()
    assert all(w & 0xFF == 0 for w in m)  # low byte zero
    assert k.mirror_errors(m) == 0
    lo, hi = k.phase_sums(m)
    assert 1 << 23 == 8388608 and abs(lo - 8388608) <= 3 and abs(hi - 8388608) <= 3
    assert m[2 * k.PHASES] == 0x7FFFFF00  # saturated peak
    assert m[1 * k.PHASES] == 0 and m[3 * k.PHASES] == 0  # zero crossings at integer offsets


def test_additive_correction_makes_taps_sum_to_one():
    real = k.model_real(k.hamming())
    for p in (0, 1, 77, 255):
        assert abs(sum(real[t][p] for t in range(k.TAPS)) - 1.0) < 1e-12


def test_read_table_and_compare_roundtrip():
    m = k.model_words()
    image = bytearray(4 * (k.KERNEL_DM + k.WORDS))
    struct.pack_into(f"<{k.WORDS}i", image, 4 * k.KERNEL_DM, *m)
    table = k.read_table(bytes(image))
    assert table == m
    r = k.compare(table, m)
    assert r["mismatches"] == 0 and r["max_lsb"] == 0.0
    assert k.compare(table, k.model_words(k.hamming(0.5)))["mismatches"] > 1000


def test_read_table_rejects_short_image():
    try:
        k.read_table(bytes(16))
    except ValueError:
        return
    raise AssertionError("expected ValueError")


def test_survey_lists_other_windows_and_main_runs(tmp_path, capsys):
    assert "hamming 0.54/0.46" in k.survey_windows() and len(k.survey_windows()) >= 4
    image = bytearray(4 * (k.KERNEL_DM + k.WORDS))
    struct.pack_into(f"<{k.WORDS}i", image, 4 * k.KERNEL_DM, *k.model_words())
    path = tmp_path / "smp.bin"
    path.write_bytes(bytes(image))
    assert k.main([str(path), "--survey"]) == 0
    out = capsys.readouterr().out
    assert "mismatches    0" in out and "mirror violations 0" in out

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_adr_oracle as adr  # noqa: E402
import synth_strip_e2e as e2e  # noqa: E402


def test_bits_to_float_reads_float32_patterns():
    assert e2e.bits_to_float(0x3F800000) == 1.0
    assert e2e.bits_to_float(0xBF800000) == -1.0
    assert e2e.bits_to_float(0) == 0.0


def test_max_abs_error_and_length_check():
    assert e2e.max_abs_error([1.0, 2.0], [1.0, 2.5]) == 0.5
    assert e2e.max_abs_error([], []) == 0.0
    try:
        e2e.max_abs_error([1.0], [1.0, 2.0])
    except ValueError:
        pass
    else:  # pragma: no cover
        raise AssertionError("length mismatch must raise")


def test_word_mismatches_lists_the_differing_indices():
    assert e2e.word_mismatches([1, 2, 3, 4], [1, 9, 3, 8]) == [1, 3]
    assert e2e.word_mismatches([5], [5]) == []


def test_envelope_records_use_the_chain_addresses_and_links():
    spec = {
        "records": {
            "attack": [0x3F800000, 0x3DCCCCCD, 0],
            "decay": [0x3F4CCCCD, 0, 0x3F000000],
            "release": [0x3F666666, 0, 0],
        }
    }
    recs = e2e.envelope_records(spec)
    assert set(recs) == {adr.ATTACK, adr.DECAY, adr.RELEASE}
    assert recs[adr.ATTACK][0] == 1.0 and recs[adr.ATTACK][3] == adr.DECAY
    assert recs[adr.DECAY][2] == 0.5 and recs[adr.DECAY][3] == adr.DECAY
    assert recs[adr.RELEASE][3] == adr.RELEASE


def test_main_prints_usage_without_arguments(capsys):
    assert e2e.main([]) == 2
    assert "Usage" in capsys.readouterr().out

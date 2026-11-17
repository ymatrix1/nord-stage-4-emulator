"""Tests for tools/synth_filter_links.py (pure helpers; the oracle runs need the local image)."""

import struct

import synth_filter_links as m


def test_res_word_is_the_arm_float() -> None:
    assert struct.unpack("<f", struct.pack("<I", m.res_word(0)))[0] == -0.5
    assert abs(struct.unpack("<f", struct.pack("<I", m.res_word(127)))[0] - 0.5) < 1e-6


def test_position_splits_index_and_fraction() -> None:
    assert m.position(0x40000000) == 64.0
    assert abs(m.position(0x40218223) - 64.131) < 1e-3


def test_transient_at_60_passes_is_the_old_law() -> None:
    # 128 (1 - 0.995^60) = 33.25: the "64 + 33.25 w" law is the pass-59 transient
    assert abs(m.POSITION_SPAN * (1 - m.SMOOTHER_POLE**60) - 33.25) < 0.01
    for knob, word in ((0, 0x2F6070C0), (64, 0x40218223), (127, 0x509F8F40)):
        assert abs(m.position_after(knob, 59) - m.position(word)) < 0.01


def test_settled_position_is_128_knob_over_127() -> None:
    assert m.settled_position(0) == 0.0
    assert abs(m.settled_position(127) - 128.0) < 1e-9
    assert abs(m.settled_position(64) - 64.504) < 1e-3


def test_model_matches_recorded_oracle_runs() -> None:
    # recorded `res` run of the ORG core 2 pass, note 60 (n = pass index)
    recorded = {
        (0, 99): 38.77,
        (0, 1499): 0.04,
        (32, 1499): 32.27,
        (64, 599): 64.48,
        (107, 299): 98.10,
        (127, 1499): 127.96,
    }
    for (knob, n), want in recorded.items():
        assert abs(m.position_after(knob, n) - want) < 0.02, (knob, n)


def test_slot_pokes_cover_every_slot() -> None:
    args = m.slot_pokes(0x12566E, 44)
    assert len(args) == 2 * m.SLOTS
    assert args[1] == "44:0xc0b8c=0x12566e"
    assert args[-1] == f"44:{0xC0B8C + 38 * 15:#x}=0x12566e"


def test_count_calls_reads_the_trace_report() -> None:
    text = "\n".join(
        [
            "booted 100000 steps, pc 0x125de0",
            "pass 0: 4309 steps, back in the poll loop",
            "pass 1: 4396 steps, back in the poll loop",
            "  trace pc 12566e R 0 1",
            "  trace pc 12566e R 0 2",
            "  trace pc 125913 R 0 3",
        ]
    )
    got = m.count_calls(text, 0x12566E)
    assert got[0] == (4309, 0)
    assert got[1] == (4396, 2)


def test_calls_command_traces_the_stage_entry() -> None:
    cmd = m.calls_command("sharc-org", "img.bin", "flat.script", 0x1253BD, 60)
    assert cmd[0] == "sharc-org"
    assert "0x1253bd:0x1253bd:100000" in cmd
    assert cmd.count("--msg") == m.SLOTS


def test_write_csv_skips_without_a_path_and_writes_rows(tmp_path) -> None:  # noqa: ANN001
    m.write_csv(None, ["a"], [[1]])
    out = tmp_path / "x.csv"
    m.write_csv(str(out), ["a", "b"], [[1, 2], [3, 4]])
    assert out.read_text("utf-8").splitlines() == ["a,b", "1,2", "3,4"]

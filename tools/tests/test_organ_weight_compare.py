"""Unit tests for tools/organ_weight_compare.py (targets, poke scenario and metrics)."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import organ_oracle_audio as oa
import organ_weight_compare as wc


def test_targets_match_the_firmware_sum() -> None:
    w = wc.firmware_targets([60], [0, 0, 8, 0, 0, 0, 0, 0, 0])
    assert w[36] == 127 * 1008 / 2**23
    assert np.count_nonzero(w) == 1
    # velocity 64: (64 * 127) >> 4 = 508
    assert wc.firmware_targets([60], [0, 0, 8, 0, 0, 0, 0, 0, 0], 64)[36] == 127 * 508 / 2**23
    chord = wc.firmware_targets([60, 64, 67], [8, 8, 8, 0, 0, 0, 0, 0, 0])
    assert chord[43] == 2 * w[36]  # G4 8' and C4 5 1/3' share wheel 43


def test_scenario_has_key_down_on_held_wheels_and_a_full_key_off() -> None:
    pokes = wc.scenario_pokes([60], [0, 0, 8, 0, 0, 0, 0, 0, 0], 127, keyoff_call=10)
    at0 = [p for p in pokes if p[0] == 0 and oa.A_BASE <= p[1] < oa.A_BASE + oa.WHEELS]
    assert at0 == [(0, oa.A_BASE + 36, wc.A_KEY_DOWN)]
    off = [p for p in pokes if p[0] == 10 and oa.WB_BASE <= p[1] < oa.WB_BASE + oa.WHEELS]
    assert len(off) == oa.WHEELS and all(p[2] == 0 for p in off)
    assert (0, wc.STATE_LEVEL[0], oa.f2u(1.0)) in pokes  # smoother states start settled


def test_update_call_sends_the_full_update_to_every_wheel() -> None:
    pokes = wc.scenario_pokes([60], [0, 0, 8, 0, 0, 0, 0, 0, 0], 127, 10, update_call=5)
    a = [p for p in pokes if p[0] == 5 and oa.A_BASE <= p[1] < oa.A_BASE + oa.WHEELS]
    assert len(a) == oa.WHEELS and {p[2] for p in a} == {wc.A_UPDATE}


def test_block_rms_envelope_and_snr_metrics() -> None:
    x = np.concatenate([np.ones(64), 2 * np.ones(64)])
    assert list(wc.block_rms(x)) == [1.0, 2.0]
    ref = np.sin(np.arange(4096) * 0.1)
    level, snr = wc.snr_after_gain(ref, 0.5 * ref)
    assert abs(level - 0.5) < 1e-9 and snr > 200
    mx, rms = wc.envelope_error(ref, ref, 0, 2048)
    assert mx == 0.0 and rms == 0.0
    mx, _ = wc.envelope_error(ref, 1.1 * ref, 0, 2048)
    assert 0.05 < mx < 0.1


def test_read_rust_csv_skips_header_lines(tmp_path: Path) -> None:
    f = tmp_path / "r.csv"
    f.write_text("# mode: layer\n# rate: 43156\nindex,L,R\n0,0.5,-0.5\n1,1,2\n", encoding="utf-8")
    assert wc.read_rust_csv(f).tolist() == [[0.5, -0.5], [1.0, 2.0]]


def test_report_lists_level_and_envelope_lines() -> None:
    t = np.arange(8192)
    sig = np.stack([np.sin(t * 0.05), np.sin(t * 0.05)], axis=1) * np.minimum(t / 300, 1)[:, None]
    lines = wc.report(sig, sig, keyoff=4096)
    assert any("level 1.0000x" in line for line in lines)
    assert any("attack envelope error" in line for line in lines)

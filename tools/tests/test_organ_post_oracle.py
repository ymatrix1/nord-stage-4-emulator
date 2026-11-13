"""Unit tests for tools/organ_post_oracle.py (fake oracle executable; no firmware needed)."""

from __future__ import annotations

import numpy as np
import organ_oracle_audio as oa
import organ_post_oracle as po
import organ_post_ref as pr


def fake_run(calls_log, word=0x3F800000):
    """A stand-in for subprocess.run that records the command and writes constant words."""

    def run(cmd, check, capture_output):
        calls_log.append(cmd)
        total = sum(int(c.split(":")[1], 0) for i, c in enumerate(cmd) if cmd[i - 1] == "--range")
        n = int(cmd[cmd.index("--calls") + 1])
        out = cmd[cmd.index("--out") + 1]
        np.full(total * n, word, dtype="<u4").tofile(out)

    return run


def test_run_chain_builds_the_command_and_shapes_the_output(monkeypatch):
    log: list = []
    monkeypatch.setattr(po.subprocess, "run", fake_run(log))
    rows = po.run_chain("exe", "img", {0x10: 7}, [po.POST_STEP], ((0xE0000, 4), (0xB0000, 2)), 3)
    cmd = log[0]
    assert rows.shape == (3, 6)
    assert cmd[cmd.index("--step") + 1] == po.POST_STEP
    assert [c for i, c in enumerate(cmd) if cmd[i - 1] == "--range"] == ["0xe0000:4", "0xb0000:2"]
    assert "--script" in cmd and "--calls" in cmd


def test_snapshot_reads_the_windows_in_order():
    a = po.snapshot(np.array([1, 2, 3, 4, 5], dtype=np.uint32), ((0xE0000, 3), (0xB0000, 2)))
    assert (a[0xE0000], a[0xE0002], a[0xB0000], a[0xB0001]) == (1, 3, 4, 5)
    assert a[0x123] == 0  # outside the windows: zero


def test_compare_lists_differing_words_and_ignores_the_saved_i0():
    a = pr.Mem({0xE0001: 1, 0xE210E: 1, 0xE210F: 2, 0xB0005: 7})
    b = pr.Mem({0xE0001: 1, 0xE210E: 9, 0xE210F: 9, 0xB0005: 8, 0xE2110: 3})
    assert po.compare(a, b) == [0xE2110, 0xB0005]
    assert po.compare(a, pr.Mem(a)) == []


def test_chain_pokes_put_the_chord_on_the_note_oscillators():
    p = po.chain_pokes([60, 64, 67], 0x240B700)
    assert p[pr.DEPTH] == 0x240B700 and p[pr.OFFSET] == 0x40000000
    ones = [i for i in range(96) if p[pr.OUT_WEIGHTS + i] == oa.f2u(1.0)]
    assert ones == [36, 40, 43]
    assert p[pr.OUT_RECORD - 0x1F] == pr.OUT_WEIGHTS
    assert p[pr.OUT_PARAMS] == oa.ONE and p[pr.OUT_PARAMS + 6] == 0
    assert all(p[s + 6] == oa.ONE for s in pr.OUT_STATES)
    assert any(a == oa.INC_ADDR for a in p)  # the tonewheel bank increments are poked too


def test_note_oscillator_frequency_follows_the_semitone_ladder():
    assert po.osc_cycles(0) == pr.RATE_BASE / 2**31 / 4
    assert abs(po.osc_cycles(12) / po.osc_cycles(0) - 2.0) < 1e-12
    assert abs(po.osc_cycles(1) / po.osc_cycles(0) - 2 ** (1 / 12)) < 1e-12


def test_verify_scenario_uses_the_model_on_the_pre_state(monkeypatch):
    # a fake oracle that returns an all-zero memory: the model must differ from it
    monkeypatch.setattr(po.subprocess, "run", fake_run([], word=0))
    assert po.verify_scenario("exe", "img", 0, "output") != []
    assert po.verify_scenario("exe", "img", 0, "post") != []

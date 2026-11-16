"""Tests for tools/synth_train.py (helpers; the training loop is exercised by a real run)."""

from __future__ import annotations

import collections
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_train as st  # noqa: E402


def test_clip_value_warms_up_then_uses_quantile():
    h: collections.deque = collections.deque(maxlen=st.GRAD_WINDOW)
    assert st.clip_value(h) == float("inf")
    h.extend(range(1, 11))  # 1..10
    assert st.clip_value(h) == pytest.approx(8.2)  # 0.8 quantile


def test_to_notes_builds_transkun_notes():
    pytest.importorskip("transkun")
    seg = [
        [
            {
                "start": 0.5,
                "end": 1.0,
                "pitch": 60,
                "velocity": 70,
                "has_onset": True,
                "has_offset": False,
            }
        ]
    ]
    (n,) = st.to_notes(seg)[0]
    assert (n.start, n.end, n.pitch, n.velocity) == (0.5, 1.0, 60, 70)
    assert n.hasOnset is True and n.hasOffset is False


def test_parser_defaults_follow_the_plan():
    a = st.build_parser().parse_args(["data", "--out", "run"])
    assert (a.arm, a.steps, a.eval_every, a.snapshot_every) == ("lokr", 8000, 500, 500)
    assert (a.bs, a.seg, a.lr, a.bf16) == (4, 16.0, 2e-4, False)
    assert a.resume is None
    c = st.build_parser().parse_args(["data", "--out", "run", "--resume", "run/last"])
    assert c.resume == "run/last"
    b = st.build_parser().parse_args(["data", "--out", "run", "--arm", "lora", "--r", "32"])
    assert b.arm == "lora" and b.r == 32


def test_schedule_flags_and_make_schedule():
    torch = pytest.importorskip("torch")
    d = st.build_parser().parse_args(["data", "--out", "run"])
    assert (d.schedule, d.warmup, d.cooldown) == ("onecycle", 150, 0)
    a = st.build_parser().parse_args(
        ["data", "--out", "run", "--schedule", "wsd", "--steps", "1000", "--cooldown", "200"]
    )
    opt = torch.optim.AdamW(torch.nn.Linear(2, 2).parameters(), lr=a.lr)
    wsd = st.make_schedule(opt, a)
    assert type(wsd).__name__ == "WsdSchedule" and wsd.cooldown == 200
    assert type(st.make_schedule(opt, d)).__name__ == "OneCycleLR"

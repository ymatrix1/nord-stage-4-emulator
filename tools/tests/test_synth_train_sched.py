"""Tests for tools/synth_train_sched.py."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_train_sched as sch  # noqa: E402


def test_factor_warms_up_holds_then_cools_down():
    f = lambda s: sch.wsd_factor(s, total=1000, warmup=100, cooldown=200)  # noqa: E731
    assert f(0) == pytest.approx(1 / 20)
    assert f(50) == pytest.approx(1 / 20 + (1 - 1 / 20) * 0.5)
    assert f(100) == f(500) == f(799) == 1.0
    assert f(800) == 1.0 and f(900) == pytest.approx(1 - (1 - 1 / 40) * 0.5)
    assert f(1000) == pytest.approx(1 / 40) and f(5000) == pytest.approx(1 / 40)


def test_cooldown_zero_never_decays():
    assert sch.wsd_factor(10**6, total=10**6, warmup=10, cooldown=0) == 1.0


def test_schedule_sets_the_optimizer_rate_and_restores_from_state():
    torch = pytest.importorskip("torch")
    net = torch.nn.Linear(2, 2)
    opt = torch.optim.AdamW(net.parameters(), lr=0.1)
    s = sch.WsdSchedule(opt, 0.1, total=100, warmup=10, cooldown=20)
    assert opt.param_groups[0]["lr"] == pytest.approx(0.1 / 20)
    for _ in range(50):
        s.step()
    assert s.get_last_lr() == [pytest.approx(0.1)] and s.state_dict() == {"step": 50}
    other = sch.WsdSchedule(opt, 0.1, total=70, warmup=10, cooldown=20)  # extended differently
    other.load_state_dict(s.state_dict())
    assert other.n == 50 and other.get_last_lr() == [pytest.approx(0.1)]


def test_reads_the_step_of_a_onecycle_state():
    torch = pytest.importorskip("torch")
    net = torch.nn.Linear(2, 2)
    opt = torch.optim.AdamW(net.parameters(), lr=0.1)
    one = torch.optim.lr_scheduler.OneCycleLR(opt, 0.1, total_steps=30, cycle_momentum=False)
    for _ in range(12):
        opt.step()
        one.step()
    s = sch.WsdSchedule(opt, 0.01, total=100, warmup=5, cooldown=0)
    s.load_state_dict(one.state_dict())
    assert s.n == 12 and s.get_last_lr() == [pytest.approx(0.01)]

"""Tests for tools/synth_train_state.py (exact-resume state)."""

from __future__ import annotations

import collections
import random
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_train_state as sts  # noqa: E402

torch = pytest.importorskip("torch")

TOTAL = 10


def make(seed: int = 0):
    torch.manual_seed(seed)
    net = torch.nn.Linear(4, 2)
    opt = torch.optim.AdamW(net.parameters(), lr=0.1)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, 0.1, total_steps=TOTAL, cycle_momentum=False)
    return net, opt, sched, collections.deque(maxlen=100), random.Random(5)


def run(net, opt, sched, history, rng, steps):
    for _ in range(steps):
        x = torch.tensor([[rng.random() for _ in range(4)]])
        opt.zero_grad()
        net(x).sum().backward()
        history.append(float(torch.nn.utils.clip_grad_norm_(net.parameters(), 10.0)))
        opt.step()
        sched.step()


def test_resume_matches_an_uninterrupted_run(tmp_path):
    ref, ropt, rsched, rhist, rrng = make()
    run(ref, ropt, rsched, rhist, rrng, TOTAL)

    net, opt, sched, hist, rng = make()
    run(net, opt, sched, hist, rng, 4)
    sts.save_state(
        tmp_path,
        step=4,
        total_steps=TOTAL,
        opt=opt,
        sched=sched,
        history=hist,
        best=12.5,
        elapsed=33.0,
        sampler_rng=rng,
    )
    weights = {k: v.clone() for k, v in net.state_dict().items()}

    net2, opt2, sched2, hist2, rng2 = make(seed=99)  # different init and RNG on purpose
    net2.load_state_dict(weights)
    step, best, elapsed = sts.apply_state(
        sts.load_state(tmp_path),
        total_steps=TOTAL,
        opt=opt2,
        sched=sched2,
        history=hist2,
        sampler_rng=rng2,
    )
    assert (step, best, elapsed) == (4, 12.5, 33.0)
    assert list(hist2) == list(hist)
    run(net2, opt2, sched2, hist2, rng2, TOTAL - step)
    for a, b in zip(ref.parameters(), net2.parameters(), strict=True):
        assert torch.allclose(a, b, atol=1e-7)


def test_step_count_mismatch_is_rejected(tmp_path):
    net, opt, sched, hist, rng = make()
    sts.save_state(
        tmp_path,
        step=1,
        total_steps=TOTAL,
        opt=opt,
        sched=sched,
        history=hist,
        best=1.0,
        elapsed=0.0,
        sampler_rng=rng,
    )
    with pytest.raises(ValueError, match="steps"):
        sts.apply_state(
            sts.load_state(tmp_path),
            total_steps=TOTAL + 1,
            opt=opt,
            sched=sched,
            history=hist,
            sampler_rng=rng,
        )


def test_missing_state_file_names_the_problem(tmp_path):
    with pytest.raises(FileNotFoundError, match="resume state"):
        sts.load_state(tmp_path)


def test_check_schedule_rules():
    onecycle = {"schedule": "onecycle", "total_steps": 100}
    legacy = {"total_steps": 100}  # states saved before the schedule key existed
    sts.check_schedule(onecycle, "onecycle", 100)
    sts.check_schedule(legacy, "onecycle", 100)
    with pytest.raises(ValueError, match="wsd"):
        sts.check_schedule(onecycle, "onecycle", 200)
    sts.check_schedule(onecycle, "wsd", 200)  # extending a OneCycle run is allowed
    with pytest.raises(ValueError, match="schedule"):
        sts.check_schedule({"schedule": "wsd", "total_steps": 9}, "onecycle", 9)

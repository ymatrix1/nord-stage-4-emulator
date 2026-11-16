#!/usr/bin/env python3
"""Learning-rate schedule for ``synth_train.py`` that can be extended: warm-up, constant, cool-down.

OneCycle (the default) decays the rate toward the end of a fixed number of steps, so a run cannot
be lengthened afterwards and snapshots from the middle of a long run are un-annealed. This
schedule (warm-up, stable, decay, "WSD") keeps the rate constant until the last ``cooldown``
steps. Train with ``--cooldown 0`` for as long as wanted; to get a finished model, resume from
any snapshot with ``--steps <snapshot step + N> --cooldown N``. To keep training instead, resume
again from the snapshot.

The rate depends only on the step number, so ``state_dict`` is just the step. It also reads the
``last_epoch`` of a OneCycle state, so a OneCycle run can be continued with this schedule.

ASSUMED: the endpoints match the OneCycle settings of the plan (start at ``lr / 20``, finish at
``lr / 40``, the upstream ``div_factor`` 20 and ``final_div_factor`` 2); the linear shapes are the
usual WSD choice, not tuned here. Torch is not imported here (the optimizer is passed in).
"""

from __future__ import annotations

from typing import Any

START_FRAC = 1 / 20
FLOOR_FRAC = 1 / 40


def wsd_factor(step: int, total: int, warmup: int, cooldown: int) -> float:
    """Multiplier of the peak rate at ``step`` (0-based) for a run of ``total`` steps."""
    if warmup > 0 and step < warmup:
        return START_FRAC + (1.0 - START_FRAC) * step / warmup
    if cooldown > 0 and step >= total - cooldown:
        frac = min(1.0, (step - (total - cooldown)) / cooldown)
        return 1.0 - (1.0 - FLOOR_FRAC) * frac
    return 1.0


class WsdSchedule:
    """Scheduler with the small part of the torch scheduler API the trainer uses."""

    def __init__(self, opt: Any, lr: float, total: int, warmup: int, cooldown: int):
        self.opt, self.lr = opt, lr
        self.total, self.warmup, self.cooldown = total, warmup, cooldown
        self.n = 0
        self._apply()

    def _apply(self) -> None:
        value = self.lr * wsd_factor(self.n, self.total, self.warmup, self.cooldown)
        for group in self.opt.param_groups:
            group["lr"] = value
        self._last = [value]

    def step(self) -> None:
        self.n += 1
        self._apply()

    def get_last_lr(self) -> list[float]:
        return list(self._last)

    def state_dict(self) -> dict[str, int]:
        return {"step": self.n}

    def load_state_dict(self, state: dict[str, Any]) -> None:
        self.n = int(state.get("step", state.get("last_epoch", 0)))
        self._apply()

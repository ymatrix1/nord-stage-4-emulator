#!/usr/bin/env python3
"""System identification of the Core2 stage `0x1257B2` (the "ladder-like" low pass).

The stage is not decoded instruction by instruction (SIMD cross-lane swaps, six coefficient
fields); this module measures its impulse response in the oracle (`tools/synth_stage_oracle.py`)
and fits a linear recurrence to it, which gives the poles. Result
(`docs/research/synth-adr-and-strip-law.md` section 5): **four real poles**, the impulse response
is an order-4 linear system to 1e-9 for every (R, resonance index) checked, DC gain 1 for
index >= 64.

Needs the local ORG core 2 image and `sharc-org` for `measure`; `prony_poles` is pure numpy.
"""

from __future__ import annotations

import numpy as np

# EMPIRICAL: poles from `measure` (impulse of 384 samples, order-4 fit, max fit error < 1.2e-8).
# (R, index) -> sorted real poles, DC gain of the response.
MEASURED = {
    (0.1, 0): ((0.75016, 0.85715, 0.96774, 0.99778), 1.27149),
    (0.1, 64): ((0.75019, 0.85704, 0.93446, 0.95237), 1.0),
    (0.1, 100): ((0.42972, 0.71384, 0.75253, 0.85716), 1.0),
    (0.3, 64): ((0.28754, 0.65876, 0.93424, 0.9525), 1.0),
    (0.3, 100): ((0.30573, 0.43573, 0.65839, 0.72018), 1.0),
    (0.3, 120): ((-0.12644, 0.27001, 0.27694, 0.65877), 1.0),
}


def prony_poles(h: np.ndarray, order: int = 4, skip: int = 8) -> tuple[np.ndarray, float]:
    """Poles of the order-`order` recurrence that best explains the tail of the response `h`.

    Least squares on `h[k] = sum_j c_j h[k - j]` for `k >= order + skip`; returns the roots of
    `z^n - sum c_j z^(n-j)` and the largest residual.
    """
    n = len(h)
    rows = np.array([[h[k - j] for j in range(1, order + 1)] for k in range(order + skip, n)])
    rhs = h[order + skip : n]
    c, *_ = np.linalg.lstsq(rows, rhs, rcond=None)
    err = float(np.max(np.abs(rows @ c - rhs)))
    return np.roots(np.concatenate(([1.0], -c))), err


def cascade_impulse(poles: list[float], n: int) -> np.ndarray:
    """Impulse response of a cascade of one-pole sections `1 / (1 - p z^-1)`, DC gain not forced."""
    h = np.zeros(n)
    h[0] = 1.0
    for p in poles:
        y = np.zeros(n)
        acc = 0.0
        for k in range(n):
            acc = h[k] + p * acc
            y[k] = acc
        h = y
    return h


def measure(sharc_org: str, image: str, r: float, index: int, n: int = 384):
    """Impulse response of the stage in the oracle for the coefficient `r` and resonance `index`."""
    import synth_stage_oracle as o

    frame = {-30: (o.f2u(r), o.f2u(r)), -28: (index << 24, index << 24)}
    for off in (-10, -8, -6, -4, -2, 0):
        frame[off] = (0, 0)
    out = o.run_stage(sharc_org, image, 0x1257B2, frame, [1.0] + [0.0] * (n - 1))
    return np.array(out["x"], dtype=np.float64)


def main() -> None:
    import synth_stage_oracle as o

    org, image = o.default_paths()
    for (r, idx), (poles, dc) in MEASURED.items():
        h = measure(org, image, r, idx)
        got, err = prony_poles(h)
        print(r, idx, np.round(np.sort(got.real), 5), "dc", round(float(h.sum()), 5), "err", err)
        print("   recorded", poles, dc)


if __name__ == "__main__":
    main()

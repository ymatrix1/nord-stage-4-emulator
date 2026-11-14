#!/usr/bin/env python3
"""Closed-form (float64) algorithms of the FX routine models, as written in
`docs/research/fx-routine-models.md`. Each function takes the scenario memory (`address -> word`)
and returns `{address: value}` for the words the routine writes. `tests/test_fx_closed_forms.py`
checks them against the bit-exact transliterations within a relative tolerance, which is what ties
the equations in the document to the oracle-verified models.
"""

from __future__ import annotations

import struct
from fractions import Fraction

import fx_model_allpass as ap
import fx_model_lfo as lfo_m
import fx_model_resonant as r2
import fx_model_resonant4 as r4
from fx_pe import fix_word, signed32

M32 = 0xFFFFFFFF
#: float32 0x3F7DA3C2 = 0.99078, the smoothing coefficient of the ORG, delay and LFO routines
K = 0.9907799959182739


def f32(word: int) -> float:
    """A float32 memory word as a number (zero exponent field flushes to zero)."""
    if (word >> 23) & 0xFF == 0:
        return 0.0
    return struct.unpack("<f", struct.pack("<I", word & M32))[0]


def clip(v: float, bound: float) -> float:
    return max(-abs(bound), min(abs(bound), v))


def smooth(target: float, state: float, k: float) -> float:
    """One-pole step toward `target`: `target + k (state - target)`."""
    return target + k * (state - target)


def resonant2(mem: dict[int, int]) -> dict[int, float]:
    """`0x1243F4`: 2 stereo frames, parameter smoothing first, input smoother + state-variable
    section with a clipped integrator + a one-pole shelf on the output."""
    ctx, inp = r2.CTX, r2.IN_ADDR
    c = lambda off: f32(mem[ctx + off])  # noqa: E731
    k, lim, a = c(-0x11), c(-0xE), c(-0x14)
    A, B = smooth(c(-0x20), c(-0x1E), k), smooth(c(-0x1F), c(-0x1D), k)
    C, D = smooth(c(-0x17), c(-0x15), k), smooth(c(-0x18), c(-0x16), k)
    E, F = smooth(c(-0x1C), c(-0x1A), k), smooth(c(-0x1B), c(-0x19), k)
    G = (lim - E) / 2
    out = {
        ctx - 0x1E: A, ctx - 0x1D: B, ctx - 0x15: C, ctx - 0x16: D, ctx - 0x1A: E,
        ctx - 0x19: F, ctx - 0x13: G, ctx - 0x12: (lim - F) / 2,
    }  # fmt: skip
    for ch in (0, 1):
        u, xp, w, z, q = (c(o + ch) for o in (-0xD, -0x9, -0x7, -0x5, -0xB))
        for n in (0, 1):
            x = f32(mem[inp + 2 * n + ch])
            u += a * (x - u)
            s = x + A * u
            z += B * w
            w = clip(w + B * ((s + xp) / 2 - z - C * w), lim)
            y = s + D * w
            rr = G * y
            d = rr - q
            out[inp + 2 * n + ch] = y + F * d
            q = rr + E * d
            xp = x
        for off, v in zip((-0xD, -0x9, -0x7, -0x5, -0xB), (u, xp, w, z, q), strict=True):
            out[ctx + off + ch] = v
    return out


def resonant4(mem: dict[int, int]) -> dict[int, float]:
    """`0x124497`: 4 stereo frames, per frame a shelf on the input, the state-variable section,
    a one-pole smoother and a feed-forward output; parameters are smoothed after the frames."""
    ctx, inp = r4.CTX, r4.IN_ADDR
    c = lambda off: f32(mem[ctx + off])  # noqa: E731
    k, lim = c(-0x11), c(-0xE)
    p13, p19, p1a, p1d, p15, p16, p14, p1e = (
        c(o) for o in (-0x13, -0x19, -0x1A, -0x1D, -0x15, -0x16, -0x14, -0x1E)
    )  # fmt: skip
    out = {}
    for ch in (0, 1):
        u, sp, w, z, q = (c(o + ch) for o in (-0xD, -0x9, -0x7, -0x5, -0xB))
        for n in range(4):
            x = f32(mem[inp + 2 * n + ch])
            rr = x * p13
            d = rr - q
            s = x + d * p19
            q = rr + d * p1a
            z += w * p1d
            w = clip(w + p1d * ((s + sp) / 2 - z - w * p15), lim)
            sp = s
            y = s + w * p16
            u += p14 * (y - u)
            out[inp + 2 * n + ch] = y + u * p1e
        for off, v in zip((-0xD, -0x9, -0x7, -0x5, -0xB), (u, sp, w, z, q), strict=True):
            out[ctx + off + ch] = v
    e, f = smooth(c(-0x1C), c(-0x1A), k), smooth(c(-0x1B), c(-0x19), k)
    out.update({
        ctx - 0x1E: smooth(c(-0x20), c(-0x1E), k), ctx - 0x1D: smooth(c(-0x1F), c(-0x1D), k),
        ctx - 0x1A: e, ctx - 0x19: f, ctx - 0x16: smooth(c(-0x18), c(-0x16), k),
        ctx - 0x15: smooth(c(-0x17), c(-0x15), k), ctx - 0x13: (lim - e) / 2,
        ctx - 0x12: (lim - f) / 2,
    })  # fmt: skip
    return out


def allpass(mem: dict[int, int]) -> dict[int, float]:
    """`0x122C39`: LFO gain `g` from a free-running counter, two parallel biquads per channel
    (`N` and `F`), wet / dry mix `m`: `out = g ((1 - m) x + m F(x)) + m N(x)`."""
    ctx, inp = ap.CTX, ap.IN_ADDR
    c = lambda off: f32(mem[ctx + off])  # noqa: E731
    k, m, count = c(3), c(2), mem[ctx + 4]
    ptr = mem[ctx - 0x1D]
    phase = (mem[ctx - 0x1F] + mem[ctx - 0x20]) & M32  # integer phase sum, both words
    out = {ctx - 0x1F: phase, ctx - 0x1E: phase}
    xi = (mem[ptr] * mem[ctx - 0x1C] - mem[ctx - 0x1B]) & M32
    xi -= 1 << 32 if xi & 0x80000000 else 0
    u = 2 * abs(xi * 2.0**-31) - 1  # triangle in [-1, 1)
    s_l = c(-0x13) * u + c(-0x12) * u**3 + c(-0x11) * u**5  # odd polynomial (sine)
    a_l, a_r = smooth(c(-0x1A), c(-0x18), k), smooth(c(-0x19), c(-0x17), k)
    s_l, s_r = smooth(s_l, c(-0x15), k), smooth(c(-0x16), c(-0x14), k)  # right: constant c[-0x16]
    out.update({ctx - 0x18: a_l, ctx - 0x17: a_r, ctx - 0x15: s_l, ctx - 0x14: s_r})
    g = clip(s_l * a_l, s_r) + a_r  # the right words are read back as broadcasts
    near = [c(o) for o in (-7, -6, -5, -4, -3)]  # b0 b1 b2 a1 a2
    far = [c(o) for o in (-0x10, -0xF, -0xE, -0xD, -0xC)]
    for ch in (0, 1):
        stn = [c(-2 + ch), c(0 + ch)]
        stf = [c(-0xB + ch), c(-9 + ch)]
        for it in range(count):
            for fr in (0, 1):
                addr = inp + 4 * it + 2 * fr + ch
                x = out.get(addr, f32(mem[addr]))
                ys = []
                for (b0, b1, b2, a1, a2), st in ((near, stn), (far, stf)):
                    y = x * b0 + st[0]
                    st[0], st[1] = (x * b1 + st[1]) + y * a1, y * a2 + x * b2
                    ys.append(y)
                out[addr] = ((x - m * x) + m * ys[1]) * g + ys[0] * m
        out[ctx - 2 + ch], out[ctx + ch] = stn
        out[ctx - 0xB + ch], out[ctx - 9 + ch] = stf
    return out


def lerp(mem: dict[int, int], base: int, idx: int, frac: float) -> float:
    """Linear interpolation in a float table: `T[i] + frac (T[i + 1] - T[i])`."""
    lo, hi = f32(mem[base + idx]), f32(mem[base + idx + 1])
    return lo + frac * (hi - lo)


def lfo_tdf2(x: float, st: list[float], b: tuple, a: tuple) -> float:
    """Transposed DF II step: `y = b0 x + z1; z1' = b1 x + z2 - a1 y; z2' = b2 x - a2 y`."""
    y = b[0] * x + st[0]
    st[0], st[1] = b[1] * x + st[1] - a[0] * y, b[2] * x - a[1] * y
    return y


def lfo(mem: dict[int, int]) -> dict[int, float]:
    """`0x1228CF`: phase -> two table LFOs (`G1`, `G2`), then per block of 2 frames a TDF2
    section, a high-pass `e = p (a_n - a_{n-1}) - q e_{n-1}`, a table waveshaper mixed with the dry
    signal, a second TDF2 section and a second high-pass."""
    ctx, inp = lfo_m.CTX, lfo_m.IN_ADDR
    c = lambda off: f32(mem[ctx + off])  # noqa: E731
    ph = smooth(c(-0x20), c(-0x1F), K)
    out = {ctx - 0x1F: ph, ctx - 0x1E: ph}
    shift = signed32(mem[ctx - 2])
    word = fix_word(Fraction(ph), shift)
    idx, frac = word >> 24, (word & 0xFFFFFF) / 2.0**24
    g1 = lerp(mem, mem[ctx - 0x15], idx, frac) * c(-0xD)
    g2 = lerp(mem, mem[ctx - 0x14], idx, frac) * (ph * c(-0xC) + c(-0xB))
    mix = clip(ph * 20.0, c(0))
    out.update({ctx - 0xA: g1, ctx - 9: g1, ctx - 8: g2, ctx - 7: g2})
    out.update({ctx - 0x1D: mix, ctx - 0x1C: mix, ctx - 0x1B: c(0) - mix, ctx - 0x1A: c(0) - mix})
    p, q, lim = c(-0x13), c(-0x12), c(1)
    s1b, s1a = (c(7), c(8), c(9)), (c(0xA), c(0xB))
    s2b, s2a = (c(0x10), c(0x11), c(0x12)), (c(0x13), c(0x14))
    for ch in (0, 1):
        z1, z2 = [c(3 + ch), c(5 + ch)], [c(0xC + ch), c(0xE + ch)]
        a_prev, e_prev, v_prev, e2_prev = c(-0x11 + ch), c(-0xF + ch), c(-6 + ch), c(-4 + ch)
        for it in range(mem[ctx + 0x15]):
            for fr in (0, 1):
                addr = inp + 4 * it + 2 * fr + ch
                y = lfo_tdf2(out.get(addr, f32(mem[addr])), z1, s1b, s1a)
                out[ctx - 0x19 + 2 * fr + ch] = y
                a_now = y * g1
                e = p * (a_now - a_prev) - q * e_prev
                a_prev, e_prev = a_now, e
                w2 = fix_word(Fraction(clip(e, lim)), shift)
                i2 = signed32(w2) >> 22
                f2 = (w2 & 0x3FFFFF) / 2.0**22
                shaped = lerp(mem, mem[ctx + 2], i2, f2)
                z = shaped * g2 * mix + y * (c(0) - mix)
                y2 = lfo_tdf2(z, z2, s2b, s2a)
                v = 2 * y2
                e2 = p * (v - v_prev) - q * e2_prev
                v_prev, e2_prev = v, e2
                out[addr] = e2
        out[ctx - 0x11 + ch], out[ctx - 0xF + ch] = a_prev, e_prev
        out[ctx - 6 + ch], out[ctx - 4 + ch] = v_prev, e2_prev
        out[ctx + 3 + ch], out[ctx + 5 + ch] = z1
        out[ctx + 0xC + ch], out[ctx + 0xE + ch] = z2
    return out

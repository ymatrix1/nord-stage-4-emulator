#!/usr/bin/env python3
"""Bit-exact model of the ANALOG SHAPE SINE wave 4 stage `0x120DAB` (ORG core 2, 40-bit registers).

Listing `0x241B56..0x241EAA` (read word by word, `docs/research/synth-stage-models-4.md`). Per lane,
with `a = inc1 2^-31`, `s5 = DM(-5, I2)`, `f` = pitch word:

    g    = C (B + clip(A - a, B))                    saw fade constants of the helper `0x12501F`
    q    = max(eps, 1 - (1 - clip(10 s5, 1))^2)      eps = 0x358637BD
    pq   = f q  (stored),  ir = (1 / 2 pi) / pq  (stored),  k = (pi 2/3) pq
    p    = (((c0 s5 + c1) s5 + c2) s5 + c3) s5 + 1   (stored)
    y_i  = |sine(ph_i) (1 + s5) + (1 - s5)|           sine = polynomial helper `0x12522E`
    w_i  = 1 - min(1, |y_i ir|)
    out_i = y_i(stored) + w_i^3 k - p

then the two oscillators are mixed like every other stage (`mix_lanes`) with the scale `g P[16]`.
"""

from __future__ import annotations

import synth_stage_shape_sine as ss
import synth_wave_model as m

EPS = m.ld(0x358637BD)  # VERIFIED (listing 0x241BD4): lower bound of the width law
TEN = m.ld(0x41200000)
POLY = tuple(m.ld(w) for w in (0x3EFC9646, 0xBFBB9043, 0x3FEC7011, 0xBF1A171A))
INV_TWO_PI = m.ld(0x3E22F981)
PI = m.ld(0x40490FD0)
TWO_THIRDS = m.ld(0x3F2AAA3B)  # 0.6667 as the listing loads it (0x241CF8)


def setup(pitch_word: int, inc1: int, spread: int):
    """`(g, p, ir, k)` of one lane (listing `0x241B82..0x241C60`)."""
    a = m.flt(inc1, -31)
    g = m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, a), m.B_FADE), m.B_FADE))))
    s5 = m.ld(spread)
    w = m.clip(m.mul(TEN, s5), m.ONE)
    w = m.sub(m.ONE, m.mul(m.sub(m.ONE, w), m.sub(m.ONE, w)))
    q = max(EPS, w)
    pq = m.ld(m.st(m.mul(m.ld(pitch_word), q)))
    p = m.mul(POLY[0], s5)
    for c in POLY[1:]:
        p = m.mul(m.add(p, c), s5)
    p = m.ld(m.st(m.add(p, m.ONE)))
    ir = m.ld(m.st(m.recip_refined(pq, INV_TWO_PI)))
    k = m.mul(m.mul(PI, TWO_THIRDS), pq)
    return g, p, ir, k


def osc(phase: int, inc: int, s5: m.X, p: m.X, ir: m.X, k: m.X):
    """`(four samples, new phase)` of one oscillator."""
    ph = [m.u32(phase + j * inc) for j in (1, 2, 3, 4)]
    out = []
    for x in ph:
        y = abs(m.add(m.mul(ss.sine_of_word(x), m.add(s5, m.ONE)), m.sub(m.ONE, s5)))
        w = m.sub(m.ONE, m.clip(m.ONE, m.mul(y, ir)))
        cube = m.mul(m.mul(m.mul(w, w), w), k)
        out.append(m.sub(m.add(cube, m.ld(m.st(y))), p))
    return out, ph[3]


HP_IDLE = (0, 0, 0x3F7F9DB2)  # VERIFIED (dump of `DM(0xC22D6..0xC22DA)` after a pass): states 0, c


def output_high_pass(words: list[int], hp: tuple) -> list[int]:
    """The tail of the stage (listing `0x241E4E..0x241EA6`): a one-pole high pass over the four
    stored output words, `y_k = (x_k - x_{k-1}) + c y_{k-1}`, with the state words `x_-1`, `y_-1`
    and the coefficient `c` in memory (`(state 0, state 1, c)`); each `y_k` is stored."""
    s0, s1, c = m.ld(hp[0]), m.ld(hp[1]), m.ld(hp[2])
    x = [m.ld(w) for w in words]
    prev_x, prev_y = s0, s1
    out = []
    for xk in x:
        yk = m.add(m.sub(xk, prev_x), m.mul(prev_y, c))
        out.append(m.st(yk))
        prev_x, prev_y = xk, yk
    return out


def stage_shape_sine_4(lanes: list[m.Lane], high_pass: bool = True):
    """Stage `0x120DAB`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    incs, info = [], []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        inc = (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        incs.append(inc)
        info.append(setup(ln.pitch, inc[0], ln.spread))
    o1, o2 = [], []
    for ln, c, s_ in zip(lanes, incs, info, strict=True):
        s5 = m.ld(ln.spread)
        o1.append(osc(ln.phase1, c[0], s5, s_[1], s_[2], s_[3]))
        o2.append(osc(ln.phase2, c[1], s5, s_[1], s_[2], s_[3]))
    scales = [m.mul(s_[0], m.ld(ln.out_gain)) for s_, ln in zip(info, lanes, strict=True)]
    mixed = m.mix_lanes(lanes, o1, o2, scales)
    if not high_pass:
        return mixed, [o[1] for o in o1], [o[1] for o in o2]
    out = [output_high_pass(w, ln.hp or HP_IDLE) for w, ln in zip(mixed, lanes, strict=True)]
    return out, [o[1] for o in o1], [o[1] for o in o2]

#!/usr/bin/env python3
"""Bit-exact models of the ANALOG MISC stages `0x122030`, `0x122151` and `0x122417` (ORG core 2).

Listings `0x244060..0x2442A0` and `0x2442A2..0x244370` (`docs/research/synth-stage-models-8.md`).
Both draw their noise from the LCG of `synth_stage_noise` (state `DM(0xB2EF9 + lane)`, the same
constants in both lanes) and write four samples per lane to `DM(-13 + 2 k + lane, I2)`.

`0x122030`, per step `k` (the frame counter `DM(13, I2)` is incremented and stored first):

    s <- LCG(s);  v = FLOAT(s, -31)
    c = counter; m = 0x3FF
    for j in 0..9:  if c & m == 0: row[j] = trunc(v), c = m;   m >>= 1      (Voss-McCartney rows)
    a = sum of the 10 rows in order (each add rounded);  s <- LCG(s);  a += FLOAT(s, -31)
    sample_k = trunc(0.2 a)

then the output high pass of `0x120DAB` over the four samples and the common two-lane mix.
`0x122151`: per step `s <- LCG(s); v = FLOAT(s, -31); z <- z + 0.0005 (v - z)` (state `DM(-3, I2)`,
stored), `sample_k = trunc(64 z)`; no high pass; the common mix.

`0x122417` (ANALOG MISC wave 2, listing `0x24482E..0x244AB8`) is a sine whose amplitude is a cubed,
phase-modulated sine. With `a = inc 2^-31`, `inc = FIX(f P[-32], 31)` (stored `DM(-25, I2)`),
`g1 = fade(a)` (stored), `v = T(s5)` the table `DM(0xB074C..)` interpolated at `FIX(s5, 30)`
([synth-stage-models-5](synth-stage-models-5.md) s3), `u = a v / 2`, `g2 = fade(u)` (stored),
`inc2 = FIX(clip(u, 0.5), 31)`:

    s_k = 0.5 sine(phase2 + k inc2) + 0.5;   w_k = s_k^3 g2 + 0.25 - 0.25 g2     (stored)
    out_k = sine(phase1 + k inc) w_k (1.5 g1)

then the output high pass and the common mix. `sine` is the helper `0x12522E`.
"""

from __future__ import annotations

import synth_stage_noise as noise
import synth_stage_shape_sine as ss
import synth_stage_shape_sine2 as hp
import synth_stage_sync as sync
import synth_wave_model as m

# VERIFIED (listing `0x2441CE`, `0x2444EC`: `R6 = 0x3e4ccccd` / `R2 = 0x3a03126f`, `0x42800000`,
# oracle 320 of 320): the stage constants.
PINK_SCALE = 0x3E4CCCCD  # 0.2
LP_COEF = 0x3A03126F  # 0.0005
LP_GAIN = 0x42800000  # 64.0
ROWS = 10
ROW_BASE_OFFSET = -32  # rows at `DM(-21, I3) - 32 + 2 j + lane`


def lcg(state: int) -> tuple[int, m.X]:
    """One generator step: `(new state word, FLOAT(state, -31))`."""
    s = m.u32(m.s32(state) * noise.MULT + noise.INC)
    return s, m.flt(s, -31)


def pink_lane(ln: m.Lane):
    """Four samples of `0x122030` for one lane: `(stored words, state, counter, rows)`."""
    s, counter, rows = ln.state, ln.counter, list(ln.rows or (0,) * ROWS)
    out = []
    for _ in range(4):
        counter = m.u32(counter + 1)
        s, v = lcg(s)
        c, mask = counter, 0x3FF
        for j in range(ROWS):
            if c & mask == 0:
                rows[j] = m.st(v)
                c = mask
            mask >>= 1
        acc = m.X(0)
        for w in rows:
            acc = m.add(acc, m.ld(w))
        s, v2 = lcg(s)
        acc = m.add(acc, v2)
        out.append(m.st(m.mul(acc, m.ld(PINK_SCALE))))
    return out, s, counter, rows


def lowpass_lane(ln: m.Lane):
    """Four samples of `0x122151` for one lane: `(stored words, state, filter word)`."""
    s, z = ln.state, m.ld(ln.lp)
    out = []
    zw = ln.lp
    for _ in range(4):
        s, v = lcg(s)
        znew = m.add(m.mul(m.sub(v, z), m.ld(LP_COEF)), z)
        zw = m.st(znew)
        out.append(m.st(m.mul(znew, m.ld(LP_GAIN))))
        z = m.ld(zw)
    return out, s, zw


def common_mix(lanes: list[m.Lane], samples: list[list[int]]) -> list[list[int]]:
    """The mix of the noise stage: `(partner w0 + own w1) * P[16]`, each sample stored."""
    out = []
    for i in (0, 1):
        me, other = lanes[i], lanes[1 - i]
        gain = m.ld(me.out_gain)
        row = []
        for k in range(4):
            mix = m.add(
                m.mul(m.ld(samples[1 - i][k]), m.ld(other.weights[0])),
                m.mul(m.ld(samples[i][k]), m.ld(me.weights[1])),
            )
            row.append(m.st(m.mul(mix, gain)))
        out.append(row)
    return out


def stage_misc0(lanes: list[m.Lane]):
    """Stage `0x122030`: `(out_bits[lane][4], final state[lane], 0)`."""
    res = [pink_lane(ln) for ln in lanes]
    filt = [
        hp.output_high_pass(r[0], ln.hp or hp.HP_IDLE) for r, ln in zip(res, lanes, strict=True)
    ]
    return common_mix(lanes, filt), [r[1] for r in res], [0, 0]


def stage_misc1(lanes: list[m.Lane]):
    """Stage `0x122151`: `(out_bits[lane][4], final state[lane], 0)`."""
    res = [lowpass_lane(ln) for ln in lanes]
    return common_mix(lanes, [r[0] for r in res]), [r[1] for r in res], [0, 0]


# VERIFIED (listing `0x244848..0x2448FC`, `0x2449DA`): constants of `0x122417`.
HALF_GAIN = 0x3FC00000  # 1.5
QUARTER = 0x3E800000  # 0.25


def fade(x: m.X) -> m.X:
    """`C (B + clip(A - x, B))` with the saw-family constants."""
    return m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, x), m.B_FADE), m.B_FADE))


def mod_inc(ln: m.Lane, words: list[int]) -> int:
    """The modulator increment `inc2 = FIX(clip(a v / 2, 0.5), 31)` of `0x122417`."""
    inc = m.fix(m.mul(m.ld(ln.pitch), m.ld(ln.ratio1)), 31)
    u = m.mul(m.flt(inc, -31), m.mul(sync.lookup(ln.spread, words), m.HALF))
    return m.fix(m.clip(u, m.HALF), 31)


def fm_lane(ln: m.Lane, words: list[int]):
    """Four samples of `0x122417` for one lane before the high pass: `(words, phase1, phase2)`."""
    inc = m.fix(m.mul(m.ld(ln.pitch), m.ld(ln.ratio1)), 31)
    a = m.flt(inc, -31)
    g1 = m.ld(m.st(fade(a)))
    half = m.HALF
    f5 = m.mul(sync.lookup(ln.spread, words), half)
    u = m.mul(a, f5)
    g2 = m.ld(m.st(fade(u)))
    inc2 = m.fix(m.clip(u, half), 31)
    ph2 = m.u32(ln.phase2 + 4 * inc2)
    s2 = [ss.sine_of_word(m.u32(ln.phase2 + k * inc2)) for k in (1, 2, 3, 4)]
    q = m.ld(QUARTER)
    c5 = m.sub(q, m.mul(g2, q))
    w = []
    for v in s2:
        sk = m.add(m.mul(v, half), half)
        cube = m.mul(sk, m.mul(sk, sk))
        w.append(m.ld(m.st(m.add(m.mul(cube, g2), c5))))
    ph1 = m.u32(ln.phase1 + 4 * inc)
    s1 = [ss.sine_of_word(m.u32(ln.phase1 + k * inc)) for k in (1, 2, 3, 4)]
    scale = m.mul(m.ld(HALF_GAIN), g1)
    out = [m.st(m.mul(m.mul(v, wk), scale)) for v, wk in zip(s1, w, strict=True)]
    return out, ph1, ph2


def stage_misc2(lanes: list[m.Lane]):
    """Stage `0x122417`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    words = sync.table()
    res = [fm_lane(ln, words) for ln in lanes]
    filt = [
        hp.output_high_pass(r[0], ln.hp or hp.HP_IDLE) for r, ln in zip(res, lanes, strict=True)
    ]
    return common_mix(lanes, filt), [r[1] for r in res], [r[2] for r in res]


# VERIFIED (listing `0x242FB6..0x24308A`, oracle): the SUB OSC constants.
SUB_MIX = 0x3E99999A  # 0.3: the level falls as `1 - 0.3 s5`
SUB_OFFSET = 0x80000000  # pulse width word of the sub oscillator (a square)

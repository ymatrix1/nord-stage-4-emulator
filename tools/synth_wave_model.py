#!/usr/bin/env python3
"""Float32 model of the Core2 oscillator stage `0x1212D3` (ANALOG SHAPE, wave 0: Smooth Lead).

The stage (listing `0x1212D3..0x1214D6` of ORG core 2 / PNO core 2) runs as a stage-A routine in
SIMD mode: lane x and lane y execute the same code on their own words (`DM(w, I2)` of lane y is
`DM(w + 1, I2)`), and the final mix swaps registers between the lanes (`Rn = Sn`). Per lane it

1. turns the frame pitch `f = DM(-29, I2)` and the two ratios `P[-32]`, `P[-30]` of the parameter
   block `P = DM(-32, I2)` into Q31 phase increments `inc = FIX(f r 2^31)`, `a = inc 2^-31`;
2. fades the level at high pitch, `g = C (B + clip(A - a, B))` (constants of the saw helper), and
   forms the pulse width `W = clip(s17 + s19, 1) (1 - min(1, 1.5 a))`;
3. calls the helper `0x1251D4` twice per oscillator (phases, and phases shifted by `2 (W - 1)`
   in 2^32 units): the helper returns four samples of a band-limited parabola
   `y = xo^2 - (4/3) a' w^3` with `xo` the phase shifted by half a cycle, `w = 1 - min(1, |x| r)`;
   the oscillator output is `first - second` (a pulse-width shaped difference of parabolas);
4. mixes the two oscillators of both lanes with the weights of `P`, scales by
   `P[16] g / (1 - W^2)` and writes four samples to `DM(-13 .. -7, I2)` per lane.

Registers are 40-bit floats (32-bit significand, `RND32 = 0`, round to nearest even, denormals
flushed); memory words are 32-bit and a store truncates the 8 extension bits. `RECIPS` uses the seed
ROM of the simulator (`nord-sharc` `float_seeds.rs`, VERIFIED there). Compared with the
oracle by `tools/synth_wave_oracle.py` (`docs/research/synth-wave-models.md`).
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from fractions import Fraction

X = Fraction  # an exact 40-bit register value

# VERIFIED (`nord-sharc` `float_seeds.rs`, simulator measurement, max err 0): RECIPS seed fraction
# (8 bits) by the 7 mantissa MSBs.
RECIPS_SEED = (
    0xFF,
    0xFC,
    0xF8,
    0xF4,
    0xF0,
    0xEC,
    0xE8,
    0xE4,
    0xE0,
    0xDE,
    0xDA,
    0xD6,
    0xD4,
    0xD0,
    0xCC,
    0xCA,
    0xC6,
    0xC2,
    0xC0,
    0xBC,
    0xBA,
    0xB6,
    0xB4,
    0xB2,
    0xAE,
    0xAC,
    0xA8,
    0xA6,
    0xA4,
    0xA0,
    0x9E,
    0x9C,
    0x98,
    0x96,
    0x94,
    0x92,
    0x8E,
    0x8C,
    0x8A,
    0x88,
    0x86,
    0x82,
    0x80,
    0x7E,
    0x7C,
    0x7A,
    0x78,
    0x76,
    0x74,
    0x72,
    0x70,
    0x6E,
    0x6C,
    0x6A,
    0x68,
    0x66,
    0x64,
    0x62,
    0x60,
    0x5E,
    0x5C,
    0x5A,
    0x58,
    0x56,
    0x54,
    0x52,
    0x50,
    0x50,
    0x4E,
    0x4C,
    0x4A,
    0x48,
    0x46,
    0x46,
    0x44,
    0x42,
    0x40,
    0x3E,
    0x3E,
    0x3C,
    0x3A,
    0x38,
    0x38,
    0x36,
    0x34,
    0x32,
    0x32,
    0x30,
    0x2E,
    0x2E,
    0x2C,
    0x2A,
    0x28,
    0x28,
    0x26,
    0x24,
    0x24,
    0x22,
    0x20,
    0x20,
    0x1E,
    0x1E,
    0x1C,
    0x1A,
    0x1A,
    0x18,
    0x18,
    0x16,
    0x14,
    0x14,
    0x12,
    0x12,
    0x10,
    0x0E,
    0x0E,
    0x0C,
    0x0C,
    0x0A,
    0x0A,
    0x08,
    0x08,
    0x06,
    0x06,
    0x04,
    0x04,
    0x02,
    0x02,
    0x00,
)

MIN_NORMAL = Fraction(1, 2**126)


def ld(bits: int) -> X:
    """Load a 32-bit memory word as a float (exact)."""
    bits &= 0xFFFFFFFF
    exp, frac = (bits >> 23) & 0xFF, bits & 0x7FFFFF
    if exp == 0:
        return X(0)
    v = X((1 << 23) | frac, 1 << 23) * (X(2) ** (exp - 127))
    return -v if bits >> 31 else v


def st(x: X) -> int:
    """Store a register float to memory: the top 32 bits of the 40-bit word (truncation)."""
    if x == 0:
        return 0
    sign = 1 if x < 0 else 0
    a = abs(x)
    e = a.numerator.bit_length() - a.denominator.bit_length()
    if a < X(2) ** e:
        e -= 1
    m = a / X(2) ** e  # 1 <= m < 2
    frac = int((m - 1) * (1 << 23))  # truncate toward zero
    return (sign << 31) | ((e + 127) << 23) | frac


def rnd(x: X) -> X:
    """Round an exact value to a 32-bit significand, nearest even; tiny values flush to zero."""
    if x == 0:
        return X(0)
    a = abs(x)
    e = a.numerator.bit_length() - a.denominator.bit_length()
    if a < X(2) ** e:
        e -= 1
    q = a / X(2) ** (e - 31)  # in [2^31, 2^32)
    n = round(q)  # Fraction.__round__ rounds half to even
    r = X(n) * X(2) ** (e - 31)
    if r < MIN_NORMAL:
        return X(0)
    return -r if x < 0 else r


def u32(x: int) -> int:
    return x & 0xFFFFFFFF


def s32(x: int) -> int:
    x &= 0xFFFFFFFF
    return x - (1 << 32) if x & 0x80000000 else x


def f2u(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def mul(a: X, b: X) -> X:
    return rnd(a * b)


def add(a: X, b: X) -> X:
    return rnd(a + b)


def sub(a: X, b: X) -> X:
    return rnd(a - b)


def flt(i: int, shift: int) -> X:
    """`FLOAT Rx BY Ry`: int32 times 2^shift (exact in a 32-bit significand)."""
    return X(s32(i)) * X(2) ** shift


def fix(x: X, shift: int) -> int:
    """`FIX Fx BY Ry`: x * 2^shift to int32, nearest even, saturating."""
    r = round(x * X(2) ** shift)
    return u32(max(-(1 << 31), min((1 << 31) - 1, r)))


def clip(x: X, limit: X) -> X:
    """`CLIP Fx BY Fy`: x limited to +-|y|."""
    m = abs(limit)
    return max(-m, min(m, x))


def recips(x: X) -> X:
    """`RECIPS`: seed of 1/x: exponent -e-1, fraction from the 7 mantissa MSBs of x."""
    a = abs(x)
    e = a.numerator.bit_length() - a.denominator.bit_length()
    if a < X(2) ** e:
        e -= 1
    m32 = int(a / X(2) ** (e - 31))  # 32-bit significand
    seed = RECIPS_SEED[(m32 >> 24) & 0x7F]
    v = (1 + X(seed, 256)) * X(2) ** (-e - 1)
    return -v if x < 0 else v


def recip_refined(x: X, numerator: X) -> X:
    """Two Newton steps of the stage (listing `0x242660..0x24268E`): `numerator / x`."""
    two = X(2)
    s = recips(x)
    n1 = mul(s, numerator)
    sx = mul(s, x)
    e1 = sub(two, sx)
    n2 = mul(e1, n1)
    e2 = sub(two, mul(e1, sx))
    return mul(e2, n2)


# VERIFIED (image words of the helper prologue and the stage, listing 0x1251D4 / 0x1212D3).
FOUR_THIRDS = ld(0x3FAAAA8F)
A_FADE = ld(0x3EB0F34D)
B_FADE = ld(0x3DEBEF11)
C_FADE = ld(0x408AE2EA)
ONE, HALF, ONE_AND_HALF = X(1), X(1, 2), X(3, 2)


@dataclass
class Lane:
    """Raw memory words of one lane (the lane y word sits one address above the lane x word)."""

    pitch: int  # DM(-29, I2): cycles per sample
    phase1: int  # DM(-21, I2)
    phase2: int  # DM(-19, I2)
    shape_a: int  # DM(17, I2)
    shape_b: int  # DM(19, I2)
    ratio1: int  # P[-32]
    ratio2: int  # P[-30]
    weights: tuple  # (P[-14], P[-16], P[-12], P[-10]) = wA1, wB1, wA2, wB2
    out_gain: int  # P[16]
    pulse: int = 0  # DM(-8, I1): pulse width offset word (stage `0x121B99`), doubled as an integer
    state: int = 0  # noise generator state word (`DM(0xB2EF9 + lane)`, stage `0x121FD8`)
    ratios: tuple = ()  # unison stages: P[-32 + 2 j] for j = 0..7 (lane word)
    phases: tuple = ()  # unison stages: voice phase words (`DM(0xC2336 + 2 j)` + lane)
    groups: tuple = ()  # unison stages: weight groups P[-16 + 4 j .. -13 + 4 j] (shared)
    spread: int = 0  # unison stages: DM(-5, I2)
    octaves: tuple = ()  # MULTI: DM(-32 .. -30, I1) voice multipliers (shared by both lanes)


@dataclass
class Setup:
    inc1: int
    inc2: int
    a: X
    g: X
    width: X
    inv_a: X  # 0.5 / a, as stored in memory
    inv_d: X  # 1 / (1 - W^2), as stored in memory
    offset: int


def lane_setup(lane: Lane) -> Setup:
    """Increments, fade, width and the two reciprocals (listing `0x2425A6..0x242690`)."""
    pitch = ld(lane.pitch)
    inc1 = fix(mul(pitch, ld(lane.ratio1)), 31)
    inc2 = fix(mul(pitch, ld(lane.ratio2)), 31)
    a = ld(st(flt(inc1, -31)))
    g = ld(st(mul(C_FADE, add(clip(sub(A_FADE, flt(inc1, -31)), B_FADE), B_FADE))))
    shape = clip(add(ld(lane.shape_a), ld(lane.shape_b)), ONE)
    fade = sub(ONE, clip(ONE, mul(a, ONE_AND_HALF)))  # `a` as stored (R2 = DM(-6, I4))
    width_reg = mul(shape, fade)
    width = ld(st(width_reg))
    d = sub(ONE, mul(width_reg, width_reg))
    inv_a = ld(st(recip_refined(flt(inc1, -31), HALF)))
    inv_d = ld(st(recip_refined(d, ONE)))
    offset = u32(s32(fix(sub(width, ONE), 30)) * 2)
    return Setup(inc1, inc2, a, g, width, inv_a, inv_d, offset)


def helper(phases: list[int], r6: X, r7: X):
    """Helper `0x1251D4`: four band-limited parabola samples; returns `(samples, R6 after)`."""
    r6 = mul(r6, FOUR_THIRDS)
    out = []
    for x in phases:
        xf = flt(x, -31)
        xo = flt(u32(x) ^ 0x80000000, -31)
        wfun = sub(ONE, clip(ONE, mul(xf, r7)))
        w3 = mul(mul(wfun, wfun), wfun)
        out.append(sub(mul(xo, xo), mul(w3, r6)))
    return out, r6


def oscillator(phase: int, inc: int, st_: Setup):
    """One oscillator: `(four stored samples, new phase)` = first call minus the shifted second."""
    ph = [u32(phase + k * inc) for k in (1, 2, 3, 4)]
    first, r6 = helper(ph, st_.a, st_.inv_a)
    first = [ld(st(v)) for v in first]
    second, _ = helper([u32(p - st_.offset) for p in ph], r6, st_.inv_a)
    return [sub(x, y) for x, y in zip(first, second, strict=True)], ph[3]


def mix_lanes(lanes: list[Lane], o1, o2, scales: list[X]):
    """Mixes of the two lanes (listing `0x242742..0x2428BA`): returns the stored output words."""
    out = []
    for i in (0, 1):
        me, other = lanes[i], lanes[1 - i]
        row = []
        for k in range(4):
            m1 = add(
                mul(o1[1 - i][0][k], ld(other.weights[0])), mul(o1[i][0][k], ld(me.weights[1]))
            )
            m1 = ld(st(m1))  # stored to the record, read back by mix 2
            m2 = add(
                add(mul(o2[i][0][k], ld(me.weights[2])), m1),
                mul(o2[1 - i][0][k], ld(other.weights[3])),
            )
            row.append(ld(st(m2)))
        out.append([st(mul(v, scales[i])) for v in row])
    return out


def stage(lanes: list[Lane]):
    """Stage `0x1212D3`, both lanes: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    sts = [lane_setup(ln) for ln in lanes]
    o1 = [oscillator(ln.phase1, s_.inc1, s_) for ln, s_ in zip(lanes, sts, strict=True)]
    o2 = [oscillator(ln.phase2, s_.inc2, s_) for ln, s_ in zip(lanes, sts, strict=True)]
    scales = [mul(mul(ld(ln.out_gain), s_.g), s_.inv_d) for ln, s_ in zip(lanes, sts, strict=True)]
    return mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


def saw_helper(phase: int, inc: int, amp: X = ONE, offset: int = 0):
    """Helper `0x12501F` (listing `0x24A03E..0x24A160`): four band-limited falling saw samples."""
    a = flt(inc, -31)
    g = mul(C_FADE, add(clip(sub(A_FADE, a), B_FADE), B_FADE))
    neg = -mul(amp, g)
    r7 = recip_refined(a, HALF)
    ph = [u32(phase + k * inc) for k in (1, 2, 3, 4)]
    out = []
    for x in (u32(p + offset) for p in ph):
        xf = flt(x, -31)
        xo = flt(x ^ 0x80000000, -31)
        w = sub(ONE, clip(ONE, mul(xf, r7)))
        w2 = mul(w, w)
        out.append(mul(sub(xo, -w2 if xo < 0 else w2), neg))
    return out, ph[3]


def stage_saw(lanes: list[Lane]):
    """Stage `0x1215FB` (ANALOG PURE saw): two saw helper calls per lane, scale `P[16]`."""
    incs = []
    for ln in lanes:
        pitch = ld(ln.pitch)
        incs.append((fix(mul(pitch, ld(ln.ratio1)), 31), fix(mul(pitch, ld(ln.ratio2)), 31)))
    o1 = [saw_helper(ln.phase1, inc[0]) for ln, inc in zip(lanes, incs, strict=True)]
    o2 = [saw_helper(ln.phase2, inc[1]) for ln, inc in zip(lanes, incs, strict=True)]
    scales = [ld(ln.out_gain) for ln in lanes]
    return mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


def pulse_helper(phase: int, inc: int, offset: int, m0: int = 0, amp: X = ONE):
    """Helper `0x1250C9` (listing `0x24A192..0x24A3A6`): four samples of
    `M0 - g saw(ph) + g saw(ph - offset)`; returns the stored (truncated) words and the phase."""
    a = flt(inc, -31)
    g = mul(amp, mul(C_FADE, add(clip(sub(A_FADE, a), B_FADE), B_FADE)))
    r7 = recip_refined(a, HALF)
    ph = [u32(phase + k * inc) for k in (1, 2, 3, 4)]

    def saw(x: int):
        xf = flt(x, -31)
        xo = flt(x ^ 0x80000000, -31)
        w = sub(ONE, clip(ONE, mul(xf, r7)))
        w2 = mul(w, w)
        return sub(xo, -w2 if xo < 0 else w2)

    first = [ld(st(sub(ld(m0), mul(saw(x), g)))) for x in ph]
    second = [add(f, mul(saw(u32(x - offset)), g)) for f, x in zip(first, ph, strict=True)]
    return [ld(st(v)) for v in second], ph[3]


def stage_pulse(lanes: list[Lane]):
    """Stage `0x121B99` (ANALOG PURE square / pulse): pulse helper twice per lane, same mix."""
    incs, offs = [], []
    for ln in lanes:
        pitch = ld(ln.pitch)
        incs.append((fix(mul(pitch, ld(ln.ratio1)), 31), fix(mul(pitch, ld(ln.ratio2)), 31)))
        offs.append(u32(ln.pulse + ln.pulse))
    o1 = [pulse_helper(ln.phase1, c[0], o) for ln, c, o in zip(lanes, incs, offs, strict=True)]
    o2 = [pulse_helper(ln.phase2, c[1], o) for ln, c, o in zip(lanes, incs, offs, strict=True)]
    scales = [ld(ln.out_gain) for ln in lanes]
    return mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


# VERIFIED (image words `DM(I0 + 4 .. 18)` of the pass, dump of the frame constants): the polynomial
# of the sine helper `0x12522E`, and the fade constants of stage `0x120CD0`.
SINE_C12 = ld(0x3FC90FAC)
SINE_C14 = ld(0xBF25591E)
SINE_C16 = ld(0x3DA2ACAA)
SINE_C18 = ld(0xBB8DEBE6)
SINE_FADE_A = ld(0x3F30F34B)
SINE_FADE_B = ld(0x3E6BEF0F)
SINE_FADE_C = ld(0x400AE2EB)
SINE_OFFSET = 0xC0000000


def sine_helper(phase: int, inc: int):
    """Helper `0x12522E` (listing `0x24A45C..0x24A4E8`): four samples of `t (c12 + q (c14 + q (c16 +
    q c18)))` with `t = |x| - 1`, `q = t^2`, `x = (phase - 0xC0000000) 2^-30`."""
    ph = [u32(phase + k * inc) for k in (1, 2, 3, 4)]
    out = []
    for x in ph:
        t = sub(abs(flt(u32(x - SINE_OFFSET), -30)), ONE)
        q = mul(t, t)
        h = add(mul(SINE_C18, q), SINE_C16)
        h = add(mul(h, q), SINE_C14)
        h = add(mul(h, q), SINE_C12)
        out.append(mul(h, t))
    return out, ph[3]


def stage_sine(lanes: list[Lane]):
    """Stage `0x120CD0` (ANALOG PURE sine): fade from the pitch word, sine helper twice per lane."""
    incs, gs = [], []
    for ln in lanes:
        pitch = ld(ln.pitch)
        incs.append((fix(mul(pitch, ld(ln.ratio1)), 31), fix(mul(pitch, ld(ln.ratio2)), 31)))
        g = mul(SINE_FADE_C, add(clip(sub(SINE_FADE_A, pitch), SINE_FADE_B), SINE_FADE_B))
        gs.append(ld(st(g)))
    o1 = [sine_helper(ln.phase1, c[0]) for ln, c in zip(lanes, incs, strict=True)]
    o2 = [sine_helper(ln.phase2, c[1]) for ln, c in zip(lanes, incs, strict=True)]
    scales = [mul(g, ld(ln.out_gain)) for g, ln in zip(gs, lanes, strict=True)]
    return mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


def stage_triangle(lanes: list[Lane]):
    """Stage `0x121183` (ANALOG PURE triangle): the parabola helper twice per oscillator with a
    fixed half-cycle shift; fade and window width come from the pitch word, not the increment."""
    sts = []
    for ln in lanes:
        f = ld(ln.pitch)
        g = ld(st(mul(C_FADE, add(clip(sub(A_FADE, f), B_FADE), B_FADE))))
        sts.append((f, g, ld(st(recip_refined(f, HALF)))))
    incs = []
    for ln in lanes:
        pitch = ld(ln.pitch)
        incs.append((fix(mul(pitch, ld(ln.ratio1)), 31), fix(mul(pitch, ld(ln.ratio2)), 31)))

    def osc(phase: int, inc: int, f: X, inv_a: X):
        ph = [u32(phase + k * inc) for k in (1, 2, 3, 4)]
        first, r6 = helper(ph, f, inv_a)
        first = [ld(st(v)) for v in first]
        second, _ = helper([u32(p - 0x80000000) for p in ph], r6, inv_a)
        return [sub(x, y) for x, y in zip(first, second, strict=True)], ph[3]

    o1 = [osc(ln.phase1, c[0], s_[0], s_[2]) for ln, c, s_ in zip(lanes, incs, sts, strict=True)]
    o2 = [osc(ln.phase2, c[1], s_[0], s_[2]) for ln, c, s_ in zip(lanes, incs, sts, strict=True)]
    scales = [mul(ld(ln.out_gain), s_[1]) for ln, s_ in zip(lanes, sts, strict=True)]
    return mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]


def stage_shape_saw(lanes: list[Lane]):
    """Stage `0x12169D` (ANALOG SHAPE wave 1, Shape Saw): two half-amplitude saw helper calls per
    oscillator, the second shifted by `2 FIX(-w 2^30)` with `w = DM(-5, I2)` (`shape_a` here)."""
    half = HALF
    incs, offs = [], []
    for ln in lanes:
        pitch = ld(ln.pitch)
        incs.append((fix(mul(pitch, ld(ln.ratio1)), 31), fix(mul(pitch, ld(ln.ratio2)), 31)))
        offs.append(u32(fix(-ld(ln.shape_a), 30) * 2))

    def osc(phase: int, inc: int, off: int):
        first, ph = saw_helper(phase, inc, half)
        first = [ld(st(v)) for v in first]
        second, _ = saw_helper(phase, inc, half, off)
        return [add(y, x) for x, y in zip(first, second, strict=True)], ph

    o1 = [osc(ln.phase1, c[0], o) for ln, c, o in zip(lanes, incs, offs, strict=True)]
    o2 = [osc(ln.phase2, c[1], o) for ln, c, o in zip(lanes, incs, offs, strict=True)]
    scales = [ld(ln.out_gain) for ln in lanes]
    return mix_lanes(lanes, o1, o2, scales), [o[1] for o in o1], [o[1] for o in o2]

#!/usr/bin/env python3
"""Closed-form (float64) algorithm of the FX delay routine `0x1206B0` (see `fx_closed_forms.py`).

Equations of `docs/research/fx-routine-models.md` s2, extracted from the symbolic run
(`tools/fx_symbolic.py delay`) of the oracle-verified model. `delay(mem)` returns
`{address: value}` for every word the routine writes (integer words as `int`).
"""

from __future__ import annotations

from fractions import Fraction

import fx_model_delay as dm
from fx_closed_forms import K, clip, f32, lerp, smooth
from fx_pe import M32, fix_word, fpack, funpack, signed32

#: `0x406429FB`, the gain of the waveshaper table output (3.5649)
SHAPER_GAIN = f32(0x406429FB)
#: `0x3E99999A`, the scale of the waveshaper input (0.3)
SHAPER_SCALE = f32(0x3E99999A)
SCRATCH = dm.SCRATCH_1  # 0xC02C8: weight and 1 - weight of the frame 0 and frame 1 taps
FLOOR = f32(0x3C23D70A)  # 0.01, lower clamp of the level estimates


def packed_lanes(word: int) -> tuple[float, float]:
    """The two 16-bit short floats of a delay memory word (low = left, high = right)."""
    return f32(funpack(word & 0xFFFF)), f32(funpack((word >> 16) & 0xFFFF))


def tap(mem: dict[int, int], i_w: int, i_s: int, shift: int) -> tuple[float, float, float]:
    """Linear interpolation of two adjacent packed stereo samples at `i_s` by the 16-bit weight
    word at `i_w` (`w = ((word << shift) & 0xFFFF) 2^-15`)."""
    sh = signed32(shift)
    word = (mem[i_w] << sh) & M32 if sh >= 0 else mem[i_w] >> -sh
    w = (word & 0xFFFF) * 2.0**-15
    (l0, r0), (l1, r1) = packed_lanes(mem[i_s]), packed_lanes(mem[i_s + 1])
    return w * l0 + (1 - w) * l1, w * r0 + (1 - w) * r1, w


def shape(mem: dict[int, int], v: float, c, table: int, shift: int, ctrl: int, rexp: int) -> float:
    """Waveshaper of the fed-back sample: `clip(0.3 v, bound)` to a fixed-point word, signed
    10-bit table index (bits 31..22) and a fraction made of the bits selected by `ctrl`."""
    w = fix_word(Fraction(clip(SHAPER_SCALE * v, c(-0xC))), signed32(shift))
    pos, ln = ctrl & 63, (ctrl >> 6) & 63
    frac_int = signed32(((w & ((1 << ln) - 1)) << pos) & M32)
    return lerp(mem, table, signed32(w) >> 22, frac_int * 2.0 ** signed32(rexp))


def delay(mem: dict[int, int]) -> dict[int, float]:
    """`0x1206B0`: two stereo frames per call (`x = [l0, r0, l1, r1]` at `I0`, in place)."""
    ctx = dm.CTX
    c = lambda off: f32(mem[ctx + off])  # noqa: E731
    x = [f32(mem[dm.IN_ADDR + n]) for n in range(4)]
    old = [f32(mem[mem[ctx + 0xF] + n]) for n in range(4)]  # previous sends
    out: dict[int, float] = {}
    # --- level ratio and its smoothed, clamped state
    m7 = max(min(max(c(7), c(8)), 1.0), FLOOR)
    ratio = m7 / max(max(c(9), c(10)), FLOOR)
    ratio_ch = [
        max(min(ratio - ratio * c(0xE) + c(0xC + ch) * c(0xE), c(0x24)), c(0x25)) for ch in (0, 1)
    ]
    out.update({ctx + 0xC: ratio_ch[0], ctx + 0xD: ratio_ch[1]})
    # --- parameter smoothers (the left result is read back for both channels)
    g_dry = smooth(c(4), c(-0x20), K)
    g_shape = smooth(c(5), c(-0x1F), K)
    g_in = smooth(c(-0x1B), c(-0x14), K)
    mat = {n: smooth(c(t), c(s), K) for n, t, s in (
        ("ll", -0x1A, -0x12), ("rr", -0x19, -0x11), ("xl", -0x18, -0x10),
        ("xr", -0x17, -0xF), ("rl", -0x16, -0xE), ("lr", -0x15, -0xD),
    )}  # fmt: skip
    out.update({ctx - 0x20: g_dry, ctx - 0x1F: g_shape, ctx - 0x14: g_in})
    out[ctx - 0x13] = smooth(c(-0x1B), c(-0x13), K)
    out.update({ctx + o: mat[n] for n, o in (("ll", -0x12), ("rr", -0x11), ("xl", -0x10),
                ("xr", -0xF), ("rl", -0xE), ("lr", -0xD))})  # fmt: skip
    a1, a2 = smooth(c(-0xB), c(-7), K), smooth(c(-0xA), c(-6), K)
    out.update({ctx - 7: a1, ctx - 6: a2, ctx - 9: (1 + a1) / 2, ctx - 8: (1 + a2) / 2})
    # --- two cascaded all-passes over the previous sends, per channel
    fb_in = [[0.0, 0.0], [0.0, 0.0]]
    for ch in (0, 1):
        s1, s2 = c(-5 + ch), c(-3 + ch)
        for n in (0, 1):
            v = (1 + a1) / 2 * old[2 * n + ch]
            y1 = s1 + v
            s1 = y1 * a1 - v
            u = (1 + a2) / 2 * y1
            y2 = s2 + u
            s2 = u - y2 * a2
            fb_in[ch][n] = y2
        out[ctx - 5 + ch], out[ctx - 3 + ch] = s1, s2
    # --- peak holds of the fed-back signal
    for ch in (0, 1):
        out[ctx + 9 + ch] = max(c(9 + ch) * c(0xB), abs(fb_in[ch][0]), abs(fb_in[ch][1]))
    # --- delay line write: waveshaped, scaled by the level ratio, packed L | R << 16
    a_base = mem[0xC03D4] + signed32(mem[ctx + 0x10])
    for n in (0, 1):
        lanes = []
        for ch in (0, 1):
            v = fb_in[ch][n] * ratio_ch[ch]
            t = shape(mem, v, c, mem[ctx + 6], mem[ctx - 0x1E], mem[ctx - 0x1C], mem[ctx - 0x1D])
            lanes.append(fpack(f32bits(v - v * g_shape + t * SHAPER_GAIN * g_shape)))
        out[a_base + 0x14 * n] = lanes[0] | (lanes[1] << 16)
    # --- delay line read, damping filter and the outputs
    b_base = mem[0xC03D5]
    wt, rd, shift = signed32(mem[ctx + 0x12]), signed32(mem[ctx + 0x11]), mem[ctx + 0x13]
    taps = [tap(mem, b_base + wt + 0x28 * n, b_base + rd + 0x28 * n, shift) for n in (0, 1)]
    for ch in (0, 1):
        z = c(2 + ch)
        for n in (0, 1):
            t = taps[n][ch]
            y = t * c(-1) + z
            z = t * c(0) + y * c(1)
            out[dm.IN_ADDR + 2 * n + ch] = x[2 * n + ch] * g_dry + y
        out[ctx + 2 + ch] = z
        out[ctx + 0x1D + ch], out[ctx + 0x1F + ch] = taps[0][ch], taps[1][ch]
    for n, (_, _, w) in enumerate(taps):  # the weight pair is parked in two scratch words
        out[SCRATCH + 2 * n], out[SCRATCH + 2 * n + 1] = w, 1 - w
    # --- sends (written to the output buffer) and their peak holds
    prev = [c(0x1D), c(0x1E), c(0x1F), c(0x20)]
    sends = []
    for n in (0, 1):
        xl, xr = x[2 * n], x[2 * n + 1]
        pl, pr = prev[2 * n], prev[2 * n + 1]
        send_l = xl * g_in + (xr + xl) * mat["xl"] + (pl * mat["ll"] + pr * mat["rl"])
        send_r = xr * g_in + (xl + xr) * mat["xr"] + (pr * mat["rr"] + pl * mat["lr"])
        sends += [send_l, send_r]
    out.update({mem[ctx + 0xF] + n: s for n, s in enumerate(sends)})
    for ch in (0, 1):
        out[ctx + 7 + ch] = max(c(7 + ch) * c(0xB), abs(sends[ch]), abs(sends[2 + ch]))
    return out


def f32bits(v: float) -> int:
    """float64 to the nearest float32 bit pattern (inputs of `FPACK`)."""
    import struct

    return struct.unpack("<I", struct.pack("<f", v))[0]

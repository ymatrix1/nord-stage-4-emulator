#!/usr/bin/env python3
"""Bit-exact model of the ANALOG SYNC wave 2 stage `0x122A32` (ORG core 2, 40-bit registers).

Listing `0x245464..0x2458B6` (read word by word, `docs/research/synth-stage-models-5.md`), two
oscillator passes (`LCNTR = 2`, the second one on `inc2` / `phase2`). With `f` = pitch word,
`s5 = DM(-5, I2)`, `x_k = phase + k inc` (k = 1..4), per lane:

    n      = FIX(s5, 30);  i = FEXT(n, 23:8);  c = FDEP(n, 8:23) 2^-31
    h      = T[i] + (T[i + 1] - T[i]) c                 table `DM(0xB074C..)`, stored word
    g      = C (B + clip(A - f h, B))                   (stored)
    ih     = trunc(h);  frac = h - ih  (stored);  lim = ih 2^25;  m = FIX(h, 25)
    q      = (1 / h)(0.5 / f)  (stored),  r7 = 0.5 / f (stored),  off = FIX(-1 / h, 31)
    wave A = ((1 - min(1, |FLOAT(ASHIFT(CLIP(UUF(x + off, m), lim), 7) ^ 2^31)| q))^2
    wave B = FLOAT(ASHIFT(UUF(x, m), 7) ^ 2^31)  (u),  d = u - copysign(wave A, u)  (stored)
    out_k  = -frac copysign((1 - min(1, |FLOAT(x)| r7))^2, FLOAT(x)) - d_k

The mix is the common `mix_lanes` with the scale `-g P[16]` (`F0 = -F0` before the product).
`UUF` is the unsigned fractional multiply, the upper word of the 64-bit product.
"""

from __future__ import annotations

import json
from pathlib import Path

import synth_wave_model as m

TABLE_PATH = Path(__file__).resolve().parents[1] / "docs/research/data/synth-sync-table.json"
SPREAD_SHIFT = 30
UUF_LEFT_SHIFT = 0  # VERIFIED by the oracle run: the unsigned fractional product is not doubled


def table() -> list[int]:
    """The 258 table words `DM(0xB074C ..)` (derived numeric table, `synth-sync-table.json`)."""
    return json.loads(TABLE_PATH.read_text())["words"]


def uuf(a: int, b: int) -> int:
    """`Rn = Rx * Ry (UUF)`: upper 32 bits of the unsigned product."""
    return m.u32(((m.u32(a) * m.u32(b)) >> 32) << UUF_LEFT_SHIFT)


def int_clip(x: int, limit: int) -> int:
    """`Rn = CLIP Rx BY Ry` on signed words: `x` limited to `+-|limit|`."""
    lim = abs(m.s32(limit))
    return m.u32(max(-lim, min(lim, m.s32(x))))


def ashift7(x: int) -> int:
    return m.u32(x << 7)


def lookup(spread: int, words: list[int]) -> m.X:
    """`h`: linear interpolation of the table at `s5` (a Q30 index: 8 bits, 23 fraction bits)."""
    n = m.fix(m.ld(spread), SPREAD_SHIFT)
    i = (n >> 23) & 0xFF
    frac = m.flt(((n & 0x7FFFFF) << 8) & 0xFFFFFFFF, -31)
    lo, hi = m.ld(words[i]), m.ld(words[i + 1])
    return m.add(m.mul(m.sub(hi, lo), frac), lo)


def setup(pitch_word: int, spread: int, words: list[int]):
    """`(g, frac, lim, m, q, r7, off)` of one lane (stored words reloaded as floats)."""
    f = m.ld(pitch_word)
    h = m.ld(m.st(lookup(spread, words)))
    g = m.ld(m.st(m.mul(m.C_FADE, m.add(m.clip(m.sub(m.A_FADE, m.mul(f, h)), m.B_FADE), m.B_FADE))))
    ih = int(h)  # TRUNC
    frac = m.ld(m.st(m.sub(h, m.X(ih))))
    lim = m.u32(ih << 25)
    mult = m.fix(h, 25)
    inv_h = m.recip_refined(h, m.ONE)
    r7 = m.recip_refined(f, m.HALF)
    q = m.ld(m.st(m.mul(inv_h, r7)))
    off = m.fix(-inv_h, 31)
    return g, frac, lim, mult, q, m.ld(m.st(r7)), off


def parabola(word: int, r: m.X) -> m.X:
    """`(1 - min(1, |FLOAT(word)| r))^2` of a signed phase word."""
    e = m.clip(m.ONE, m.mul(m.flt(word, -31), r))
    d = m.sub(m.ONE, e)
    return m.mul(d, d)


def osc(phase: int, inc: int, st_: tuple):
    """`(four samples, new phase)` of one oscillator."""
    _, frac, lim, mult, q, r7, off = st_
    ph = [m.u32(phase + j * inc) for j in (1, 2, 3, 4)]
    out = []
    for x in ph:
        a = m.u32(ashift7(int_clip(uuf(m.u32(x + off), mult), lim)) ^ 0x80000000)
        wa = parabola(a, q)
        u = m.flt(m.u32(ashift7(uuf(x, mult)) ^ 0x80000000), -31)
        d = m.ld(m.st(m.sub(u, -wa if u < 0 else wa)))
        xf = m.flt(x, -31)
        wb = parabola(x, r7)
        t = m.mul(-wb if xf < 0 else wb, -frac)
        out.append(m.sub(t, d))
    return out, ph[3]


def stage_sync2(lanes: list[m.Lane], words: list[int] | None = None):
    """Stage `0x122A32`: `(out_bits[lane][4], phase1[lane], phase2[lane])`."""
    words = words or table()
    sts = [setup(ln.pitch, ln.spread, words) for ln in lanes]
    incs = []
    for ln in lanes:
        pitch = m.ld(ln.pitch)
        incs.append(
            (m.fix(m.mul(pitch, m.ld(ln.ratio1)), 31), m.fix(m.mul(pitch, m.ld(ln.ratio2)), 31))
        )
    o1 = [osc(ln.phase1, c[0], s_) for ln, c, s_ in zip(lanes, incs, sts, strict=True)]
    o2 = [osc(ln.phase2, c[1], s_) for ln, c, s_ in zip(lanes, incs, sts, strict=True)]
    scales = [m.mul(-s_[0], m.ld(ln.out_gain)) for s_, ln in zip(sts, lanes, strict=True)]
    mixed = m.mix_lanes(lanes, o1, o2, scales)
    return mixed, [o[1] for o in o1], [o[1] for o in o2]


def rust_table() -> str:
    """The Rust source of the table words (`synth/fw_sync_table.rs`), made from the JSON."""
    words = table()
    body = "".join(
        "    " + ", ".join(f"{w:#010x}" for w in words[i : i + 8]) + ",\n"
        for i in range(0, len(words), 8)
    )
    return (
        "//! Interpolation table of the SYNC stages (`DM(0xB074C ..)` of the ORG core 2 image).\n"
        "//!\n"
        "//! Derived numeric table: 258 raw 32-bit float words read from the oracle after the boot,\n"
        "//! made by `python tools/synth_stage_sync.py > <file>` from `docs/research/data/\n"
        "//! synth-sync-table.json` (`docs/research/synth-stage-models-5.md`). It rises roughly as\n"
        "//! `2^(i / 32)` up to entry 127, holds 16.0 at entry 128, restarts at 0.092 (entry 129) and\n"
        "//! falls to the lower bound `1e-6` (`0x358637BD`).\n\n"
        f"pub(super) const SYNC_TABLE: [u32; {len(words)}] = [\n{body}];\n\n"
        "#[cfg(test)]\nmod tests {\n    use super::*;\n\n"
        "    #[test]\n    fn table_has_the_documented_shape() {\n"
        "        assert_eq!(f32::from_bits(SYNC_TABLE[0]), 1.0);\n"
        "        assert_eq!(f32::from_bits(SYNC_TABLE[128]), 16.0);\n"
        "        assert_eq!(SYNC_TABLE[257], 0x3586_37BD);\n"
        "        assert!(SYNC_TABLE[..128]\n"
        "            .windows(2)\n"
        "            .all(|w| f32::from_bits(w[1]) > f32::from_bits(w[0])));\n"
        "    }\n}\n"
    )


if __name__ == "__main__":
    import sys

    sys.stdout.write(rust_table())

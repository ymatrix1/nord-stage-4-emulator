#!/usr/bin/env python3
"""Golden grids of the ninth batch of stages for the Rust ports (`synth/fw_misc.rs`, `fw_sub.rs`).

    python tools/synth_stage_golden8.py > crates/nord-core/src/synth/tests_fw_stage_golden8.rs

Derived values of the bit-exact models `tools/synth_stage_misc.py` (MISC 0, 1, 2) and
`tools/synth_stage_sub.py` (SUB OSC), checked against the SHARC oracle by
`tools/synth_wave_oracle.py --stage 0x122030 / 0x122151 / 0x122417 / 0x12179A`. Only derived
values appear: the two noise stages as 32-sample sequences from the default state, the FM and SUB
OSC stages as grids of one oscillator (own weight 1, gain 1).
"""

from __future__ import annotations

import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import synth_stage_golden as g
import synth_stage_misc as misc
import synth_stage_shape_sine2 as hp
import synth_stage_sub as sub
import synth_stage_sync as sync
import synth_stage_sync1 as sync1
import synth_wave_model as m

BLOCKS = 8  # four samples each
SHAPES = (0.1, 0.25, 0.5, 0.7, 0.9)
SEED = 3  # `NOISE_SEED` of `fw_stages.rs`


def blank(**kw) -> m.Lane:
    """A lane with unit own weight and gain (partner weights zero)."""
    one, zero = m.f2u(1.0), m.f2u(0.0)
    base = {
        "pitch": zero, "phase1": 0, "phase2": 0, "shape_a": zero, "shape_b": zero,
        "ratio1": one, "ratio2": one, "weights": (zero, one, zero, zero), "out_gain": one,
    }  # fmt: skip
    base.update(kw)
    return m.Lane(**base)


def pink_sequence() -> list[int]:
    """Output words of MISC 0 after the output high pass, `BLOCKS` blocks from the default state."""
    ln = blank(state=SEED, rows=(0,) * misc.ROWS, counter=0)
    state_hp = hp.HP_IDLE
    out = []
    for _ in range(BLOCKS):
        words, ln.state, ln.counter, rows = misc.pink_lane(ln)
        ln.rows = tuple(rows)
        y = hp.output_high_pass(words, state_hp)
        state_hp = (words[3], y[3], hp.HP_IDLE[2])
        out += y
    return out


def lowpass_sequence() -> list[int]:
    ln = blank(state=SEED, lp=0)
    out = []
    for _ in range(BLOCKS):
        words, ln.state, ln.lp = misc.lowpass_lane(ln)
        out += words
    return out


def fm_rows() -> list[tuple[int, ...]]:
    """`(phase, modulator phase, increment, shape, sample)` bits of the pre-high-pass FM sample."""
    words = sync.table()
    rows = []
    for dt in g.INCREMENTS:
        for shape in SHAPES:
            for s in range(g.STARTS // 4):
                ph0, ph2 = g.phase_word(s + 3), g.phase_word(s + 9)
                ln = blank(
                    pitch=m.f2u(2.0 * g.f32(dt)), phase1=ph0, phase2=ph2, spread=m.f2u(shape)
                )
                out, _, _ = misc.fm_lane(ln, words)
                inc = m.fix(m.mul(m.ld(ln.pitch), m.ld(ln.ratio1)), 31)
                inc2 = misc.mod_inc(ln, words)
                for k in range(1, 5):
                    p1 = m.u32(ph0 + k * inc) / 2**32
                    p2 = m.u32(ph2 + k * inc2) / 2**32
                    rows.append(
                        (m.f2u(p1), m.f2u(p2), m.f2u(inc / 2**32), m.f2u(shape), out[k - 1])
                    )
    return rows


def sub_rows() -> list[tuple[int, ...]]:
    """`(sub phase, increment, shape, sample)` bits of one SUB OSC oscillator."""
    rows = []
    for dt in g.INCREMENTS:
        for shape in SHAPES:
            for s in range(g.STARTS // 4):
                ph0 = g.phase_word(s + 3)
                ln = blank(pitch=m.f2u(2.0 * g.f32(dt)), phase1=ph0, phase2=0, spread=m.f2u(shape))
                out, _, _ = sub.stage_sub_osc([ln, ln])
                inc = m.fix(m.mul(m.ld(ln.pitch), m.ld(ln.ratio1)), 31)
                half = sync1.ssfr(inc, 0x40000000)
                for k in range(1, 5):
                    u = m.u32(ph0 + k * half) / 2**32
                    rows.append((m.f2u(u), m.f2u(inc / 2**32), m.f2u(shape), out[0][k - 1]))
    return rows


def rust() -> str:
    parts = [
        "//! Firmware parity of the ninth stage batch (`fw_misc.rs`, `fw_sub.rs`).\n"
        "//!\n"
        "//! Derived values only, made by `python tools/synth_stage_golden8.py > <file>` from the\n"
        "//! bit-exact models (`docs/research/synth-stage-models-8.md`).\n"
    ]
    parts.append(g.words_rust("PK_OUT", pink_sequence()))
    parts.append(g.words_rust("LP_OUT", lowpass_sequence()))
    fm = fm_rows()
    for i, name in enumerate(("PHASE", "PHASE2", "DT", "SHAPE", "OUT")):
        parts.append(g.words_rust(f"FM_{name}", [r[i] for r in fm]))
    sb = sub_rows()
    for i, name in enumerate(("PHASE", "DT", "SHAPE", "OUT")):
        parts.append(g.words_rust(f"SUB_{name}", [r[i] for r in sb]))
    tail = (
        "\n#[test]\nfn golden_tables_have_the_documented_shape() {\n"
        f"    assert_eq!(PK_OUT.len(), {4 * BLOCKS});\n"
        f"    assert_eq!(LP_OUT.len(), {4 * BLOCKS});\n"
        f"    assert_eq!(FM_OUT.len(), {len(fm)});\n"
        f"    assert_eq!(SUB_OUT.len(), {len(sb)});\n"
        "    assert!(FM_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n"
        "    assert!(SUB_OUT.iter().all(|&w| f32::from_bits(w).is_finite()));\n}\n"
    )
    return "\n".join(parts) + tail


if __name__ == "__main__":
    sys.stdout.write(rust())

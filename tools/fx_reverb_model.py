#!/usr/bin/env python3
"""Bit-exact model of the half-rate wrapper of the type 0 / 1 reverb routines.

`0x12417E` and `0x12435C` are not multi-tap lists: they wrap the reverb core `0x123F61` (called
through `I8`) in a sample-rate converter (`docs/research/fx-reverb-leslie.md` s5).

* `0x12417E` (4 stereo frames at `I0`): the three Butterworth stages at fs/4 of `0x120FDB` (PM
  `0x9437A`, states `I1 + 0x49`) low-pass the frames; frames 0 and 2 go to the core (`R0`, `R4`),
  which returns two frames (`R12`, `R14`); they are held (`w0 w0 w2 w2`, stored at `I1 + 0x41`),
  low-passed by a second cascade (PM `0x9437F`, states `I1 + 0x55`), and `out = in * c[0x15] + z`.
* `0x12435C` (2 frames): the core runs on frames 0 and 1 directly, `out = in * c[0x15] + wet`.

The core is not modelled: its two outputs are an input of the model (read from the oracle trace at
the return address). `check4` / `check2` compare every output and state word with the oracle.

    cargo build -p nord-sharc --release
    python tools/fx_reverb_model.py check4 SRAM/core2_sram.bin --calls 24
    python tools/fx_reverb_model.py check2 SRAM/core2_sram.bin
    python tools/fx_reverb_model.py golden > crates/nord-core/src/fx/tests_rvb_wrap_golden.rs
"""

from __future__ import annotations

import argparse
import random
import sys
from collections.abc import Callable
from fractions import Fraction
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
import fx_biquad_check as fb
import fx_oracle as fo
from fx_pe import parse_trace

PC4, POST4 = 0x12417E, 0x12425C  # entry; first instruction after `call (m8, i8)`
PC2, POST2 = 0x12435C, 0x124365
MIX4 = PC4 + 0x1BD  # `r0 = dm(0, i0)` of the final mix: R4 holds the gain just loaded
END = 0x1FFFFF  # never reached: the routine ends with its `RTS` to this return address
IN_ADDR, CTX = 0xB1000, 0xB0CAE  # frames at I0, context at I1 (the organ Rvb 0 instance)
GAIN, HOLD, ST1, ST2 = CTX + 0x15, CTX + 0x41, CTX + 0x49, CTX + 0x55
STAGES = 3
COEF = tuple(fb.from_bits(w) for w in fb.COEF_WORDS)  # both banks (PM 0x9437A / 0x9437F)
Pair = tuple[Fraction, Fraction]
#: pointers and lengths the core `0x123F61` needs to run (synthetic; the firmware's are unknown)
CORE_CTX = {
    0x11: 0x3F7D70A4, 0x12: 0x3F800000, 0x13: 0x3F800000, 0x16: 0x3F800000, 0x25: 0x3F000000,
    0x26: 0x3E99999A, 0x27: 0xB5000, 0x28: 0x40, 0x29: 0xB5000, 0x2B: 0x20, 0x2C: 0xB5100,
    0x2D: 0x40, 0x2E: 0xB5100, 0x30: 0x10, 0x20: 0x800, 0x32: 0xB5800, 0x34: 0xB5800,
}  # fmt: skip


TINY = Fraction(1, 2**126)


def rnd(v: Fraction) -> Fraction:
    """A register result: 32-bit mantissa, an underflow (below the smallest normal) flushes to 0."""
    return Fraction(0) if abs(v) < TINY else fb.ext40(v)


def store(v: Fraction) -> Fraction:
    return Fraction(0) if abs(v) < TINY else fb.store32(v)


def bank(frames: list[Pair], states: list[Fraction]) -> list[Pair]:
    """Three stages of the fs/4 Butterworth cascade (`states` updated in place, 12 words).

    Same arithmetic as `fx_biquad_check.reference_cascade` with the underflow flush added: the
    stage reads and writes memory words once per block; states are `s1L s1R s2L s2R` per stage."""
    out = list(frames)
    for k in range(STAGES):
        st = states[4 * k : 4 * k + 4]
        sl, sr = [st[0], st[2]], [st[1], st[3]]
        out = [
            (store(fb.biquad_step(a, COEF, sl, rnd)), store(fb.biquad_step(b, COEF, sr, rnd)))
            for a, b in out
        ]
        states[4 * k : 4 * k + 4] = [store(v) for v in (sl[0], sr[0], sl[1], sr[1])]
    return out


def mix(x: Pair, g: Fraction, w: Pair) -> Pair:
    """`out = x * g + w`: the product is a register value, the sum is stored (truncated)."""
    return tuple(store(rnd(rnd(a * g) + b)) for a, b in zip(x, w))  # type: ignore[return-value]


def wrapper4(
    frames: list[Pair],
    gain: Fraction,
    st1: list[Fraction],
    st2: list[Fraction],
    core: Callable[[Pair, Pair], tuple[Pair, Pair]],
) -> tuple[list[Pair], list[Pair]]:
    """`0x12417E`: `(out frames, z)`; `core(frame0, frame2)` returns the core's `(R12, R14)`."""
    y = bank(frames, st1)
    w0, w2 = (tuple(store(v) for v in w) for w in core(y[0], y[2]))
    z = bank([w0, w0, w2, w2], st2)  # type: ignore[list-item]
    return [mix(x, gain, zz) for x, zz in zip(frames, z)], z


def wrapper2(frames: list[Pair], gain: Fraction, wet: tuple[Pair, Pair]) -> list[Pair]:
    """`0x12435C`: two frames, the core's outputs are added after the gain."""
    return [mix(x, gain, w) for x, w in zip(frames, wet)]


def unsign0(w: int) -> int:
    """`-0` as `+0`: the sign of a flushed underflow is not modelled (values compare equal)."""
    return 0 if w & 0x7FFFFFFF == 0 else w


def q(x: float) -> Fraction:
    return fb.from_bits(fb.bits(x))


def flat(frames: list[Pair]) -> list[int]:
    return [fb.word(v) for f in frames for v in f]


def pairs(words: list[int]) -> list[Pair]:
    v = [fb.from_bits(w) for w in words]
    return [(v[i], v[i + 1]) for i in range(0, len(v), 2)]


# --- oracle comparison ---------------------------------------------------------------------------


def static_context() -> dict[int, int]:
    """Type 0 words (the loop count `c[-0x12]` among them) plus the synthetic core pointers."""
    import csv

    path = Path(__file__).resolve().parents[1] / "docs/research/data/fx-type-words.csv"
    mem = {}
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            if (r["slot"], r["type"], r["kinds"]) == ("Rvb", "0", "organ"):
                mem[CTX + int(r["ctx"])] = int(r["word"], 16)
    mem.update({CTX + o: w for o, w in CORE_CTX.items()})
    return mem


def rand_frames(rng: random.Random, n: int) -> list[int]:
    return [fb.bits(rng.uniform(-1, 1)) for _ in range(2 * n)]


def check4(image: str, binary: Path, calls: int, seed: int = 1) -> tuple[int, int, list]:
    """Words differing from the oracle over `calls` carried calls: `(bad, total, first)`."""
    rng = random.Random(seed)
    gain = fb.bits(rng.uniform(0.0, 1.0))
    s1 = [fb.bits(rng.uniform(-0.2, 0.2)) for _ in range(12)]
    s2 = [fb.bits(rng.uniform(-0.2, 0.2)) for _ in range(12)]
    ins = [rand_frames(rng, 4) for _ in range(calls)]
    mem = static_context()
    mem.update({GAIN: gain, **{ST1 + k: w for k, w in enumerate(s1)}})
    mem.update({ST2 + k: w for k, w in enumerate(s2)})
    rt = fo.Routine("rvb4", PC4, END, {"I0": IN_ADDR, "I1": CTX, "M0": 0, "M2": 2, "M9": 1},
                    lambda *_: None, lambda *_: {}, mem)  # fmt: skip
    dumps = {*range(IN_ADDR, IN_ADDR + 8), *range(HOLD, HOLD + 8), *range(ST1, ST2 + 12)}
    cases = [{IN_ADDR + k: w for k, w in enumerate(i)} for i in ins]
    text = fo.run_oracle(image, binary, rt, cases, dumps, trace=True)
    got = fb.parse_dumps(text)
    st1, st2 = [fb.from_bits(w) for w in s1], [fb.from_bits(w) for w in s2]
    bad = total = 0
    first: list = []
    for k, words in enumerate(ins):
        rows = {r["pc"]: r for r in parse_trace(text, k)}
        row = rows[POST4]
        g_k = fb.from_bits(
            rows[MIX4]["r"][4]
        )  # `c[0x15]` as loaded after the core (it rewrites it)
        wet = (pairs([row["r"][12], row["s"][12]])[0], pairs([row["r"][14], row["s"][14]])[0])
        out, z = wrapper4(pairs(words), g_k, st1, st2, lambda *_: wet)
        want = {IN_ADDR + i: w for i, w in enumerate(flat(out))}
        want |= {HOLD + i: w for i, w in enumerate(flat(z))}
        want |= {ST1 + i: fb.word(v) for i, v in enumerate(st1 + st2)}
        for a, w in want.items():
            total += 1
            if unsign0(got[k][a]) != w:
                bad += 1
                if len(first) < 4:
                    first.append((k, hex(a), hex(got[k][a]), hex(w)))
    return bad, total, first


def check2(image: str, binary: Path, runs: int, seed: int = 1) -> tuple[int, int, list]:
    """The addition after the core of `0x12435C`, `runs` register presets of 3 calls each."""
    rng = random.Random(seed)
    bad = total = 0
    first: list = []
    for _ in range(runs):
        gain = fb.bits(rng.uniform(0, 1))
        regs = [fb.bits(rng.uniform(-1, 1)) for _ in range(4)]  # R12 S12 R14 S14
        sets = {"I0": IN_ADDR, "I1": CTX, "R12": regs[0], "S12": regs[1], "R14": regs[2],
                "S14": regs[3]}  # fmt: skip
        rt = fo.Routine("rvb2", POST2, END, sets, lambda *_: None, lambda *_: {}, {GAIN: gain})
        ins = [rand_frames(rng, 2) for _ in range(3)]
        cases = [{IN_ADDR + k: w for k, w in enumerate(i)} for i in ins]
        got = fb.parse_dumps(
            fo.run_oracle(image, binary, rt, cases, set(range(IN_ADDR, IN_ADDR + 4)))
        )
        wet = (pairs(regs[:2])[0], pairs(regs[2:])[0])
        for k, words in enumerate(ins):
            want = flat(wrapper2(pairs(words), fb.from_bits(gain), wet))
            for i, w in enumerate(want):
                total += 1
                if unsign0(got[k][IN_ADDR + i]) != w:
                    bad += 1
                    first.append((k, i, hex(got[k][IN_ADDR + i]), hex(w)))
    return bad, total, first[:4]


# --- golden vectors for the Rust test ------------------------------------------------------------

GOLDEN_CALLS = 8
GOLDEN_GAIN = Fraction(1, 4)


def golden_input(n: int, ch: int) -> Fraction:
    mul, add, mod = (37, 11, 64) if ch == 0 else (53, 5, 61)
    return Fraction((mul * n + add) % mod, 64) - Fraction(1, 2)


def golden_wet(k: int, which: int, ch: int) -> Fraction:
    """Synthetic core outputs: `((k + 3 which + 5 ch) % 7) / 16 - 3/16`, exact in f32."""
    return Fraction((k + 3 * which + 5 * ch) % 7, 16) - Fraction(3, 16)


def golden_vectors() -> dict[str, list[int]]:
    """Words of `wrapper4` over `GOLDEN_CALLS` calls (zero states) and of `wrapper2`."""
    st1, st2 = [Fraction(0)] * 12, [Fraction(0)] * 12
    out4: list[int] = []
    out2: list[int] = []
    for k in range(GOLDEN_CALLS):
        frames = [(golden_input(4 * k + f, 0), golden_input(4 * k + f, 1)) for f in range(4)]

        def core(_f0: Pair, _f2: Pair, k: int = k) -> tuple[Pair, Pair]:
            return tuple((golden_wet(k, w, 0), golden_wet(k, w, 1)) for w in (0, 1))  # type: ignore[return-value]

        out, _ = wrapper4(frames, GOLDEN_GAIN, st1, st2, core)
        out4 += flat(out)
        wet = (
            (golden_wet(k, 2, 0), golden_wet(k, 2, 1)),
            (golden_wet(k, 3, 0), golden_wet(k, 3, 1)),
        )
        out2 += flat(wrapper2(frames[:2], GOLDEN_GAIN, wet))
    return {"out4": out4, "out2": out2, "states": [fb.word(v) for v in st1 + st2]}


RUST_HEAD = """//! Firmware parity of the reverb half-rate wrapper `0x12417E` / `0x12435C`.
//!
//! GENERATED by `python tools/fx_reverb_model.py golden` (the model matches the `sharc-fx` oracle
//! word for word given the core's outputs, `docs/research/fx-reverb-leslie.md` s5); derived words
//! only. The core outputs are synthetic here: `wet(k, which, ch) = ((k + 3 which + 5 ch) % 7) / 16
//! - 3 / 16`; `which` 0, 1 are the two frames of `process4`, 2, 3 those of `mix2`.

use super::biquad::{ext40_ref, FW_BUTTERWORTH_FS4_WORDS};
use super::fw_rvb_wrap::{mix2, FwRvbWrap};
use crate::sinc::ext40::round_sig;

const CALLS: usize = 8;
"""

RUST_TEST = """
type Frame = [f32; 2];
type Cascade = [[[f32; 2]; 2]; 3];

fn input(ch: usize, n: usize) -> f32 {
    let (mul, add, modulus) = if ch == 0 { (37, 11, 64) } else { (53, 5, 61) };
    ((mul * n + add) % modulus) as f32 / 64.0 - 0.5
}

fn wet(k: usize, which: usize, ch: usize) -> f32 {
    ((k + 3 * which + 5 * ch) % 7) as f32 / 16.0 - 3.0 / 16.0
}

fn wet_pair(k: usize, a: usize) -> (Frame, Frame) {
    (
        [wet(k, a, 0), wet(k, a, 1)],
        [wet(k, a + 1, 0), wet(k, a + 1, 1)],
    )
}

fn frames(k: usize) -> [Frame; 4] {
    std::array::from_fn(|f| [input(0, 4 * k + f), input(1, 4 * k + f)])
}

/// The cascade with the 40-bit register arithmetic of the DSP (`biquad::ext40_ref`).
fn ref_cascade(st: &mut Cascade, x: &mut [Frame; 4]) {
    let c = FW_BUTTERWORTH_FS4_WORDS.map(f32::from_bits);
    for stage in st.iter_mut() {
        for (ch, s) in stage.iter_mut().enumerate() {
            let mut col = [x[0][ch], x[1][ch], x[2][ch], x[3][ch]];
            ext40_ref::stage(&mut col, &c, s);
            for (f, v) in x.iter_mut().zip(col) {
                f[ch] = v;
            }
        }
    }
}

/// `x * g + w` with a register product and a truncating store.
fn ref_mix(x: Frame, g: f32, w: Frame) -> Frame {
    let reg = |v: f64| round_sig(v, 32, false);
    let st = |v: f64| round_sig(v, 24, true) as f32;
    std::array::from_fn(|i| st(reg(reg(f64::from(x[i]) * f64::from(g)) + f64::from(w[i]))))
}

fn words(v: impl IntoIterator<Item = f32>) -> Vec<u32> {
    v.into_iter().map(f32::to_bits).collect()
}

#[test]
fn ext40_reference_matches_the_firmware_golden_vectors() {
    let (mut s1, mut s2): (Cascade, Cascade) = Default::default();
    let (mut out4, mut out2) = (Vec::new(), Vec::new());
    for k in 0..CALLS {
        let dry = frames(k);
        let mut y = dry;
        ref_cascade(&mut s1, &mut y);
        let (w0, w2) = wet_pair(k, 0);
        let mut z = [w0, w0, w2, w2];
        ref_cascade(&mut s2, &mut z);
        for (d, zz) in dry.iter().zip(z) {
            out4.extend(ref_mix(*d, 0.25, zz));
        }
        let (a, b) = wet_pair(k, 2);
        out2.extend(ref_mix(dry[0], 0.25, a));
        out2.extend(ref_mix(dry[1], 0.25, b));
    }
    assert_eq!(words(out4), GOLDEN_OUT4, "0x12417E output words");
    assert_eq!(words(out2), GOLDEN_OUT2, "0x12435C output words");
    let st: Vec<f32> = [s1, s2]
        .iter()
        .flat_map(|b| b.iter().flat_map(|s| [s[0][0], s[1][0], s[0][1], s[1][1]]))
        .collect();
    assert_eq!(words(st), GOLDEN_STATES, "state words");
}

#[test]
fn f32_path_follows_the_golden_vectors() {
    let mut w = FwRvbWrap::new();
    let (mut worst4, mut worst2) = (0.0f32, 0.0f32);
    for k in 0..CALLS {
        let mut f = frames(k);
        let mut two = [f[0], f[1]];
        let (a, b) = wet_pair(k, 0);
        w.process4(&mut f, 0.25, |_, _| (a, b));
        mix2(&mut two, 0.25, wet_pair(k, 2));
        for (i, v) in f.iter().flatten().enumerate() {
            worst4 = worst4.max((v - f32::from_bits(GOLDEN_OUT4[8 * k + i])).abs());
        }
        for (i, v) in two.iter().flatten().enumerate() {
            worst2 = worst2.max((v - f32::from_bits(GOLDEN_OUT2[4 * k + i])).abs());
        }
    }
    assert!(worst4 < 2e-6 && worst2 < 1e-7, "{worst4} {worst2}");
    let st = w.state_words();
    for (a, b) in st.iter().zip(GOLDEN_STATES) {
        assert!((a - f32::from_bits(b)).abs() < 2e-6);
    }
}
"""


def golden_rust() -> str:
    lines = [RUST_HEAD]
    for name, words in golden_vectors().items():
        lines.append(f"pub(super) const GOLDEN_{name.upper()}: [u32; {len(words)}] = [")
        lines += [
            "    " + ", ".join(f"{w:#010x}" for w in words[i : i + 6]) + ","
            for i in range(0, len(words), 6)
        ]
        lines.append("];")
    return "\n".join(lines) + "\n" + RUST_TEST


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("cmd", choices=["check4", "check2", "golden"])
    p.add_argument("image", nargs="?", help="FX Core2 flat SRAM image")
    p.add_argument("--calls", type=int, default=24)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--bin")
    a = p.parse_args(argv)
    if a.cmd == "golden":
        print(golden_rust(), end="")
        return 0
    if not a.image:
        p.error("image required")
    binary = fb.find_binary(a.bin)
    fn = check4 if a.cmd == "check4" else check2
    bad, total, first = fn(a.image, binary, a.calls, a.seed)
    print(f"{a.cmd}: {total - bad}/{total} words identical", first)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())

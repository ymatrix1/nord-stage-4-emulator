//! Oracle versus engine: an SY1SY2 sample voice started through the firmware's own command
//! sequence and driven by its real audio ISR (Stage A state machine, gain/step slewing, Stage B).
//!
//! The oracle frames, and the per-frame gain and step words the firmware computed, are embedded
//! in `tests/support/sy_voice_golden.rs` (scenario `tools/oracle_scenarios/sy_voice_hash.scn`;
//! method and status of every assumption: `docs/research/sy-smp-audio.md`). This test replays the
//! position model with those words and renders with `SincKernelSet`.
//!
//! Case: 128-frame ring of the `hash` signal (37/11 left, 23/5 right), kernel 0, start position
//! word `0xFB000000` (set by the firmware's prepare state), `step` word `0x00A00000` (ratio
//! 0.625, from the pitch table entry `0x0800`), 48 frames.
#[path = "support/sy_voice_golden.rs"]
mod golden;

use golden::*;
use nord_core::sinc::ext40;
use nord_core::sinc::{SincKernelSet, TAPS};

/// Position word after the firmware's prepare state (`0x1C07CD`: `DM(rec+0x2B) = 0xFB000000`).
/// VERIFIED: read back from the oracle record after the prepare ISR.
const START_POS: u32 = 0xFB00_0000;
const FRAMES: usize = 48;
/// One float32 ulp in [0.25, 0.5): 2^-25. The bound allows two (engine kernel 1 ulp off the table).
const TOL: f32 = 1.2e-7;

fn hash_ring(mul: u64, add: u64) -> Vec<f32> {
    (0..128u64)
        .map(|n| ((n * mul + add) % 64) as f32 / 64.0 - 0.5)
        .collect()
}

fn floats(bits: &[u32]) -> Vec<f32> {
    bits.iter().map(|b| f32::from_bits(*b)).collect()
}

/// Engine render of one channel with the firmware's per-frame gain and step words.
fn render(ring: &[f32], gains: &[u32], accum_ext40: bool) -> Vec<f32> {
    let set = SincKernelSet::new();
    let kernel = set.get(0);
    let mut pos = START_POS;
    (0..FRAMES)
        .map(|n| {
            let (idx, phase) = ((pos >> 25) as usize, ((pos >> 17) & 0xFF) as usize);
            let w = kernel.weights(phase);
            let x = |t: usize| ring[(idx + t) % ring.len()];
            let gain = f32::from_bits(gains[n]);
            let v = if accum_ext40 {
                ext40::mac_ext40(w, x, gain, false, true)
            } else {
                (0..TAPS).fold(0.0_f32, |a, t| a + w[t] * x(t)) * gain
            };
            pos = pos.wrapping_add(ORACLE_STEP[n].wrapping_mul(2));
            v
        })
        .collect()
}

fn max_err(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

#[test]
fn firmware_step_word_is_the_pitch_table_ratio() {
    // 0x0800 in the 16-bit pitch table gives ratio 0.625 = 0x00A00000 / 2^24 on every frame
    assert!(ORACLE_STEP.iter().all(|s| *s == 0x00A0_0000));
}

#[test]
fn firmware_gain_slews_to_unity_within_one_frame() {
    // frame 0 is one ulp below 1.0 (the prepare state starts the smoother at 1.0), then 1.0
    assert_eq!(ORACLE_GAIN_L[0], 0x3F7F_FFFF);
    assert!(ORACLE_GAIN_L[1..].iter().all(|g| *g == 0x3F80_0000));
    assert_eq!(ORACLE_GAIN_L, ORACLE_GAIN_R);
}

#[test]
fn engine_render_with_firmware_words_matches_the_oracle_frames() {
    let (rl, rr) = (hash_ring(37, 11), hash_ring(23, 5));
    let (ol, or) = (floats(&ORACLE_L), floats(&ORACLE_R));
    for ext in [true, false] {
        let (l, r) = (
            render(&rl, &ORACLE_GAIN_L, ext),
            render(&rr, &ORACLE_GAIN_R, ext),
        );
        let (el, er) = (max_err(&l, &ol), max_err(&r, &or));
        assert!(el < TOL && er < TOL, "ext40 {ext}: errors {el:e} {er:e}");
    }
}

#[test]
fn oracle_frames_are_not_silent() {
    let energy: f32 = floats(&ORACLE_L).iter().map(|v| v * v).sum();
    assert!(energy > 1.0, "energy {energy}");
}

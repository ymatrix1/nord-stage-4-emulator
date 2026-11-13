//! Parameter fills of the firmware stages: the closed forms of [`super::fw_map`] written into the
//! context words of [`FwDelay`], [`FwCompressor`], [`FwAllpass`], [`FwAmpDrive`] and
//! [`FwToneStack2`]. Allocation free; every fill touches only the words its id owns, so the base
//! words installed by the type switch stay as the caller set them.

use super::fw_allpass::{ctx as ap, FwAllpass};
use super::fw_amp_drive::{ctx as drive, FwAmpDrive};
use super::fw_compressor::{ctx as cmp, FwCompressor};
use super::fw_delay::FwDelay;
use super::fw_map::{self as map, AllpassType};
use super::fw_tone_stack::FwToneStack2;

/// ASSUMED: slope word of the compressor context. The firmware's value is not captured
/// (`fx-contexts.md` s7); `0.05` keeps `excess * slope * 0.5` below 1 for the whole `kCmpAmt`
/// range (excess up to 37 log2 units), so the `FIX` stage never saturates. What would confirm it:
/// the Cmp base words of the type switch.
pub const CMP_SLOPE: f32 = 0.05;

/// Sets `kDlyMix` and `kDlyFbkAmt` (UI values `0..=127`) on a delay context: send `c[-0x1B]`, dry
/// `c[4]`, feedback matrix diagonal `c[-0x1A]`, `c[-0x19]`. The delay time words are not touched
/// (undecoded, `fx-contexts.md` s5.1).
pub fn set_delay(stage: &mut FwDelay, mix: f32, feedback: f32) {
    stage.set_value(-0x1B, map::delay_send(mix));
    stage.set_value(4, map::delay_dry(mix));
    let fb = map::delay_feedback(feedback);
    stage.set_value(-0x1A, fb);
    stage.set_value(-0x19, fb);
}

/// Sets `kCmpAmt` (UI value `0..=127`) on a compressor context: makeup target `c[-0x1C]`,
/// threshold target `c[-0x1B]`. The smoothed makeup state `c[-0x1A]` and the threshold word
/// `c[-0x19]` start at their targets (`settle = true`, a static setting) or glide there with the
/// stage's smoother (`settle = false`, a live knob move).
pub fn set_compressor_amount(stage: &mut FwCompressor, amount: f32, settle: bool) {
    let (makeup, threshold) = (map::cmp_makeup(amount), map::cmp_threshold(amount));
    stage.set_value(cmp::MAKEUP_TARGET, makeup);
    stage.set_value(cmp::MAKEUP_TARGET + 1, threshold);
    if settle {
        stage.set_value(cmp::MAKEUP, makeup);
        stage.set_value(cmp::MAKEUP + 1, threshold);
        stage.set_value(cmp::THRESHOLD + 1, threshold);
    }
}

/// A compressor context for `kCmpAmt = amount` (UI `0..=127`): [`FwCompressor::new`] defaults, the
/// captured attack and release pairs, [`CMP_SLOPE`] and the amount words, settled.
pub fn compressor_for_amount(amount: f32) -> FwCompressor {
    let mut c = FwCompressor::new();
    for (pair, [l, r]) in [
        (cmp::ATTACK, map::CMP_ATTACK),
        (cmp::RELEASE, map::CMP_RELEASE),
    ] {
        c.set_value(pair, l);
        c.set_value(pair + 1, r);
    }
    c.set_pair(cmp::SLOPE, CMP_SLOPE);
    set_compressor_amount(&mut c, amount, true);
    c
}

/// Sets `kFx1RateClkDiv` (`rate` `0..=127`) and `kFx1Amt` (`amount` `0..=127`) on an all-pass
/// context of Fx1 type 2 or 3: phase increment `c[-0x20]`, wet and dry words.
pub fn set_allpass(stage: &mut FwAllpass, kind: AllpassType, rate: u32, amount: f32) {
    stage.set_word(ap::RATE, map::allpass_rate(rate));
    let wet_at = match kind {
        AllpassType::Two => ap::A_TARGET,
        AllpassType::Three => ap::RIGHT_LFO,
    };
    stage.set_value(wet_at, map::allpass_wet(amount));
    stage.set_value(ap::A_TARGET + 1, map::allpass_dry(amount));
}

/// Sets `kAmpDrive` (UI `0..=127`) on the Amp drive stage: the phase target `c[-0x20]`.
pub fn set_amp_drive(stage: &mut FwAmpDrive, amount: f32) {
    stage.set_value(drive::PHASE_TARGET, map::amp_drive_target(amount));
}

/// Sets `kAmpTreble` and `kAmpBass` (signed counts `-60..=60`) on the tone stack (`0x1243F4`):
/// treble `F` `c[-0x1B]` and `E` `c[-0x1C]`, bass `A` `c[-0x20]` and smoother `c[-0x14]`
/// (piano unit tables; the mid band words are not mapped).
pub fn set_tone_stack(stage: &mut FwToneStack2, treble: f32, bass: f32) {
    stage.set_value(-0x1B, map::tone_shelf_gain(treble));
    stage.set_value(-0x1C, map::tone_shelf_pole(treble));
    stage.set_value(-0x20, map::tone_shelf_gain(bass));
    stage.set_value(-0x14, map::tone_bass_smoother(bass));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_fill_writes_the_four_words_and_leaves_the_rest() {
        let mut d = FwDelay::default();
        set_delay(&mut d, 127.0, 127.0);
        assert_eq!(d.value(-0x1B), 1.0);
        assert!(d.value(4) < 1e-3);
        assert_eq!((d.value(-0x1A), d.value(-0x19)), (1.0, 1.0));
        let nonzero = d.words().iter().filter(|w| **w != 0).count();
        assert_eq!(nonzero, 4, "send, dry (2.4e-4), feedback pair");
    }

    #[test]
    fn compressor_amount_settles_or_glides() {
        let mut c = compressor_for_amount(127.0);
        assert!((f32::from_bits(c.words()[(cmp::MAKEUP + 0x20) as usize]) - 6.72).abs() < 1e-3);
        set_compressor_amount(&mut c, 0.0, false);
        let state = f32::from_bits(c.words()[(cmp::MAKEUP + 0x20) as usize]);
        assert!((state - 6.72).abs() < 1e-3, "state keeps gliding from 6.72");
        let target = f32::from_bits(c.words()[(cmp::MAKEUP_TARGET + 0x20) as usize]);
        assert_eq!(target, 1.0);
    }

    #[test]
    fn allpass_types_use_their_own_wet_word() {
        let mut two = FwAllpass::default();
        set_allpass(&mut two, AllpassType::Two, 10, 127.0);
        let mut three = FwAllpass::default();
        set_allpass(&mut three, AllpassType::Three, 10, 127.0);
        assert_eq!(two.value(ap::A_TARGET), 0.5);
        assert_eq!(three.value(ap::A_TARGET), 0.0);
        assert_eq!(three.value(ap::RIGHT_LFO), 0.5);
        assert_eq!(two.words()[0], 12_800 + 16_384 * 10);
    }

    #[test]
    fn drive_and_tone_fills_set_the_documented_words() {
        let mut d = FwAmpDrive::default();
        set_amp_drive(&mut d, 127.0);
        assert_eq!(d.value(drive::PHASE_TARGET), 1.0);
        let mut t = FwToneStack2::default();
        set_tone_stack(&mut t, 0.0, -60.0);
        assert_eq!(t.value(-0x1B), 0.0);
        assert!(t.value(-0x20) < -0.8);
        assert!(t.value(-0x14) > 0.07);
    }
}

//! Closed-form maps from the UI value of an FX edit id to the firmware context word it makes the
//! ARM write (`docs/research/fx-contexts.md` s5, `tools/fx_forms.py`).
//!
//! Every function takes the id's **UI value** `v` (7-bit `0..=127` unless noted, tone controls are
//! signed counts `-60..=60`) and returns the word the DSP routine reads. The engine's own fields
//! (`delay_mix`, `comp_amount`, `drive_amount`, `eq_*_db`) are unit values or dB; [`unit_to_ui`] and
//! [`db_to_counts`] convert them.
//!
//! Tags: the **word values** are `VERIFIED`: each form reproduces the series of
//! `docs/research/data/fx-param-fits.csv` (float32 words captured from the ARM, 9 to 13 UI values
//! per series), checked by `tests_fw_map.rs` against `tests_fw_series.rs`. The **role** of a word
//! (which model field it is) is `EMPIRICAL`: the position matches a field of the routine model
//! (`fx-routine-models.md`). That the engine's UI scale equals the preset field scale is
//! `ASSUMED` where noted (the preset stores `comp_amount`, `delay_mix`, ... as `raw7 / 127`,
//! `cbin/fx_preset.rs`, which is exactly the firmware's `0..=127` id range).

use super::EQ_GAIN_DB_PER_COUNT;

/// Engine unit value `0..=1` (preset field `raw7 / 127`) to the 7-bit UI value of the id.
#[inline]
pub fn unit_to_ui(x: f32) -> f32 {
    x.clamp(0.0, 1.0) * 127.0
}

/// Engine EQ gain in dB to the signed UI count of `kAmpBass` / `kAmpMid` / `kAmpTreble`.
/// ASSUMED: [`EQ_GAIN_DB_PER_COUNT`] (0.25 dB, `tables.rs`) is the count to dB step.
#[inline]
pub fn db_to_counts(db: f32) -> f32 {
    (db / EQ_GAIN_DB_PER_COUNT).clamp(-60.0, 60.0)
}

/// Delay input (send) gain `c[-0x1B]` of `kDlyMix`: `min(1, (v / 64)^2)`. VERIFIED words (11
/// values, series `Dly,0,kDlyMix,0xC0279098,0,-27`), EMPIRICAL role (`g_in`).
#[inline]
pub fn delay_send(v: f32) -> f32 {
    ((v / 64.0) * (v / 64.0)).min(1.0)
}

/// Delay dry gain `c[4]` of `kDlyMix`: `min(1, (2 - v / 64)^2)`. VERIFIED words (series
/// `Dly,0,kDlyMix,0xC0279098,0,4`), EMPIRICAL role (`g_d`).
#[inline]
pub fn delay_dry(v: f32) -> f32 {
    let d = 2.0 - v / 64.0;
    (d * d).min(1.0)
}

/// Delay feedback matrix diagonal `c[-0x1A]`, `c[-0x19]` of `kDlyFbkAmt`: `v / 127`. VERIFIED
/// words, EMPIRICAL role (`m_ll`, `m_rr`).
#[inline]
pub fn delay_feedback(v: f32) -> f32 {
    v / 127.0
}

/// Reverb send (wet) word of `kRvbAmt`: `x^2 (2 - x)` with `x = v / 128`, exactly 1 at `v = 127`.
/// VERIFIED words (all 128 values of `docs/research/data/fx-reverb-amt.csv`, max abs error 1.2e-7;
/// module `0xC02BB44C` words `-0x1F` and the same pair at `0xC02BB54C` word 18), EMPIRICAL role
/// (send / dry of the reverb crossfade).
#[inline]
pub fn reverb_send(v: f32) -> f32 {
    if v >= 127.0 {
        return 1.0;
    }
    let x = v / 128.0;
    x * x * (2.0 - x)
}

/// Reverb dry word of `kRvbAmt`: the equal-power partner of [`reverb_send`], `sqrt(1 - wet^2)`
/// (0 at `v = 127`). VERIFIED words (same series), EMPIRICAL role.
#[inline]
pub fn reverb_dry(v: f32) -> f32 {
    let w = reverb_send(v);
    (1.0 - w * w).max(0.0).sqrt()
}

/// Compressor threshold word `c[-0x1B]` of `kCmpAmt` (log2 level units): `-36 v / 127`. VERIFIED
/// words (series `Cmp,,kCmpAmt,0xC0441558,0,-27`), EMPIRICAL role (it becomes the threshold word
/// through the makeup smoother, `fw_compressor` overlap).
#[inline]
pub fn cmp_threshold(v: f32) -> f32 {
    -36.0 * v / 127.0
}

/// Compressor makeup target `c[-0x1C]` of `kCmpAmt`: `1 + 5.72 (v / 127)^2` (1.0 to 6.72). VERIFIED
/// words (series `Cmp,,kCmpAmt,0xC0441558,0,-28`), EMPIRICAL role.
#[inline]
pub fn cmp_makeup(v: f32) -> f32 {
    let u = v / 127.0;
    1.0 + 5.72 * u * u
}

/// Attack pair of the compressor (`c[-0x20]`, `c[-0x1F]`), the same for every `kCmpAmt` and
/// `kCmpResponse` value captured. VERIFIED words, EMPIRICAL roles (position only).
pub const CMP_ATTACK: [f32; 2] = [0.102_071, 1.0];
/// Release pair of the compressor (`c[-0x1E]`, `c[-0x1D]`). VERIFIED words, EMPIRICAL roles.
pub const CMP_RELEASE: [f32; 2] = [4.607e-4, 0.04];

/// Amp drive stage phase target `c[-0x20]` of `kAmpDrive` (module `0xC04438D8#0`): `v / 127`.
/// VERIFIED words (all ten Amp types), EMPIRICAL role (drive amount, not an oscillator phase).
#[inline]
pub fn amp_drive_target(v: f32) -> f32 {
    v / 127.0
}

/// Which Fx1 type of the all-pass module `0xC02BB030#1` (the module serves types 2 and 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllpassType {
    /// Fx1 type 2: amount words `c[-0x1A]` (wet), `c[-0x19]` (dry).
    Two,
    /// Fx1 type 3: wet word `c[-0x16]`, dry word `c[-0x19]`.
    Three,
}

/// All-pass LFO phase increment `c[-0x20]` of `kFx1RateClkDiv` (integer word): `12800 + 16384 v`.
/// VERIFIED (both types, exact integers).
#[inline]
pub fn allpass_rate(v: u32) -> u32 {
    12_800 + 16_384 * v
}

/// All-pass wet word of `kFx1Amt`: `v / 254` (peaks at 0.5). VERIFIED words (both types).
#[inline]
pub fn allpass_wet(v: f32) -> f32 {
    v / 254.0
}

/// All-pass dry word of `kFx1Amt`: `1 - v / 254`. VERIFIED words (both types).
#[inline]
pub fn allpass_dry(v: f32) -> f32 {
    1.0 - v / 254.0
}

/// UI counts of the 13 captured points of the tone controls (non uniform: the steps 1, 15, 30, 45
/// near the ends are the series' sample values).
pub(super) const TONE_COUNTS: [i32; 13] = [-60, -59, -58, -45, -30, -15, -1, 0, 1, 15, 30, 45, 60];

/// Shelf gain word of `kAmpTreble` (`c[-0x1B]`, `F`) and of `kAmpBass` (`c[-0x20]`, `A`): the same
/// curve for both ids and all three units (series `Amp,7,kAmpTreble,..,-27`, `kAmpBass,..,-32`).
/// VERIFIED words at the 13 counts.
const TONE_SHELF_GAIN: [f32; 13] = [
    f32::from_bits(0xBF5279D0),
    f32::from_bits(0xBF512560),
    f32::from_bits(0xBF4FC700),
    f32::from_bits(0xBF39E570),
    f32::from_bits(0xBF140BB0),
    f32::from_bits(0xBEB38440),
    f32::from_bits(0xBCE90400),
    f32::from_bits(0x00000000),
    f32::from_bits(0x3CEAD800),
    f32::from_bits(0x3F0798E0),
    f32::from_bits(0x3FAB7EC8),
    f32::from_bits(0x40250B7C),
    f32::from_bits(0x408F2892),
];

/// Treble shelf pole word `c[-0x1C]` (`E`) of the piano unit (series `Amp,7,kAmpTreble,..,-28`).
/// The organ and synth units differ (`0xBF3EDADA` against `0xBF0A7B80` above 0).
const TONE_SHELF_POLE_PIANO: [f32; 13] = [
    f32::from_bits(0xBF663DB6),
    f32::from_bits(0xBF658708),
    f32::from_bits(0xBF64CB98),
    f32::from_bits(0xBF5961D4),
    f32::from_bits(0xBF46DBC6),
    f32::from_bits(0xBF2D01DE),
    f32::from_bits(0xBF0D1328),
    f32::from_bits(0xBF0A7B80),
    f32::from_bits(0xBF0A7B80),
    f32::from_bits(0xBF0A7B80),
    f32::from_bits(0xBF0A7B80),
    f32::from_bits(0xBF0A7B80),
    f32::from_bits(0xBF0A7B80),
];

/// Bass smoother word `c[-0x14]` (`a`) of the piano unit (series `Amp,7,kAmpBass,..,-20`): 0.0144
/// above 0 and up to 0.0782 at -60.
const TONE_BASS_SMOOTHER_PIANO: [f32; 13] = [
    f32::from_bits(0x3DA03430),
    f32::from_bits(0x3D9BD470),
    f32::from_bits(0x3D979220),
    f32::from_bits(0x3D52F5C0),
    f32::from_bits(0x3D0A3E00),
    f32::from_bits(0x3CB49D00),
    f32::from_bits(0x3C725400),
    f32::from_bits(0x3C6B7C00),
    f32::from_bits(0x3C6B7C00),
    f32::from_bits(0x3C6B7C00),
    f32::from_bits(0x3C6B7C00),
    f32::from_bits(0x3C6B7C00),
    f32::from_bits(0x3C6B7C00),
];

/// Piecewise linear interpolation of a 13-point table at signed `counts` (clamped to `-60..=60`).
pub(super) fn tone_table(table: &[f32; 13], counts: f32) -> f32 {
    let x = counts.clamp(-60.0, 60.0);
    let hi = TONE_COUNTS
        .iter()
        .position(|&c| c as f32 >= x)
        .unwrap_or(12)
        .max(1);
    let (x0, x1) = (TONE_COUNTS[hi - 1] as f32, TONE_COUNTS[hi] as f32);
    let t = (x - x0) / (x1 - x0);
    table[hi - 1] + t * (table[hi] - table[hi - 1])
}

/// `F` / `A` shelf gain word for a treble or bass count. EMPIRICAL interpolation between the 13
/// VERIFIED points (linear in the word between them).
pub fn tone_shelf_gain(counts: f32) -> f32 {
    tone_table(&TONE_SHELF_GAIN, counts)
}

/// `E` shelf pole word for a treble count (piano unit).
pub fn tone_shelf_pole(counts: f32) -> f32 {
    tone_table(&TONE_SHELF_POLE_PIANO, counts)
}

/// `a` bass smoother word for a bass count (piano unit).
pub fn tone_bass_smoother(counts: f32) -> f32 {
    tone_table(&TONE_BASS_SMOOTHER_PIANO, counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_and_db_conversions_use_the_preset_scales() {
        assert_eq!(unit_to_ui(1.0), 127.0);
        assert_eq!(unit_to_ui(-1.0), 0.0);
        assert_eq!(unit_to_ui(2.0), 127.0);
        assert_eq!(db_to_counts(15.0), 60.0);
        assert_eq!(db_to_counts(-100.0), -60.0);
        assert_eq!(db_to_counts(0.0), 0.0);
    }

    #[test]
    fn reverb_law_is_equal_power_and_hits_the_captured_words() {
        // captured words (float32 bits) of docs/research/data/fx-reverb-amt.csv
        assert_eq!((reverb_send(0.0), reverb_dry(0.0)), (0.0, 1.0));
        assert_eq!((reverb_send(127.0), reverb_dry(127.0)), (1.0, 0.0));
        assert_eq!(reverb_send(64.0), 0.375);
        assert!((reverb_dry(64.0) - 0.927_025).abs() < 1e-6);
        assert!((reverb_send(8.0) - 0.007_568_36).abs() < 1e-7);
        assert!((reverb_send(96.0) - 0.703_125).abs() < 1e-7);
        assert!((reverb_send(126.0) - 0.984_135).abs() < 1e-6);
        for v in 0..=127 {
            let (w, d) = (reverb_send(v as f32), reverb_dry(v as f32));
            assert!((w * w + d * d - 1.0).abs() < 1e-5, "v {v}");
        }
    }

    #[test]
    fn mix_law_is_a_crossfade_with_unity_plateaus() {
        assert_eq!(delay_send(0.0), 0.0);
        assert_eq!(delay_send(64.0), 1.0);
        assert_eq!(delay_send(127.0), 1.0);
        assert_eq!(delay_dry(0.0), 1.0);
        assert_eq!(delay_dry(64.0), 1.0);
        assert!(delay_dry(127.0) < 1e-3);
        assert_eq!(allpass_rate(0), 12_800);
        assert_eq!(allpass_rate(127), 0x001F_F200);
        assert_eq!(allpass_wet(127.0), 0.5);
        assert_eq!(allpass_dry(127.0), 0.5);
    }

    #[test]
    fn tone_tables_interpolate_between_nodes_and_clamp() {
        assert_eq!(tone_shelf_gain(0.0), 0.0);
        assert!((tone_shelf_gain(60.0) - 4.474).abs() < 1e-3);
        assert_eq!(tone_shelf_gain(99.0), tone_shelf_gain(60.0));
        let mid = tone_shelf_gain(7.5);
        assert!(mid > tone_shelf_gain(1.0) && mid < tone_shelf_gain(15.0));
        assert!(tone_shelf_pole(-60.0) < tone_shelf_pole(10.0));
        assert!(tone_bass_smoother(-60.0) > tone_bass_smoother(10.0));
    }
}

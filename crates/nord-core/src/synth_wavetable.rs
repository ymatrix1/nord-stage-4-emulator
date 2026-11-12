//! Band-limited, mipmapped single-cycle wavetable bank shared by all Synth layers.
//!
//! ASSUMED: the 64 tables are synthetic harmonic families, not the hardware waves; the
//! hardware WAVE category/wave order maps onto them via `cbin::synth_osc::wave_flat_index`.

use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

/// Total number of single-cycle wavetables in the Nord Wave 2 derived bank (`0..=63`).
pub const NUM_WAVETABLES: usize = 64;

/// Number of octave mipmap levels per wavetable to prevent Nyquist aliasing.
pub(crate) const WAVETABLE_MIPS: usize = 8;

/// Single-cycle wavetable lookup resolution in samples.
pub(crate) const WAVETABLE_SIZE: usize = 256;

/// Maximum harmonic indices permitted at each mipmap level (2x oversampled guard).
const MIP_MAX_HARMONICS: [usize; WAVETABLE_MIPS] = [128, 64, 32, 16, 8, 4, 2, 1];

/// Precomputed band-limited wavetable bank shared across all `SynthLayer` instances.
pub(crate) struct WavetableBank {
    pub(crate) tables: [[[f32; WAVETABLE_SIZE]; WAVETABLE_MIPS]; NUM_WAVETABLES],
}

static WAVETABLE_BANK: OnceLock<Box<WavetableBank>> = OnceLock::new();

fn wavetable_harmonic_amp(table_idx: usize, k: usize) -> (f32, f32) {
    let family = table_idx / 8;
    let variant = (table_idx % 8) as f32;
    let kf = k as f32;
    match family {
        // 0..7: Analog morphs (sine -> tri -> saw -> pulse/square)
        0 => {
            let tilt = 0.6 + 0.12 * variant;
            let amp = 1.0 / kf.powf(tilt);
            let odd_bias = if k % 2 == 1 {
                1.0
            } else {
                (variant / 7.0) * 0.95
            };
            (amp * odd_bias, 0.0)
        }
        // 8..15: Formant vocal peaks (A/E/I/O/U vowel formants)
        1 => {
            let f1 = 2.5 + variant * 0.85;
            let f2 = 7.0 + variant * 1.4;
            let g1 = (-0.5 * ((kf - f1) / 1.8).powi(2)).exp();
            let g2 = (-0.5 * ((kf - f2) / 2.8).powi(2)).exp();
            let amp = (0.35 / kf) + 0.9 * g1 + 0.55 * g2;
            (amp, 0.25)
        }
        // 16..23: Bell / metallic harmonic clusters
        2 => {
            let density = 1 + (variant as usize % 4);
            if k == 1 || k.is_multiple_of(density + 1) || k == 5 || k == 11 {
                (0.9 / kf.powf(0.55), 0.5 * (variant / 7.0))
            } else {
                (0.05 / kf, 0.0)
            }
        }
        // 24..31: Additive Organ / drawbar recipes
        3 => {
            let match_bar = matches!(k, 1 | 2 | 3 | 4 | 6 | 8 | 10 | 12 | 16);
            let weight = if match_bar {
                0.7 + 0.3 * ((kf + variant) * 0.7).sin().abs()
            } else {
                0.08 / kf
            };
            (weight / kf.powf(0.45), 0.0)
        }
        // 32..39: Hollow / Woodwind / Comb notch
        4 => {
            let notch = 2.0 + variant;
            let comb = ((PI * kf / notch).sin()).abs();
            let amp = (comb / kf.powf(0.85)) * if k % 2 == 1 { 1.0 } else { 0.35 };
            (amp, 0.0)
        }
        // 40..47: Brass / Reed bright edge
        5 => {
            let brightness = 0.55 + 0.07 * (7.0 - variant);
            let bump = (-0.5 * ((kf - (4.0 + variant)) / 2.2).powi(2)).exp();
            (1.0 / kf.powf(brightness) + 0.4 * bump, 0.15)
        }
        // 48..55: Resonant Sync-style harmonic sweep
        6 => {
            let center = 3.0 + variant * 2.2;
            let bw = 2.4 + 0.3 * variant;
            let env = (-0.5 * ((kf - center) / bw).powi(2)).exp();
            (0.25 / kf + 1.15 * env, 0.4)
        }
        // 56..63: Deep Bass / Rich fundamental + sub-harmonic weight
        _ => {
            let cutoff_harm = 3.0 + variant * 1.5;
            let lp = 1.0 / (1.0 + (kf / cutoff_harm).powi(2));
            let fund = if k == 1 { 1.5 } else { 1.0 };
            (fund * lp / kf.sqrt(), 0.0)
        }
    }
}

pub(crate) fn wavetable_bank() -> &'static WavetableBank {
    WAVETABLE_BANK.get_or_init(|| {
        let mut bank = Box::new(WavetableBank {
            tables: [[[0.0; WAVETABLE_SIZE]; WAVETABLE_MIPS]; NUM_WAVETABLES],
        });
        for (t_idx, mip_set) in bank.tables.iter_mut().enumerate() {
            let mut ref_norm = 1.0f32;
            for (mip_idx, &max_h) in MIP_MAX_HARMONICS.iter().enumerate() {
                let table = &mut mip_set[mip_idx];
                for (s_idx, sample) in table.iter_mut().enumerate() {
                    let phase = (s_idx as f32) / (WAVETABLE_SIZE as f32);
                    let mut sum = 0.0f32;
                    for k in 1..=max_h {
                        let (amp, phase_skew) = wavetable_harmonic_amp(t_idx, k);
                        // Lanczos sigma factor avoids Gibbs ringing on sharp truncations
                        let sigma = if max_h > 4 {
                            let x = PI * (k as f32) / ((max_h + 1) as f32);
                            x.sin() / x
                        } else {
                            1.0
                        };
                        sum += amp * sigma * (TAU * (k as f32) * phase + phase_skew).sin();
                    }
                    *sample = sum;
                }
                if mip_idx == 0 {
                    let peak = table.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
                    ref_norm = if peak > 1e-6 { 1.0 / peak } else { 1.0 };
                }
                for sample in table.iter_mut() {
                    *sample *= ref_norm;
                }
            }
        }
        bank
    })
}

/// Evaluates a band-limited mipmapped wavetable sample with position formant scan and interpolation.
#[inline]
pub(crate) fn sample_wavetable(
    bank: &WavetableBank,
    table_idx: u8,
    position: f32,
    phase: f32,
    dt: f32,
) -> f32 {
    let max_non_aliasing = (0.48 / dt.max(1e-5)).floor() as usize;
    let mip = MIP_MAX_HARMONICS
        .iter()
        .position(|&limit| limit <= max_non_aliasing)
        .unwrap_or(WAVETABLE_MIPS - 1);

    let t0 = (table_idx as usize) % NUM_WAVETABLES;
    let t1 = (t0 + 1) % NUM_WAVETABLES;
    let pos = position.clamp(0.0, 1.0);
    // Gentle phase warp provides continuous spectral formant motion even on a single table index
    let warped_phase = (phase + 0.16 * pos * (TAU * phase).sin()).rem_euclid(1.0);
    let idx_f = warped_phase * (WAVETABLE_SIZE as f32);
    let i0 = (idx_f.floor() as usize) % WAVETABLE_SIZE;
    let i1 = (i0 + 1) % WAVETABLE_SIZE;
    let frac = idx_f - idx_f.floor();

    let s0 = bank.tables[t0][mip][i0] * (1.0 - frac) + bank.tables[t0][mip][i1] * frac;
    let s1 = bank.tables[t1][mip][i0] * (1.0 - frac) + bank.tables[t1][mip][i1] * frac;
    (1.0 - pos) * s0 + pos * s1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_tables_are_normalised_and_finite() {
        let bank = wavetable_bank();
        for table in bank.tables.iter() {
            let peak = table[0].iter().fold(0.0f32, |m, &x| m.max(x.abs()));
            assert!((peak - 1.0).abs() < 1e-3 || peak == 0.0, "peak={peak}");
            assert!(table.iter().flatten().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn sample_wavetable_is_bounded_and_wraps_table_index() {
        let bank = wavetable_bank();
        for step in 0..64 {
            let phase = step as f32 / 64.0;
            let v = sample_wavetable(bank, 200, 0.5, phase, 0.01);
            assert!(v.is_finite() && v.abs() < 4.0);
        }
        let a = sample_wavetable(bank, 3, 0.0, 0.25, 0.01);
        let b = sample_wavetable(bank, 3 + NUM_WAVETABLES as u8, 0.0, 0.25, 0.01);
        assert_eq!(a, b);
    }
}

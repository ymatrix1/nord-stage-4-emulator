//! `Mod 1` dynamics-driven types: `A-Wah` (envelope-follower auto-wah) and `Pump` (ducking LFO).
//!
//! Manual (OS v1.6x p.49-50): A-WAH uses the signal amplitude (envelope follower) to control the filter
//! sweep range, SENS (Rate knob) sets sensitivity and range, VARIATION is a different model. PUMP is a quick
//! "duck" followed by a slower recovery, driven by the Rate LFO (optionally Master Clock synced) or, in PED
//! mode, by a pedal / foot switch. No firmware DSP for either is decoded, so every number below is
//! `ASSUMED`/`EMPIRICAL`; see `docs/specs/effects-and-routing.md`.

use super::*;

/// EMPIRICAL: envelope-follower attack ~ 5 ms. Typical envelope-filter / auto-wah detectors use a fast attack of a
/// few ms (https://en.wikipedia.org/wiki/Envelope_follower, https://en.wikipedia.org/wiki/Auto-Wah). Not compared
/// to the hardware; confirm by recording A-Wah on a Stage 4 with a gated sine burst and measuring the cutoff rise time.
const AWAH_ATTACK_S: f32 = 0.005;
/// ASSUMED: envelope release ~ 120 ms (slow enough not to flutter on low notes, fast enough for staccato playing).
const AWAH_RELEASE_S: f32 = 0.12;
/// ASSUMED: detector gain at the minimum SENS (Rate = 0.1 Hz) and the span to maximum SENS (two decades).
const AWAH_SENS_MIN: f32 = 2.0;
const AWAH_SENS_SPAN_DECADES: f32 = 2.0;
/// ASSUMED: model A (standard) band-pass sweep `350 Hz * 8^sweep` = same range as the LFO `Wah`; model B (VARIATION)
/// resonant low-pass sweep `150 Hz * 24^sweep` with higher Q, the classic envelope-filter voicing
/// (https://en.wikipedia.org/wiki/Auto-Wah). Confirm against a hardware sweep recording.
const AWAH_A_FC_MIN_HZ: f32 = 350.0;
const AWAH_A_SPAN: f32 = 8.0;
const AWAH_B_FC_MIN_HZ: f32 = 150.0;
const AWAH_B_SPAN: f32 = 24.0;
const AWAH_A_DAMP: f32 = 0.35;
const AWAH_B_DAMP: f32 = 0.2;
/// ASSUMED: wet makeup gains of the two models (A equals the LFO `Wah`).
const AWAH_A_GAIN: f32 = 1.4;
const AWAH_B_GAIN: f32 = 1.0;

/// ASSUMED: duck attack 4 ms. Side-chain pumping uses a few-ms compressor attack
/// (https://en.wikipedia.org/wiki/Dynamic_range_compression, side-chaining). Capped to 10 % of the period so fast
/// rates keep a recovery. Confirm by recording Pump on a Stage 4 and measuring the duck edge.
const PUMP_ATTACK_S: f32 = 0.004;
/// ASSUMED: foot-switch / pedal threshold (rising edge) and re-arm hysteresis for PED mode.
const PUMP_PED_ON: f32 = 0.5;
const PUMP_PED_OFF: f32 = 0.4;

/// DSP state of the `A-Wah` and `Pump` types (preallocated, `Copy`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Mod1Dyn {
    env: f32,
    svf_l: [f32; 2],
    svf_r: [f32; 2],
    cutoff_hz: f32,
    /// Pump gain-curve phase `0..=1` (`1` = idle after the last one-shot duck in PED mode).
    pump_phase: f32,
    pump_armed: bool,
}

impl Mod1Dyn {
    pub(super) const fn new() -> Self {
        Self {
            env: 0.0,
            svf_l: [0.0; 2],
            svf_r: [0.0; 2],
            cutoff_hz: AWAH_A_FC_MIN_HZ,
            pump_phase: 1.0,
            pump_armed: true,
        }
    }
}

/// Ducking depth `0..=1` at normalised phase `p`: linear attack over `a_frac`, then squared recovery to 0 at `p = 1`.
#[inline]
pub(super) fn pump_duck(p: f32, a_frac: f32) -> f32 {
    if p < a_frac {
        p / a_frac
    } else {
        let u = ((p - a_frac) / (1.0 - a_frac)).min(1.0);
        (1.0 - u) * (1.0 - u)
    }
}

impl LayerFxChain {
    /// Current `A-Wah` filter cutoff in Hz (diagnostic / test hook).
    pub fn awah_cutoff_hz(&self) -> f32 {
        self.dyn1.cutoff_hz
    }

    /// Sweep position `0..=1` of the A-Wah from the follower level and the SENS (Rate) setting.
    #[inline]
    fn awah_sweep(&self) -> f32 {
        let norm = ((self.mod1_rate_hz - 0.1) / 14.9).clamp(0.0, 1.0);
        let gain = AWAH_SENS_MIN * 10.0f32.powf(AWAH_SENS_SPAN_DECADES * norm);
        1.0 - (-self.dyn1.env * gain).exp()
    }

    pub(super) fn process_awah(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        let amt = self.mod1_amount.clamp(0.0, 1.0);
        // Envelope follower on the stereo-linked rectified level.
        let level = 0.5 * (l.abs() + r.abs());
        let tau = if level > self.dyn1.env {
            AWAH_ATTACK_S
        } else {
            AWAH_RELEASE_S
        };
        self.dyn1.env += (level - self.dyn1.env) * (inv_fs / tau).min(1.0);
        if self.dyn1.env < 1e-12 {
            self.dyn1.env = 0.0;
        }
        let sweep = self.awah_sweep();
        let var = self.mod1_variation;
        let (fc, damp, gain) = if var {
            (
                AWAH_B_FC_MIN_HZ * AWAH_B_SPAN.powf(sweep),
                AWAH_B_DAMP,
                AWAH_B_GAIN,
            )
        } else {
            (
                AWAH_A_FC_MIN_HZ * AWAH_A_SPAN.powf(sweep),
                AWAH_A_DAMP,
                AWAH_A_GAIN,
            )
        };
        self.dyn1.cutoff_hz = fc;
        let w = (TAU * fc * inv_fs).clamp(0.02, 0.55);
        let mut out = [0.0f32; 2];
        for (ch, x) in [l, r].into_iter().enumerate() {
            let st = if ch == 0 {
                &mut self.dyn1.svf_l
            } else {
                &mut self.dyn1.svf_r
            };
            let hp = x - st[1] - damp * st[0];
            let bp = st[0] + w * hp;
            let lp = st[1] + w * bp;
            *st = [bp, lp];
            let wet = if var { lp } else { bp } * gain;
            out[ch] = x * (1.0 - amt) + wet * amt;
        }
        out
    }

    pub(super) fn process_pump(&mut self, l: f32, r: f32, inv_fs: f32) -> [f32; 2] {
        let amt = self.mod1_amount.clamp(0.0, 1.0);
        let rate = self.mod1_effective_rate_hz().max(0.01);
        let step = rate * inv_fs;
        let ped = self.mod1_ped;
        let v = self.mod1_ped_pos;
        let d = &mut self.dyn1;
        if ped {
            // PED: one duck per pedal / foot-switch press (rising edge), no LFO.
            if d.pump_armed && v >= PUMP_PED_ON {
                d.pump_phase = 0.0;
                d.pump_armed = false;
            } else if v < PUMP_PED_OFF {
                d.pump_armed = true;
            }
            d.pump_phase = (d.pump_phase + step).min(1.0);
        } else {
            d.pump_phase += step;
            if d.pump_phase >= 1.0 {
                d.pump_phase -= 1.0;
            }
        }
        let a_frac = (PUMP_ATTACK_S * rate).min(0.1);
        let g = 1.0 - amt * pump_duck(d.pump_phase, a_frac);
        [l * g, r * g]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn chain(t: Mod1Type) -> LayerFxChain {
        let mut fx = LayerFxChain::new(FS);
        fx.reverb_model = ReverbModel::Off;
        fx.mod1_type = t;
        fx
    }

    fn dip_count(g: &[f32]) -> usize {
        g.windows(2).filter(|w| w[0] > 0.5 && w[1] <= 0.5).count()
    }

    #[test]
    fn pump_duck_curve_is_continuous_fast_attack_slow_release() {
        let a = 0.01;
        assert_eq!(pump_duck(0.0, a), 0.0);
        assert!((pump_duck(a, a) - 1.0).abs() < 1e-6);
        assert!(pump_duck(1.0, a).abs() < 1e-6);
        assert!(pump_duck(0.5, a) < 0.3);
    }

    #[test]
    fn pump_ducks_periodically_with_fast_attack_and_slow_release() {
        let mut fx = chain(Mod1Type::Pump);
        fx.mod1_rate_hz = 2.0;
        fx.mod1_amount = 1.0;
        let gains: Vec<f32> = (0..96_000)
            .map(|_| fx.process_frame([1.0, 1.0])[0])
            .collect();
        // gain crosses 0.5 downwards once per period (24000 samples)
        let falls: Vec<usize> = (1..gains.len())
            .filter(|&n| gains[n - 1] > 0.5 && gains[n] <= 0.5)
            .collect();
        assert!(falls.len() >= 3, "falls: {falls:?}");
        for w in falls.windows(2) {
            assert!(
                (w[1] - w[0]).abs_diff(24_000) < 400,
                "period {} samples",
                w[1] - w[0]
            );
        }
        // attack: 1.0 -> minimum in <= 8 ms; release: minimum -> 0.9 takes >= 5x longer
        let f = falls[1];
        let top = (0..f).rev().find(|&i| gains[i] > 0.97).unwrap();
        let bottom = (f..f + 4000)
            .min_by(|&a, &b| gains[a].total_cmp(&gains[b]))
            .unwrap();
        let attack = bottom - top;
        let back = (bottom..gains.len()).find(|&i| gains[i] > 0.9).unwrap();
        let release = back - bottom;
        assert!(gains[bottom] < 0.02, "min gain {}", gains[bottom]);
        assert!(attack < 384, "attack {attack} samples");
        assert!(release > 5 * attack, "attack {attack} release {release}");
    }

    #[test]
    fn pump_follows_master_clock_sync_beats() {
        let mut fx = chain(Mod1Type::Pump);
        fx.mod1_rate_hz = 9.0; // ignored while synced
        fx.mod1_amount = 1.0;
        fx.mod1_sync_beats = Some(1.0); // quarter notes at 120 BPM = 2 Hz
        let gains: Vec<f32> = (0..48_000)
            .map(|_| fx.process_frame_with_bpm([1.0, 1.0], 120.0)[0])
            .collect();
        assert_eq!(dip_count(&gains), 2);
    }

    #[test]
    fn pump_ped_ducks_once_per_press_and_not_without_pedal() {
        let mut fx = chain(Mod1Type::Pump);
        fx.mod1_ped = true;
        fx.mod1_rate_hz = 4.0;
        fx.mod1_amount = 1.0;
        fx.mod1_ped_pos = 0.0;
        let quiet: Vec<f32> = (0..24_000)
            .map(|_| fx.process_frame([1.0, 1.0])[0])
            .collect();
        // the very first sample may still sit in the initial idle ramp
        assert!(
            quiet.iter().skip(4000).all(|&g| g > 0.999),
            "pump without pedal press"
        );
        fx.mod1_ped_pos = 1.0;
        let held: Vec<f32> = (0..48_000)
            .map(|_| fx.process_frame([1.0, 1.0])[0])
            .collect();
        assert!(held.iter().cloned().fold(1.0, f32::min) < 0.05, "no duck");
        assert_eq!(dip_count(&held), 1, "held pedal must not retrigger");
        fx.mod1_ped_pos = 0.0;
        for _ in 0..100 {
            fx.process_frame([1.0, 1.0]);
        }
        fx.mod1_ped_pos = 1.0;
        let again: Vec<f32> = (0..24_000)
            .map(|_| fx.process_frame([1.0, 1.0])[0])
            .collect();
        assert!(again.iter().cloned().fold(1.0, f32::min) < 0.05);
    }

    fn settle_cutoff(fx: &mut LayerFxChain, amp: f32) -> f32 {
        for n in 0..24_000 {
            let x = amp * (TAU * 220.0 * n as f32 / FS).sin();
            fx.process_frame([x, x]);
        }
        fx.awah_cutoff_hz()
    }

    #[test]
    fn awah_louder_input_opens_the_filter_further() {
        for var in [false, true] {
            let mut last = 0.0;
            for amp in [0.0, 0.02, 0.1, 0.5] {
                let mut fx = chain(Mod1Type::AutoWah);
                fx.mod1_variation = var;
                fx.mod1_amount = 1.0;
                let fc = settle_cutoff(&mut fx, amp);
                assert!(fc > last, "var {var} amp {amp}: {fc} <= {last}");
                last = fc;
            }
        }
    }

    #[test]
    fn awah_sens_knob_raises_the_sweep_and_amount_zero_is_transparent() {
        let mut lo = chain(Mod1Type::AutoWah);
        lo.mod1_rate_hz = 0.1;
        let mut hi = chain(Mod1Type::AutoWah);
        hi.mod1_rate_hz = 15.0;
        assert!(settle_cutoff(&mut hi, 0.05) > settle_cutoff(&mut lo, 0.05));
        let mut dry = chain(Mod1Type::AutoWah);
        dry.mod1_amount = 0.0;
        for n in 0..2000 {
            let x = 0.3 * (n as f32 * 0.1).sin();
            assert_eq!(dry.process_frame([x, x]), [x, x]);
        }
    }

    #[test]
    fn awah_variation_is_a_different_finite_model() {
        let run = |var: bool| -> Vec<[f32; 2]> {
            let mut fx = chain(Mod1Type::AutoWah);
            fx.mod1_variation = var;
            fx.mod1_amount = 0.8;
            (0..9600)
                .map(|n| {
                    let x = 0.4 * (n as f32 * 0.07).sin();
                    fx.process_frame([x, x])
                })
                .collect()
        };
        let (a, b) = (run(false), run(true));
        assert!(a != b);
        assert!(a
            .iter()
            .chain(&b)
            .all(|f| f[0].is_finite() && f[1].is_finite()));
    }
}

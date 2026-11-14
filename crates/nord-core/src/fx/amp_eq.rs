//! Amp / cabinet, drive, EQ and compressor stage of the per-layer FX chain.

use super::fw_coeffs::{amp_fw_case_for, amp_fw_sets};
use super::*;

impl LayerFxChain {
    /// Runs the firmware drive stage on `(l, r)` when `firmware_fx.amp` is on and the amp has one;
    /// returns whether it did (the caller then skips the engine drive and cabinet).
    fn process_fw_amp(&mut self, l: &mut f32, r: &mut f32) -> bool {
        let kind = fw_amp_types::type_for(self.amp_cabinet, self.amp_variation);
        let (Some(kind), true) = (kind, self.firmware_fx.amp) else {
            self.fw_amp = None;
            return false;
        };
        let drive_ui = fw_map::unit_to_ui(self.drive_amount);
        match &mut self.fw_amp {
            Some(st) if st.kind() == kind => st.set_drive(drive_ui),
            slot => *slot = fw_amp_stage::FwAmpStage::new(kind, drive_ui),
        }
        match &mut self.fw_amp {
            Some(st) => {
                [*l, *r] = st.process_frame([*l, *r]);
                true
            }
            None => false,
        }
    }

    pub(super) fn process_amp_eq_comp(&mut self, mut l: f32, mut r: f32, inv_fs: f32) -> [f32; 2] {
        // Opt-in firmware drive stage (`FirmwareFx::amp`): replaces the tube drive and the cabinet
        // voicing below for the amps whose type switch installs drive stage words.
        let fw_amp = self.process_fw_amp(&mut l, &mut r);

        // Soft-clipping tube drive
        if !fw_amp && self.drive_amount > 0.001 {
            let drv = 1.0 + 4.0 * self.drive_amount.clamp(0.0, 1.0);
            l = (drv * l).tanh();
            r = (drv * r).tanh();
        }

        // Cabinet voicing: 2nd-order Butterworth HP + LP, a peaking mid band and a treble shelf.
        // Direction VERIFIED by the vendor manual (v1.6X p.51): Twin and JC boost the treble range
        // (especially non-variation); Twin variation = less treble boost and a different mid; JC variation
        // = reduced treble and a different mid; Small = Wurlitzer 200A internal speaker, variation = an even
        // smaller amp/speaker. ASSUMED: every Hz and dB value below (no firmware source; one-clip checks only,
        // see docs/research/hardware-audio-reference-fx.md).
        // Optional firmware path: the two cascaded biquad sets of the ARM Amp type switch replace the
        // RBJ voicing for the modes `fw_coeffs::amp_fw_case_for` maps (default off, output unchanged).
        let fw = if self.amp_fw_coeffs && !fw_amp {
            amp_fw_case_for(self.amp_cabinet, self.amp_variation).and_then(amp_fw_sets)
        } else {
            None
        };
        if let Some([s1, s2]) = fw {
            self.cab_hp.c = s1;
            self.cab_lp.c = s2;
            for (ch, sig) in [&mut l, &mut r].into_iter().enumerate() {
                let x = self.cab_hp.process(ch, *sig);
                *sig = self.cab_lp.process(ch, x);
            }
        }
        let cab = fw.is_none()
            && !fw_amp
            && matches!(
                self.amp_cabinet,
                AmpCabinet::Small | AmpCabinet::Jc | AmpCabinet::Twin
            );
        if cab {
            let key = (self.amp_cabinet, self.amp_variation);
            if self.cab_cache != Some(key) {
                let var = self.amp_variation;
                // (hp_hz, lp_hz, mid_hz, mid_db, shelf_hz, shelf_db)
                let (hp_hz, lp_hz, mid_hz, mid_db, shelf_hz, shelf_db) =
                    match (self.amp_cabinet, var) {
                        (AmpCabinet::Twin, false) => (65.0, 14_000.0, 650.0, -2.5, 3000.0, 6.0),
                        (AmpCabinet::Twin, true) => (65.0, 12_000.0, 900.0, 1.0, 3000.0, 2.0),
                        (AmpCabinet::Jc, false) => (70.0, 14_000.0, 2800.0, 2.5, 3000.0, 6.0),
                        (AmpCabinet::Jc, true) => (70.0, 9_000.0, 1800.0, 1.5, 3000.0, -3.0),
                        (_, false) => (140.0, 3400.0, 900.0, 3.2, 3000.0, 0.0),
                        (_, true) => (220.0, 2600.0, 1100.0, 2.0, 3000.0, 0.0),
                    };
                let fs = self.sample_rate;
                self.cab_hp
                    .set(BiquadKind::HighPass, hp_hz, fs, EQ_SHELF_Q, 0.0);
                self.cab_lp
                    .set(BiquadKind::LowPass, lp_hz, fs, EQ_SHELF_Q, 0.0);
                self.cab_mid.set(BiquadKind::Peak, mid_hz, fs, 1.0, mid_db);
                self.cab_shelf
                    .set(BiquadKind::HighShelf, shelf_hz, fs, EQ_SHELF_Q, shelf_db);
                self.cab_cache = Some(key);
            }
            for (ch, sig) in [&mut l, &mut r].into_iter().enumerate() {
                let x = self.cab_hp.process(ch, *sig);
                let x = self.cab_lp.process(ch, x);
                let x = self.cab_mid.process(ch, x);
                *sig = self.cab_shelf.process(ch, x);
            }
        } else {
            self.cab_cache = None;
        }

        // `LP24` / `HP24`: 24 dB/oct (2 cascaded 2nd-order sections), cutoff = EQ Mid Freq knob,
        // resonance = Mid knob. VERIFIED: names `LP24`/`HP24` (0x2cc6cc). ASSUMED: Butterworth-4 section Q
        // (0.5412 / 1.3066) with resonance scaling the second section's Q; real topology unknown.
        if matches!(
            self.amp_cabinet,
            AmpCabinet::LpFilter | AmpCabinet::HpFilter
        ) {
            let mode = if self.amp_cabinet == AmpCabinet::LpFilter {
                0.0
            } else {
                1.0
            };
            let now = [mode, self.eq_mid_freq_hz, self.eq_filter_res];
            if now != self.flt_cache {
                let kind = if mode == 0.0 {
                    BiquadKind::LowPass
                } else {
                    BiquadKind::HighPass
                };
                let res = (self.eq_filter_res / 10.0).clamp(0.0, 1.0);
                let q2 = (1.3066 * (1.0 + 2.0 * res)).min(10.0);
                self.flt_a
                    .set(kind, self.eq_mid_freq_hz, self.sample_rate, 0.5412, 0.0);
                self.flt_b
                    .set(kind, self.eq_mid_freq_hz, self.sample_rate, q2, 0.0);
                self.flt_cache = now;
            }
            for (ch, sig) in [&mut l, &mut r].into_iter().enumerate() {
                let x = self.flt_a.process(ch, *sig);
                *sig = self.flt_b.process(ch, x);
            }
        }

        // 3-band EQ: bass low shelf, sweepable mid peak (200 Hz .. 8 kHz), treble high shelf.
        // True 2nd-order biquads per channel (stereo image preserved); redesigned only on change.
        let eq_now = [
            self.eq_bass_db,
            self.eq_mid_db,
            self.eq_mid_freq_hz,
            self.eq_treble_db,
        ];
        if eq_now != self.eq_cache {
            let fs = self.sample_rate;
            let mid_fc = self.eq_mid_freq_hz.clamp(200.0, 8000.0);
            self.eq_bass.set(
                BiquadKind::LowShelf,
                EQ_BASS_HZ,
                fs,
                EQ_SHELF_Q,
                self.eq_bass_db.clamp(-15.0, 15.0),
            );
            self.eq_mid.set(
                BiquadKind::Peak,
                mid_fc,
                fs,
                firmware_q(EQ_MID_Q_IDX),
                self.eq_mid_db.clamp(-15.0, 15.0),
            );
            self.eq_treble.set(
                BiquadKind::HighShelf,
                EQ_TREBLE_HZ,
                fs,
                EQ_SHELF_Q,
                self.eq_treble_db.clamp(-15.0, 15.0),
            );
            self.eq_cache = eq_now;
        }
        if self.eq_bass_db.abs() > 0.1 {
            l = self.eq_bass.process(0, l);
            r = self.eq_bass.process(1, r);
        }
        let mid_is_res = matches!(
            self.amp_cabinet,
            AmpCabinet::LpFilter | AmpCabinet::HpFilter
        );
        if !mid_is_res && self.eq_mid_db.abs() > 0.1 {
            l = self.eq_mid.process(0, l);
            r = self.eq_mid.process(1, r);
        }
        if self.eq_treble_db.abs() > 0.1 {
            l = self.eq_treble.process(0, l);
            r = self.eq_treble.process(1, r);
        }

        // Opt-in firmware gain computer with the `kCmpAmt` words (latency 4 frames). It runs even
        // while `comp_enabled` is false (amount 0, unity) so the layer latency does not change.
        if self.firmware_fx.compressor {
            let amount = if self.comp_enabled {
                fw_map::unit_to_ui(self.comp_amount)
            } else {
                0.0
            };
            let stage = self.fw_comp.get_or_insert_with(|| {
                fw_stage::FwCompressorStage::new(fw_params::compressor_for_amount(amount))
            });
            if amount != self.fw_comp_amount {
                fw_params::set_compressor_amount(stage.compressor_mut(), amount, false);
            }
            self.fw_comp_amount = amount;
            return stage.process_frame([l, r]);
        }
        self.fw_comp = None;

        // Per-layer compressor (`comp_enabled`, `comp_fast` 1.5 ms vs 12 ms attack)
        if self.comp_enabled && self.comp_amount > 0.001 {
            let peak = l.abs().max(r.abs());
            let atk_tau = if self.comp_fast { 0.0015 } else { 0.012 };
            let rel_tau = 0.085;
            let coeff = if peak > self.comp_env {
                (inv_fs / atk_tau).min(1.0)
            } else {
                (inv_fs / rel_tau).min(1.0)
            };
            self.comp_env += coeff * (peak - self.comp_env);
            let amt = self.comp_amount.clamp(0.0, 1.0);
            let gain = if self.comp_env > 0.35 {
                1.0 / (1.0 + amt * 2.5 * (self.comp_env - 0.35))
            } else {
                1.0 + 0.12 * amt
            };
            l *= gain;
            r *= gain;
        }

        [l, r]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amp_eq_stage_is_finite_for_a_loud_input() {
        let mut fx = LayerFxChain::new(48_000.0);
        fx.drive_amount = 1.0;
        for n in 0..512 {
            let x = (n as f32 * 0.05).sin();
            let [l, r] = fx.process_amp_eq_comp(x, -x, 1.0 / 48_000.0);
            assert!(l.is_finite() && r.is_finite());
        }
    }

    #[test]
    fn firmware_amp_sets_are_opt_in_and_only_voice_small_speaker() {
        use crate::fx::support::{cab_chain, tone_gain_db};
        let mut rbj = cab_chain(AmpCabinet::Small, false);
        let mut fw = cab_chain(AmpCabinet::Small, false);
        fw.amp_fw_coeffs = true;
        let (a, b) = (
            tone_gain_db(&mut rbj, 1000.0),
            tone_gain_db(&mut fw, 1000.0),
        );
        assert!(
            (a - b).abs() > 0.2,
            "firmware set must differ from RBJ: {a} vs {b}"
        );
        // case 5: high pass then low pass, band limited (see fw_coeffs tests for the exact shape)
        let lo = tone_gain_db(&mut fw, 60.0);
        let hi = tone_gain_db(&mut fw, 18_000.0);
        assert!(lo < b - 15.0 && hi < b - 25.0, "lo {lo} mid {b} hi {hi}");
        // unmapped modes ignore the flag
        let mut twin = cab_chain(AmpCabinet::Twin, false);
        let mut twin_fw = cab_chain(AmpCabinet::Twin, false);
        twin_fw.amp_fw_coeffs = true;
        assert_eq!(
            tone_gain_db(&mut twin, 3000.0),
            tone_gain_db(&mut twin_fw, 3000.0)
        );
    }
}

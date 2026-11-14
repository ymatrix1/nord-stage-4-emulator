//! Settings sharing between layer FX chains: GROUP mode (Piano / Synth sections) and GLOBAL mode
//! (Delay, Compressor, Reverb). Manual p.48-49: entering a shared mode applies the focused layer's
//! settings to the other layers. Only user settings are copied; DSP state (delay lines, envelopes,
//! LFO phases) stays per chain, so no allocation happens here.

use super::*;

impl LayerFxChain {
    /// Copies the Compressor settings (`comp_*`) of `src`.
    pub fn copy_comp_settings_from(&mut self, src: &LayerFxChain) {
        self.comp_enabled = src.comp_enabled;
        self.comp_amount = src.comp_amount;
        self.comp_fast = src.comp_fast;
    }

    /// Copies the Delay settings (time, sync, filter, feedback effect, mix and morph) of `src`.
    pub fn copy_delay_settings_from(&mut self, src: &LayerFxChain) {
        self.delay_enabled = src.delay_enabled;
        self.delay_time_ms = src.delay_time_ms;
        self.delay_clock_sync = src.delay_clock_sync;
        self.delay_subdivision = src.delay_subdivision;
        self.delay_sync_beats = src.delay_sync_beats;
        self.delay_analog = src.delay_analog;
        self.delay_ping_pong = src.delay_ping_pong;
        self.delay_filter = src.delay_filter;
        self.delay_fx = src.delay_fx;
        self.delay_fx_variation = src.delay_fx_variation;
        self.delay_feedback = src.delay_feedback;
        self.delay_mix = src.delay_mix;
        self.delay_rate_raw = src.delay_rate_raw;
        self.morph.delay_mix = src.morph.delay_mix;
        self.morph.delay_feedback = src.morph.delay_feedback;
    }

    /// Copies the Reverb settings (model, colour, mix and morph) of `src`.
    pub fn copy_reverb_settings_from(&mut self, src: &LayerFxChain) {
        self.reverb_model = src.reverb_model;
        self.reverb_color = src.reverb_color;
        self.reverb_mix = src.reverb_mix;
        self.firmware_fx.reverb_mix = src.firmware_fx.reverb_mix;
        self.morph.reverb_amount = src.morph.reverb_amount;
    }

    /// Copies every user setting of `src` (GROUP mode). `bypass_reverb` and DSP state are kept.
    pub fn copy_all_settings_from(&mut self, src: &LayerFxChain) {
        self.mod1_type = src.mod1_type;
        self.mod1_rate_hz = src.mod1_rate_hz;
        self.mod1_amount = src.mod1_amount;
        self.mod1_variation = src.mod1_variation;
        self.mod1_ped = src.mod1_ped;
        self.mod1_ped_pos = src.mod1_ped_pos;
        self.mod1_sync_beats = src.mod1_sync_beats;
        self.mod2_type = src.mod2_type;
        self.mod2_rate_hz = src.mod2_rate_hz;
        self.mod2_amount = src.mod2_amount;
        self.mod2_variation = src.mod2_variation;
        self.amp_cabinet = src.amp_cabinet;
        self.amp_variation = src.amp_variation;
        self.amp_fw_coeffs = src.amp_fw_coeffs;
        self.eq_filter_res = src.eq_filter_res;
        self.drive_amount = src.drive_amount;
        self.eq_bass_db = src.eq_bass_db;
        self.eq_mid_db = src.eq_mid_db;
        self.eq_mid_freq_hz = src.eq_mid_freq_hz;
        self.eq_treble_db = src.eq_treble_db;
        self.copy_comp_settings_from(src);
        self.copy_delay_settings_from(src);
        self.copy_reverb_settings_from(src);
        self.firmware_fx = src.firmware_fx;
        self.morph = src.morph;
    }

    /// Runs only the Reverb stage on a stereo frame (the shared GLOBAL reverb after the layer mix).
    #[inline]
    pub fn process_reverb_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        self.process_reverb(frame[0], frame[1], 1.0 / self.sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_all_settings_matches_source_and_keeps_bypass() {
        let mut src = LayerFxChain::new(48_000.0);
        src.mod1_type = Mod1Type::Wah;
        src.mod2_variation = true;
        src.delay_sync_beats = Some(0.375);
        src.reverb_color = ReverbColor::Dark;
        src.comp_amount = 0.7;
        let mut dst = LayerFxChain::new(48_000.0);
        dst.bypass_reverb = true;
        dst.copy_all_settings_from(&src);
        assert_eq!(dst.mod1_type, Mod1Type::Wah);
        assert!(dst.mod2_variation);
        assert_eq!(dst.delay_sync_beats, Some(0.375));
        assert_eq!(dst.reverb_color, ReverbColor::Dark);
        assert_eq!(dst.comp_amount, 0.7);
        assert!(dst.bypass_reverb);
    }

    #[test]
    fn reverb_frame_is_finite_and_has_a_tail() {
        let mut fx = LayerFxChain::new(48_000.0);
        let mut tail = 0.0f32;
        for n in 0..4096 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let [l, r] = fx.process_reverb_frame([x, x]);
            assert!(l.is_finite() && r.is_finite());
            if n > 2000 {
                tail += l.abs();
            }
        }
        assert!(tail > 0.0);
    }
}

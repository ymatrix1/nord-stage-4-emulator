//! Master 4-section IIR of the FX DSP pass (optional stage of [`super::MasterBus`]).
//!
//! The firmware runs four transposed direct form II sections (`fw_coeffs::MASTER_IIR_WORDS`,
//! PM `0x945F2..0x94605`) over the summed bus. The sets form a flat-passband low pass (-0.0001 dB
//! at 5 kHz, about -12 dB at 12 kHz, below -50 dB at 20 kHz at 48 kHz): EMPIRICAL (computed from the
//! verified words by `master_iir_is_flat_in_band_and_cuts_the_top_octave`). The coefficients are
//! designed for the 48 kHz firmware rate (ASSUMED); at other rates the corner scales with the rate.

use super::biquad::Biquad;
use super::fw_coeffs::master_iir_sets;

/// Four cascaded stereo sections with the firmware coefficient sets.
#[derive(Debug, Clone, Copy)]
pub(super) struct MasterIir {
    sections: [Biquad; 4],
}

impl MasterIir {
    pub(super) fn new() -> Self {
        let mut sections = [Biquad::identity(); 4];
        for (b, c) in sections.iter_mut().zip(master_iir_sets()) {
            b.c = c;
        }
        Self { sections }
    }

    /// One stereo frame through the four sections (f32 transposed form II, allocation free).
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let mut out = frame;
        for (ch, v) in out.iter_mut().enumerate() {
            for s in &mut self.sections {
                *v = s.process(ch, *v);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_passes_with_unity_gain_and_the_top_octave_is_cut() {
        let mut iir = MasterIir::new();
        let mut y = [0.0; 2];
        for _ in 0..2000 {
            y = iir.process_frame([0.5, -0.25]);
        }
        assert!((y[0] - 0.5).abs() < 1e-3 && (y[1] + 0.25).abs() < 1e-3);
        // 20 kHz at 48 kHz
        let mut iir = MasterIir::new();
        let mut peak = 0.0f32;
        for n in 0..4000 {
            let x = (std::f32::consts::TAU * 20_000.0 / 48_000.0 * n as f32).sin();
            let [l, _] = iir.process_frame([x, x]);
            if n > 2000 {
                peak = peak.max(l.abs());
            }
        }
        assert!(peak < 0.01, "peak {peak}");
    }
}

//! Analog oscillator waveforms of `SynthLayer` (`Saw`, `SuperSaw`, `Pulse`, `Triangle`, `Sine`).

use super::fw_shape::fw_shape_wave;
use super::hw_saw::saw_wave;
use super::p_block::PBlock;
use super::*;
use crate::phase_wrap::wrap_pos;

impl SynthLayer {
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub(super) fn render_va(
        waveform: VaWaveform,
        saw_model: SawModel,
        shape_stage: Option<f32>,
        p_block: Option<PBlock>,
        supersaw_detune: f32,
        measured: bool,
        pulse_width: f32,
        lfo_dest: LfoDestination,
        lfo_amount: f32,
        lfo_val: f32,
        t: f32,
        dt: f32,
        eff_freq: f32,
        inv_fs: f32,
        supersaw_phases: &mut [f32; SUPERSAW_VOICES],
    ) -> f32 {
        match waveform {
            VaWaveform::Saw => {
                let first = saw_wave(saw_model, t, dt, inv_fs);
                match p_block {
                    Some(pb) => {
                        pb.single()
                            .mix(first, dt, eff_freq, supersaw_phases, |t2, d2, _| {
                                saw_wave(saw_model, t2, d2, inv_fs)
                            })
                    }
                    None => first,
                }
            }
            VaWaveform::SuperSaw if measured => {
                let d = supersaw_detune.clamp(0.0, 1.0) * 10.0;
                let mut sum = 0.0f32;
                for (i, ph) in supersaw_phases.iter_mut().enumerate() {
                    let f_osc = eff_freq * supersaw_voice_ratio(i, d);
                    let dt_i = (f_osc * inv_fs).clamp(1e-5, 0.49);
                    sum += saw_wave(saw_model, *ph, dt_i, inv_fs);
                    *ph = wrap_pos(*ph + dt_i, 1.0);
                }
                sum * (1.0 / SUPERSAW_VOICES as f32)
            }
            VaWaveform::SuperSaw => {
                let mut sum = 0.0f32;
                let detune_scale = supersaw_detune.clamp(0.0, 1.0);
                for (i, &det) in SUPERSAW_DETUNES.iter().enumerate() {
                    let f_osc = eff_freq * (1.0 + det * detune_scale * 0.08);
                    let dt_i = (f_osc * inv_fs).clamp(1e-5, 0.49);
                    let p_i = supersaw_phases[i];
                    let saw = saw_wave(saw_model, p_i, dt_i, inv_fs);
                    sum += saw;
                    supersaw_phases[i] = wrap_pos(p_i + dt_i, 1.0);
                }
                sum * (1.0 / 7.0)
            }
            VaWaveform::Pulse => {
                let mut pw = pulse_width.clamp(0.05, 0.95);
                if lfo_dest == LfoDestination::OscCtrl {
                    pw = (pw + lfo_amount * lfo_val * 0.4).clamp(0.05, 0.95);
                }
                let base = if t < pw { 1.0 } else { -1.0 };
                base + poly_blep(t, dt) - poly_blep((t + 1.0 - pw) % 1.0, dt)
            }
            VaWaveform::Triangle => match shape_stage {
                Some(shape) => {
                    let first = fw_shape_wave(t, dt, eff_freq, shape);
                    match p_block {
                        Some(pb) => pb.mix(first, dt, eff_freq, supersaw_phases, |t2, d2, f2| {
                            fw_shape_wave(t2, d2, f2, shape)
                        }),
                        None => first,
                    }
                }
                None => 1.0 - 4.0 * (t - 0.5).abs(),
            },
            VaWaveform::Sine => (TAU * t).sin(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(w: VaWaveform, model: SawModel, t: f32) -> f32 {
        let mut ph = [0.0f32; SUPERSAW_VOICES];
        SynthLayer::render_va(
            w,
            model,
            None,
            None,
            0.3,
            false,
            0.5,
            LfoDestination::FilterCutoff,
            0.0,
            0.0,
            t,
            0.001,
            100.0,
            1.0 / 48_000.0,
            &mut ph,
        )
    }

    #[test]
    fn basic_shapes_have_their_textbook_values() {
        assert!(wave(VaWaveform::Sine, SawModel::Classic, 0.25) > 0.999);
        assert!((wave(VaWaveform::Triangle, SawModel::Classic, 0.5) - 1.0).abs() < 1e-6);
        assert!(wave(VaWaveform::Saw, SawModel::Classic, 0.25) < 0.0);
        assert!(wave(VaWaveform::Saw, SawModel::Classic, 0.75) > 0.0);
        assert!((wave(VaWaveform::Pulse, SawModel::Classic, 0.25) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn firmware_saw_is_falling_and_the_sy_model_matches_it_at_low_pitch() {
        let a = wave(VaWaveform::Saw, SawModel::Firmware, 0.25);
        let b = wave(VaWaveform::Saw, SawModel::FirmwareSy, 0.25);
        assert!(a > 0.0, "falling saw is positive early in the cycle");
        assert!((a - b).abs() < 1e-6, "{a} vs {b}");
    }
}

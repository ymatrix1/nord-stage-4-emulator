//! `SAMPLES` oscillator: looped multisample playback, sinc interpolation and timbre damping.
//!
//! Interpolation is selected by [`SAMPLE_INTERPOLATION`]: the firmware's 12-tap x 256-phase
//! Kaiser(8.5) sinc ([`crate::sinc`], kernel chosen per playback ratio) or the legacy 4-point
//! Catmull-Rom (`cubic_pcm`). Kernels are built once (see [`shared_sinc_kernels`]); the render
//! path only reads them.

use super::*;
use crate::sinc::{SincKernel, SincKernelSet};
use std::sync::OnceLock;

/// Interpolator used by the `SAMPLES` oscillator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleInterpolation {
    /// Legacy 4-point Catmull-Rom (4 taps, no anti-alias at ratios above 1).
    Cubic,
    /// Firmware 12-tap Kaiser(8.5) sinc, kernel picked by `SincKernelSet::for_ratio`.
    Sinc,
}

/// Active `SAMPLES` interpolator (TD-08).
///
/// ASSUMED: the hardware reads these kernels in the SY1SY2 resampler (kernel values VERIFIED in
/// `crate::sinc`), but which kernel the ARM selects for a pitch is not decoded
/// (`SincKernelSet::select`, ASSUMED nearest `fc` to `min(1, 1/ratio)`). Flip to
/// `SampleInterpolation::Cubic` to restore the previous behaviour; no other code changes.
pub const SAMPLE_INTERPOLATION: SampleInterpolation = SampleInterpolation::Sinc;

/// Process-wide kernel set, built on first use (always at `SynthLayer::new`, never in render).
pub(super) fn shared_sinc_kernels() -> Arc<SincKernelSet> {
    static KERNELS: OnceLock<Arc<SincKernelSet>> = OnceLock::new();
    KERNELS
        .get_or_init(|| Arc::new(SincKernelSet::new()))
        .clone()
}

impl SynthLayer {
    #[inline]
    pub(super) fn sample_with_loop(
        kernel: &SincKernel,
        pcm: &[f32],
        wrapped: f64,
        ls: usize,
        le_c: usize,
        crossfade: usize,
    ) -> f32 {
        let primary = Self::interp_pcm(kernel, pcm, wrapped);
        if crossfade > 0 {
            let xf = crossfade.min((le_c - ls) / 2) as f64;
            let dist_to_end = le_c as f64 - wrapped;
            if xf >= 1.0 && dist_to_end < xf && ls as f64 >= dist_to_end {
                let fade = (xf - dist_to_end) / xf;
                let pre_loop = Self::interp_pcm(kernel, pcm, ls as f64 - dist_to_end);
                return primary * (1.0 - fade as f32) + pre_loop * (fade as f32);
            }
        }
        primary
    }

    #[inline]
    pub(super) fn render_sample_zone_stereo(
        kernels: &SincKernelSet,
        zone: &LoadedSynthSample,
        sample_pos: &mut f64,
        step: f64,
        active: &mut bool,
        kernel_index: Option<usize>,
    ) -> (f32, Option<f32>) {
        // `Some(k)`: the firmware key rule (`sinc::key_rule`); `None`: the ASSUMED ratio rule.
        let kernel = match kernel_index {
            Some(k) => kernels.get(k),
            None => kernels.for_ratio(step as f32),
        };
        let len = zone.pcm.len();
        if len < 2 {
            *active = false;
            return (0.0, None);
        }
        let pcm_r = zone.pcm_right.as_deref().filter(|r| r.len() >= 2);
        let pos = *sample_pos;
        if let (Some(ls), Some(le)) = (zone.loop_start, zone.loop_end) {
            let le_c = le.min(len);
            if le_c > ls {
                let loop_len = (le_c - ls) as f64;
                let wrapped = if pos >= le_c as f64 {
                    ls as f64 + (pos - le_c as f64) % loop_len
                } else {
                    pos
                };
                let out_l = Self::sample_with_loop(
                    kernel,
                    &zone.pcm,
                    wrapped,
                    ls,
                    le_c,
                    zone.loop_crossfade,
                );
                let out_r = pcm_r.map(|r| {
                    let le_r = le_c.min(r.len());
                    if le_r > ls {
                        Self::sample_with_loop(kernel, r, wrapped, ls, le_r, zone.loop_crossfade)
                    } else {
                        Self::interp_pcm(kernel, r, wrapped)
                    }
                });
                let next_pos = wrapped + step;
                *sample_pos = if next_pos >= le_c as f64 {
                    ls as f64 + (next_pos - le_c as f64) % loop_len
                } else {
                    next_pos
                };
                return (out_l, out_r);
            }
        }

        if pos >= (len - 1) as f64 {
            *active = false;
            return (0.0, None);
        }
        let out_l = Self::interp_pcm(kernel, &zone.pcm, pos);
        let out_r = pcm_r.map(|r| Self::interp_pcm(kernel, r, pos));
        *sample_pos = pos + step;
        (out_l, out_r)
    }

    /// Interpolates `pcm` at `pos` with the interpolator named by [`SAMPLE_INTERPOLATION`].
    #[inline]
    pub(super) fn interp_pcm(kernel: &SincKernel, pcm: &[f32], pos: f64) -> f32 {
        match SAMPLE_INTERPOLATION {
            SampleInterpolation::Sinc => kernel.interpolate(pcm, pos),
            SampleInterpolation::Cubic => Self::cubic_pcm(pcm, pos),
        }
    }

    #[inline]
    pub(super) fn cubic_pcm(pcm: &[f32], pos: f64) -> f32 {
        let len = pcm.len();
        if len == 0 {
            return 0.0;
        }
        let max_idx = len - 1;
        let idx1 = (pos.floor() as usize).min(max_idx);
        let idx2 = (idx1 + 1).min(max_idx);
        let frac = (pos - (idx1 as f64)).clamp(0.0, 1.0) as f32;
        let p0 = pcm[idx1.saturating_sub(1)];
        let p1 = pcm[idx1];
        let p2 = pcm[idx2];
        let p3 = pcm[(idx2 + 1).min(max_idx)];
        let t2 = frac * frac;
        let t3 = t2 * frac;
        0.5 * ((2.0 * p1)
            + (-p0 + p2) * frac
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
    }

    #[inline]
    pub(super) fn apply_sample_timbre(
        timbre: SampleTimbre,
        sample: f32,
        lp_state: &mut f32,
    ) -> f32 {
        match timbre {
            SampleTimbre::Off => sample,
            SampleTimbre::Soft => {
                *lp_state += 0.25 * (sample - *lp_state);
                *lp_state
            }
            SampleTimbre::Bright => {
                *lp_state += 0.18 * (sample - *lp_state);
                sample + 0.55 * (sample - *lp_state)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sinc_is_the_default_and_beats_cubic_on_a_fractional_sine() {
        assert_eq!(SAMPLE_INTERPOLATION, SampleInterpolation::Sinc);
        let w = 0.3 * TAU as f64; // 0.3 fs, where Catmull-Rom is weakest
        let pcm: Vec<f32> = (0..256).map(|i| (w * i as f64).sin() as f32).collect();
        let kernels = SincKernelSet::new();
        let kernel = kernels.for_ratio(1.0);
        let (mut e_sinc, mut e_cubic) = (0.0f64, 0.0f64);
        for i in 0..400 {
            let pos = 20.0 + i as f64 * 0.449;
            let truth = (w * pos).sin();
            let a = f64::from(SynthLayer::interp_pcm(kernel, &pcm, pos)) - truth;
            let b = f64::from(SynthLayer::cubic_pcm(&pcm, pos)) - truth;
            e_sinc = e_sinc.max(a.abs());
            e_cubic = e_cubic.max(b.abs());
        }
        assert!(e_sinc * 5.0 < e_cubic, "sinc {e_sinc} vs cubic {e_cubic}");
    }

    #[test]
    fn zone_renderer_picks_a_lower_cutoff_kernel_when_pitched_up() {
        assert!(SincKernelSet::select(2.0) > SincKernelSet::select(1.0));
        let kernels = shared_sinc_kernels();
        assert!(
            Arc::ptr_eq(&kernels, &shared_sinc_kernels()),
            "one shared set"
        );
        let pcm: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin()).collect();
        let sound = SynthSampleSound::new_synthetic("Z", 60, &pcm, 48_000.0, None, 0);
        let (mut pos, mut active) = (3.25f64, true);
        let (l, r) = SynthLayer::render_sample_zone_stereo(
            &kernels,
            &sound.zones[0],
            &mut pos,
            2.0,
            &mut active,
            None,
        );
        assert!(active && r.is_none() && l.is_finite());
        assert!((pos - 5.25).abs() < 1e-12);
    }

    #[test]
    fn key_rule_overrides_the_ratio_rule_and_kernel_index_matches_for_ratio() {
        use crate::sinc::key_rule::kernel_for_key;
        let kernels = shared_sinc_kernels();
        let pcm: Vec<f32> = (0..64).map(|i| (i as f32 * 0.9).sin()).collect();
        let sound = SynthSampleSound::new_synthetic("K", 60, &pcm, 48_000.0, None, 0);
        let play = |idx: Option<usize>| {
            let (mut pos, mut active) = (10.37f64, true);
            SynthLayer::render_sample_zone_stereo(
                &kernels,
                &sound.zones[0],
                &mut pos,
                1.0,
                &mut active,
                idx,
            )
            .0
        };
        // Ratio 1.0 selects kernel 2 (fc 1.00); key 60 maps to kernel 2 as well.
        assert_eq!(kernel_for_key(60), SincKernelSet::select(1.0));
        assert_eq!(play(None), play(Some(kernel_for_key(60))));
        // Key 0 (kernel 0, fc 1.10) and key 127 (kernel 5, fc 0.895) render differently.
        assert_ne!(
            play(Some(kernel_for_key(0))),
            play(Some(kernel_for_key(127)))
        );
    }

    #[test]
    fn test_sample_mode_zone_playback_loop_crossfade_and_timbre() {
        let fs = 48_000.0;
        let len = 512;
        let pcm: Vec<f32> = (0..len)
            .map(|i| {
                let ph = (i as f32) / 32.0;
                (TAU * ph).sin() + 0.4 * (3.0 * TAU * ph).sin()
            })
            .collect();
        let sound = Arc::new(SynthSampleSound::new_synthetic(
            "TestHarmonicSample",
            60,
            &pcm,
            fs,
            Some((128, 384)),
            16,
        ));

        let mut s_soft = SynthLayer::new(fs);
        s_soft.osc_mode = SynthOscMode::Sample;
        s_soft.sample_timbre = SampleTimbre::Soft;
        s_soft.cutoff_hz = 16_000.0;
        s_soft.load_sample(Some(sound.clone()));
        assert_eq!(s_soft.loaded_sample_name(), Some("TestHarmonicSample"));
        s_soft.note_on(72, 110); // +12 semitones => 2.0x playback speed

        let mut s_bright = SynthLayer::new(fs);
        s_bright.osc_mode = SynthOscMode::Sample;
        s_bright.sample_timbre = SampleTimbre::Bright;
        s_bright.cutoff_hz = 16_000.0;
        s_bright.load_sample(Some(sound));
        s_bright.note_on(72, 110);

        assert!((s_soft.voices[0].sample_step - 2.0).abs() < 1e-4);

        let mut soft_energy = 0.0f32;
        let mut bright_energy = 0.0f32;
        for _ in 0..1024 {
            let [l_s, _] = s_soft.render_frame();
            let [l_b, _] = s_bright.render_frame();
            assert!(l_s.is_finite() && l_b.is_finite());
            soft_energy += l_s * l_s;
            bright_energy += l_b * l_b;
        }
        // Looped 1024 frames @2x speed crossed the loop boundary multiple times without stopping
        assert!(s_soft.voices[0].active);
        assert!(soft_energy > 0.02);
        assert!(
            bright_energy > soft_energy,
            "Bright timbre must retain more HF energy than Soft damping"
        );
    }

    #[test]
    fn test_from_nsmp4_bytes_loads_and_renders_stereo_looped_sample() {
        // Build a synthetic .nsmp4 container with one stereo \0stk chunk (order-1 LPC).
        let mut data = vec![0u8; 48];
        data[0..4].copy_from_slice(b"CBIN");
        data[4..8].copy_from_slice(&1u32.to_le_bytes());
        data[8..12].copy_from_slice(b"nsmp");
        data[0x2c..0x30].copy_from_slice(b"NSMP");
        data.extend_from_slice(&[0u8; 12]);

        let mut hdr = vec![0u8; 64];
        let nm = b"CleanRoom Violin\0";
        hdr[10..10 + nm.len()].copy_from_slice(nm);
        data.extend_from_slice(b"\0hdr");
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&(hdr.len() as u32).to_be_bytes());
        data.extend_from_slice(&hdr);

        let mut map = vec![0u8; 0x524 + 16];
        let p = 0x524;
        map[p + 1] = 127;
        map[p + 2] = 60;
        map[p + 3] = 72;
        map[p + 4] = 48;
        map[p + 10..p + 14].copy_from_slice(&1u32.to_be_bytes());
        map[p + 15] = 1;
        data.extend_from_slice(b"\0map");
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&(map.len() as u32).to_be_bytes());
        data.extend_from_slice(&map);

        // Construct sustained stereo order-0 frame: L positive (+1000), R negative (-500), 8 frames each
        let mut stk = vec![0u8; 0x44];
        stk[0..4].copy_from_slice(&1u32.to_be_bytes());
        stk[0x05] = 60;
        stk[0x06..0x08].copy_from_slice(&35_002u16.to_be_bytes());
        stk[0x08] = 2; // stereo
        let w1: u32 = 100;
        let w3: u32 = 100 + 1 + 4; // loop starts at frame 2 (1 ctrl + 2*2 interleaved words)
        stk[0x12..0x16].copy_from_slice(&w1.to_be_bytes());
        stk[0x24..0x28].copy_from_slice(&w3.to_be_bytes());
        stk[0x39..0x3D].copy_from_slice(&0.0f32.to_be_bytes());

        // Frame 1: spc=4 (ns=8), order=0, nbits=16 => wpc=2 -> 2 L words + 2 R words interleaved
        let ctrl1: u32 = 8 | (15 << 19);
        stk.extend_from_slice(&ctrl1.to_be_bytes());
        for _ in 0..2 {
            stk.extend_from_slice(&((1000u32 << 16) | 1000u32).to_be_bytes());
            stk.extend_from_slice(
                &((((-500i16) as u16) as u32) << 16 | ((-500i16) as u16) as u32).to_be_bytes(),
            );
        }
        // Frame 2 (loop boundary at w3): spc=4, order=0, nbits=16
        let ctrl2: u32 = 8 | (1 << 18) | (15 << 19);
        stk.extend_from_slice(&ctrl2.to_be_bytes());
        for _ in 0..2 {
            stk.extend_from_slice(&((1200u32 << 16) | 1200u32).to_be_bytes());
            stk.extend_from_slice(
                &((((-600i16) as u16) as u32) << 16 | ((-600i16) as u16) as u32).to_be_bytes(),
            );
        }
        stk.extend_from_slice(&0x0080_0040u32.to_be_bytes());

        data.extend_from_slice(b"\0stk");
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&(stk.len() as u32).to_be_bytes());
        data.extend_from_slice(&stk);

        let sound = Arc::new(SynthSampleSound::from_nsmp4_bytes(&data).expect("load nsmp4"));
        assert_eq!(sound.name, "CleanRoom Violin");
        assert_eq!(sound.zones.len(), 1);
        assert_eq!(sound.zones[0].loop_start, Some(4));
        assert_eq!(sound.zones[0].loop_end, Some(8));
        assert!(sound.zones[0].pcm_right.is_some());

        let mut layer = SynthLayer::legacy(48_000.0);
        layer.osc_mode = SynthOscMode::Sample;
        layer.cutoff_hz = 18_000.0;
        layer.load_sample(Some(sound));
        layer.note_on(60, 127);

        let [l, r] = layer.render_frame();
        assert!(l.is_finite() && r.is_finite());
        assert!(l > 0.0 && r < 0.0, "stereo polarity L={l} R={r}");
    }
}

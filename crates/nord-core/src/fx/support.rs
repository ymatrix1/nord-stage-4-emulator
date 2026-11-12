//! Shared test helpers for the FX tests.

use super::*;

pub(super) fn tone_gain_db(fx: &mut LayerFxChain, hz: f32) -> f32 {
    let fs = fx.sample_rate;
    let (mut e_in, mut e_out) = (0.0f64, 0.0f64);
    for i in 0..24_000 {
        let x = (TAU * hz * i as f32 / fs).sin() * 0.1;
        let [l, _] = fx.process_frame([x, x]);
        if i >= 12_000 {
            e_in += f64::from(x * x);
            e_out += f64::from(l * l);
        }
    }
    (10.0 * (e_out / e_in).log10()) as f32
}

pub(super) fn eq_only(bass: f32, mid: f32, mid_hz: f32, treble: f32) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.eq_bass_db = bass;
    fx.eq_mid_db = mid;
    fx.eq_mid_freq_hz = mid_hz;
    fx.eq_treble_db = treble;
    fx
}

pub(super) fn filter_chain(mode: AmpCabinet, hz: f32, res: f32) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.amp_cabinet = mode;
    fx.eq_mid_freq_hz = hz;
    fx.eq_filter_res = res;
    fx
}

pub(super) fn corr_after_delay_fx(
    fx_kind: DelayFeedbackFx,
    var: bool,
    mono_in: bool,
) -> (f32, f32, f32) {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.delay_enabled = true;
    fx.delay_time_ms = 30.0;
    fx.delay_feedback = 0.0;
    fx.delay_mix = 1.0;
    fx.delay_fx = fx_kind;
    fx.delay_fx_variation = var;
    let (mut ll, mut rr, mut lr) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..48_000 {
        let t = i as f32 / 48_000.0;
        let a = 0.3 * (TAU * 440.0 * t).sin() + 0.2 * (TAU * 1250.0 * t).sin();
        let b = if mono_in {
            a
        } else {
            0.3 * (TAU * 330.0 * t).sin() + 0.2 * (TAU * 900.0 * t).sin()
        };
        let [l, r] = fx.process_frame([a, b]);
        assert!(l.is_finite() && r.is_finite());
        if i > 12_000 {
            let (l, r) = (f64::from(l), f64::from(r));
            ll += l * l;
            rr += r * r;
            lr += l * r;
        }
    }
    let corr = (lr / (ll.sqrt() * rr.sqrt()).max(1e-12)) as f32;
    (corr, ll as f32, rr as f32)
}

pub(super) fn cab_chain(mode: AmpCabinet, var: bool) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.amp_cabinet = mode;
    fx.amp_variation = var;
    fx
}

/// Level at 6 kHz relative to 500 Hz (dB): the treble character of a cabinet.
pub(super) fn treble_tilt_db(mode: AmpCabinet, var: bool) -> f32 {
    let hi = tone_gain_db(&mut cab_chain(mode, var), 6000.0);
    let lo = tone_gain_db(&mut cab_chain(mode, var), 500.0);
    hi - lo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_only_helper_flattens_everything_but_the_requested_bands() {
        let mut fx = eq_only(0.0, 0.0, 1000.0, 0.0);
        assert!(tone_gain_db(&mut fx, 1000.0).abs() < 1.0);
    }
}

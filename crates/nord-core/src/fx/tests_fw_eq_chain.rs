//! `FirmwareFx::eq` and `FirmwareFx::compressor` on `LayerFxChain`: the firmware EQ replaces the
//! three engine biquads, 2 frame latency, default unchanged; the firmware compressor ignores
//! `comp_fast` (`kCmpResponse` has no DSP effect, `docs/research/fx-reverb-leslie.md` s4).

use super::*;

const EQ_ONLY: FirmwareFx = FirmwareFx {
    eq: true,
    ..FirmwareFx::OFF
};
const CMP_ONLY: FirmwareFx = FirmwareFx {
    compressor: true,
    ..FirmwareFx::OFF
};

fn chain(fw: FirmwareFx) -> LayerFxChain {
    let mut fx = LayerFxChain::new(48_000.0);
    fx.reverb_model = ReverbModel::Off;
    fx.amp_cabinet = AmpCabinet::Off;
    fx.firmware_fx = fw;
    fx
}

fn rms_db(fx: &mut LayerFxChain, cycles: f32) -> f32 {
    let n = 16_000;
    let (mut i2, mut o2) = (0.0f64, 0.0f64);
    for k in 0..n {
        let x = 0.1 * (std::f32::consts::TAU * cycles * k as f32).sin();
        let y = fx.process_frame([x, x])[0];
        if k >= n / 2 {
            i2 += f64::from(x * x);
            o2 += f64::from(y * y);
        }
    }
    (10.0 * (o2 / i2).log10()) as f32
}

fn render(fx: &mut LayerFxChain, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let x = (i as f32 * 0.043).sin() * 0.3;
            fx.process_frame([x, -x])
        })
        .collect()
}

#[test]
fn off_is_unchanged_and_on_replaces_the_engine_eq() {
    let mut off = chain(FirmwareFx::OFF);
    off.eq_treble_db = 6.0;
    off.eq_bass_db = -4.0;
    let mut on = chain(EQ_ONLY);
    on.eq_treble_db = 6.0;
    on.eq_bass_db = -4.0;
    let (a, b) = (render(&mut off, 800), render(&mut on, 800));
    assert_ne!(a, b);
    assert!(b.iter().flatten().all(|v| v.is_finite()));
    assert!(on.fw_eq.is_some() && off.fw_eq.is_none());
    assert_eq!(b[0], [0.0, 0.0], "latency is one 2 frame block");
}

#[test]
fn neutral_eq_is_a_two_frame_delay_of_the_engine_path() {
    let mut off = chain(FirmwareFx::OFF);
    let mut on = chain(EQ_ONLY);
    let (a, b) = (render(&mut off, 400), render(&mut on, 400));
    for k in 2..400 {
        assert!((b[k][0] - a[k - 2][0]).abs() < 1e-5, "frame {k}");
    }
}

#[test]
fn treble_follows_the_firmware_shelf_gain_not_the_engine_biquad() {
    let mut fw = chain(EQ_ONLY);
    fw.eq_treble_db = 15.0;
    // 60 counts: the firmware boost word gives 1 + F = 5.474 (14.77 dB) at Nyquist
    let g = rms_db(&mut fw, 0.5 - 1e-3);
    assert!((g - 14.77).abs() < 0.2, "{g}");
}

#[test]
fn lp_hp_filter_amps_leave_the_mid_band_to_the_filter() {
    let mut a = chain(EQ_ONLY);
    a.amp_cabinet = AmpCabinet::LpFilter;
    a.eq_mid_db = 10.0;
    let mut b = chain(EQ_ONLY);
    b.amp_cabinet = AmpCabinet::LpFilter;
    b.eq_mid_db = 0.0;
    let (x, y) = (render(&mut a, 300), render(&mut b, 300));
    assert_eq!(x, y, "the Mid knob is the resonance of LP24 / HP24");
}

#[test]
fn switching_the_flag_off_drops_the_stage() {
    let mut fx = chain(EQ_ONLY);
    let _ = render(&mut fx, 10);
    assert!(fx.fw_eq.is_some());
    fx.firmware_fx = FirmwareFx::OFF;
    let _ = render(&mut fx, 10);
    assert!(fx.fw_eq.is_none());
}

#[test]
fn firmware_compressor_ignores_the_response_flag() {
    let run = |fast: bool| {
        let mut fx = chain(CMP_ONLY);
        fx.comp_enabled = true;
        fx.comp_amount = 0.8;
        fx.comp_fast = fast;
        render(&mut fx, 3000)
    };
    assert_eq!(
        run(false),
        run(true),
        "kCmpResponse writes no value dependent word"
    );
}

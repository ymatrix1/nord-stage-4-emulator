//! The opt-in firmware Leslie in the organ layer (`--leslie firmware`).

use super::OrganLayer;
use crate::engine::Stage4Engine;
use crate::fx::FwLeslie;
use crate::render_opts::{apply_leslie_firmware, FirmwareOptions};

const FS: f32 = 48_000.0;

fn rms(v: &[[f32; 2]]) -> f32 {
    (v.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum::<f32>() / (2.0 * v.len() as f32)).sqrt()
}

fn run(o: &mut OrganLayer, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|k| {
            let x = 0.1 * (std::f32::consts::TAU * 440.0 * k as f32 / FS).sin();
            o.process_rotary(x, 1.0 / FS)
        })
        .collect()
}

#[test]
fn the_option_parses_and_applies_to_both_layers() {
    let o = FirmwareOptions::from_args(&["--leslie".into(), "firmware".into()]).unwrap();
    assert!(o.leslie_firmware && o.leslie_gain_mdb == 0);
    let g = FirmwareOptions::from_args(&["--leslie-gain-db".into(), "40.5".into()]).unwrap();
    assert_eq!(g.leslie_gain_mdb, 40_500);
    assert!(FirmwareOptions::from_args(&["--leslie".into(), "x".into()]).is_err());
    assert!(
        !FirmwareOptions::default().leslie_firmware,
        "off by default"
    );
    let mut e = Stage4Engine::new(FS);
    apply_leslie_firmware(&mut e, false, 0.0);
    assert!(e.organ_a.leslie_fw.is_none());
    apply_leslie_firmware(&mut e, true, 20.0);
    assert!(e.organ_a.leslie_fw.is_some() && e.organ_b.leslie_fw.is_some());
    assert!((e.organ_b.leslie_gain - 10.0).abs() < 1e-5);
}

#[test]
fn the_firmware_leslie_replaces_the_engine_rotary_at_its_booted_level() {
    let mut engine = OrganLayer::new(FS);
    let mut fw = OrganLayer::new(FS);
    fw.leslie_fw = Some(Box::new(FwLeslie::new()));
    let (a, b) = (run(&mut engine, 4000), run(&mut fw, 4000));
    assert!(b.iter().flatten().all(|v| v.is_finite()));
    let (ra, rb) = (rms(&a[2000..]), rms(&b[2000..]));
    assert!(ra > 1e-3, "the engine rotary passes the tone");
    let db = 20.0 * (rb / 0.0707).log10();
    assert!(
        (-70.0..-35.0).contains(&db),
        "booted level about -50 dB, got {db}"
    );
}

#[test]
fn stop_mode_freezes_the_firmware_rotor() {
    let mut o = OrganLayer::new(FS);
    o.leslie_fw = Some(Box::new(FwLeslie::new()));
    o.rotary_stop_mode = true;
    o.horn_rate_hz = 0.0;
    o.drum_rate_hz = 0.0;
    let out = run(&mut o, 6000);
    let env = |s: &[[f32; 2]]| rms(s);
    let (first, last) = (env(&out[1000..3000]), env(&out[4000..6000]));
    assert!(
        (first - last).abs() < 0.05 * first.max(1e-9),
        "no modulation: {first} vs {last}"
    );
}

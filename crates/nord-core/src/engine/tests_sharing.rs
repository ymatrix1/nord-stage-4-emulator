//! Engine tests for `TO ROTARY` routing and the shared GLOBAL reverb.

use super::*;
use crate::fx::{Mod1Type, ReverbModel};

fn render(e: &mut Stage4Engine, frames: usize) -> Vec<[f32; 2]> {
    (0..frames).map(|_| e.render_frame()).collect()
}

fn synth_engine() -> Stage4Engine {
    let mut e = Stage4Engine::new(48_000.0);
    for fx in &mut e.fx_chains {
        fx.reverb_model = ReverbModel::Off;
    }
    e.synth_a.note_on(60, 100);
    e
}

#[test]
fn to_rotary_routes_a_synth_layer_through_the_rotary_with_stereo_motion() {
    let mut plain = synth_engine();
    let mut routed = synth_engine();
    routed.fx_chains[4].amp_cabinet = AmpCabinet::ToRotary;
    routed.organ_a.rotary_speed = RotarySpeed::Fast;
    let (a, b) = (render(&mut plain, 24_000), render(&mut routed, 24_000));
    assert!(b.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    assert!(
        b.iter().any(|f| f[0].abs() > 1e-4),
        "routed layer is silent"
    );
    assert!(a != b, "TO ROTARY has no effect");
    // plain synth is dual mono; the rotary adds L/R differences
    let plain_diff: f32 = a.iter().map(|f| (f[0] - f[1]).abs()).sum();
    let rot_diff: f32 = b.iter().map(|f| (f[0] - f[1]).abs()).sum();
    assert!(rot_diff > plain_diff + 0.1, "{rot_diff} vs {plain_diff}");
}

#[test]
fn to_rotary_is_ignored_on_organ_layers_and_when_unused_nothing_changes() {
    let mut base = synth_engine();
    let mut organ_flag = synth_engine();
    organ_flag.fx_chains[0].amp_cabinet = AmpCabinet::ToRotary;
    organ_flag.fx_chains[1].amp_cabinet = AmpCabinet::ToRotary;
    assert_eq!(render(&mut base, 4096), render(&mut organ_flag, 4096));
}

#[test]
fn rotary_speed_of_the_organ_section_controls_the_routed_layer() {
    let run = |speed: RotarySpeed| {
        let mut e = synth_engine();
        e.fx_chains[4].amp_cabinet = AmpCabinet::ToRotary;
        e.organ_a.rotary_speed = speed;
        render(&mut e, 24_000)
    };
    assert!(run(RotarySpeed::Slow) != run(RotarySpeed::Fast));
}

#[test]
fn global_reverb_moves_the_reverb_after_the_mix_and_fader() {
    let build = |global: bool| {
        let mut e = synth_engine();
        e.fx_chains[4].reverb_model = ReverbModel::Hall;
        e.fx_chains[4].reverb_mix = 0.5;
        if global {
            e.fx_sharing.reverb_global = Some(4);
            e.apply_fx_sharing();
        }
        e
    };
    let (mut local, mut global) = (build(false), build(true));
    let (a, b) = (render(&mut local, 24_000), render(&mut global, 24_000));
    assert!(b.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
    assert!(a != b, "GLOBAL reverb renders like the layer reverb");
    // the shared reverb also processes layers that have no reverb of their own
    let mut other = Stage4Engine::new(48_000.0);
    for fx in &mut other.fx_chains {
        fx.reverb_model = ReverbModel::Off;
    }
    other.fx_chains[4].reverb_model = ReverbModel::Hall;
    other.fx_chains[4].reverb_mix = 0.5;
    other.fx_sharing.reverb_global = Some(4);
    other.apply_fx_sharing();
    other.synth_b.note_on(64, 100);
    let tail: f32 = render(&mut other, 24_000).iter().map(|f| f[0].abs()).sum();
    assert!(tail > 0.0);
}

#[test]
fn global_reverb_mix_follows_the_source_layer_and_fader_scales_the_wet_signal() {
    let wet = |gain: f32| {
        let mut e = synth_engine();
        e.fx_chains[4].reverb_model = ReverbModel::Hall;
        e.fx_chains[4].reverb_mix = 0.5;
        e.fx_sharing.reverb_global = Some(4);
        e.apply_fx_sharing();
        e.layer_levels[4] = crate::morph::MorphParameter::new(gain);
        let out = render(&mut e, 12_000);
        out.iter().map(|f| f[0].abs()).sum::<f32>()
    };
    assert!(wet(0.2) < wet(0.8), "reverb is not after the level fader");
}

#[test]
fn group_and_global_do_not_disturb_an_unshared_engine() {
    let mut a = synth_engine();
    let mut b = synth_engine();
    a.fx_chains[4].mod1_type = Mod1Type::Tremolo;
    b.fx_chains[4].mod1_type = Mod1Type::Tremolo;
    let mut l = vec![0.0; 512];
    let mut r = vec![0.0; 512];
    a.process_block(&mut l, &mut r);
    let direct: Vec<_> = render(&mut b, 512);
    assert!(l.iter().zip(&direct).all(|(x, d)| *x == d[0]));
}

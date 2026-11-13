//! Opt-in firmware paths for `ns4 render-patch` (`--tone-source`, `--filter-engine`,
//! `--firmware-fx`). Defaults leave the engine untouched, so default renders stay bit-identical.

use crate::engine::Stage4Engine;
use crate::fx::FirmwareFx;
use crate::organ::ToneWheelSource;
use crate::synth::{FilterEngine, SynthChain};

/// Parses a `--tone-source` value: `default | legacy | firmware | vox | b3wheels`.
pub fn parse_tone_source(v: &str) -> Result<ToneWheelSource, String> {
    match v.to_ascii_lowercase().as_str() {
        "default" | "model" => Ok(ToneWheelSource::ModelDefault),
        "legacy" => Ok(ToneWheelSource::Legacy),
        "firmware" | "pipe" => Ok(ToneWheelSource::Firmware),
        "vox" => Ok(ToneWheelSource::Vox),
        "b3wheels" | "b3" => Ok(ToneWheelSource::B3Wheels),
        _ => Err(format!(
            "invalid --tone-source '{v}' (expected default|legacy|firmware|vox|b3wheels)"
        )),
    }
}

/// Parses a `--filter-engine` value: `zdf | firmware`.
pub fn parse_filter_engine(v: &str) -> Result<FilterEngine, String> {
    match v.to_ascii_lowercase().as_str() {
        "zdf" | "default" => Ok(FilterEngine::Zdf),
        "firmware" => Ok(FilterEngine::Firmware),
        _ => Err(format!(
            "invalid --filter-engine '{v}' (expected zdf|firmware)"
        )),
    }
}

/// Parses a `--synth-chain` value: `legacy | strip`.
pub fn parse_synth_chain(v: &str) -> Result<SynthChain, String> {
    match v.to_ascii_lowercase().as_str() {
        "legacy" | "default" => Ok(SynthChain::Legacy),
        "strip" | "firmware" => Ok(SynthChain::FirmwareStrip),
        _ => Err(format!(
            "invalid --synth-chain '{v}' (expected legacy|strip)"
        )),
    }
}

/// Selects the per-voice chain of every synth layer (`None` leaves the engine untouched).
pub fn apply_synth_chain(engine: &mut Stage4Engine, chain: Option<SynthChain>) {
    if let Some(c) = chain {
        engine.synth_a.chain = c;
        engine.synth_b.chain = c;
        engine.synth_c.chain = c;
    }
}

/// Sets the firmware level law of both piano layers (`PianoLayer::firmware_velocity_level`,
/// default on); `ns4 render-patch --legacy-piano-level` turns it off.
pub fn apply_piano_level(engine: &mut Stage4Engine, on: bool) {
    engine.piano_a.firmware_velocity_level = on;
    engine.piano_b.firmware_velocity_level = on;
}

/// Parses an `--b3-tail` value: `on | off`.
pub fn parse_on_off(name: &str, v: &str) -> Result<bool, String> {
    match v.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        _ => Err(format!("invalid {name} '{v}' (expected on|off)")),
    }
}

/// Selects the B3 processor-tail response of both organ layers' `B3Wheels` source (`None` leaves
/// the engine untouched).
pub fn apply_b3_tail(engine: &mut Stage4Engine, tail: Option<bool>) {
    if let Some(on) = tail {
        engine.organ_a.set_b3_tail(on);
        engine.organ_b.set_b3_tail(on);
    }
}

/// Applies the opt-in firmware switches to every layer of `engine`.
pub fn apply_firmware_options(
    engine: &mut Stage4Engine,
    tone: Option<ToneWheelSource>,
    filter: Option<FilterEngine>,
    firmware_fx: bool,
) {
    if let Some(t) = tone {
        engine.organ_a.tonewheel_source = t;
        engine.organ_b.tonewheel_source = t;
    }
    if let Some(f) = filter {
        engine.synth_a.filter_engine = f;
        engine.synth_b.filter_engine = f;
        engine.synth_c.filter_engine = f;
    }
    if firmware_fx {
        for chain in engine.fx_chains.iter_mut() {
            chain.firmware_fx = FirmwareFx::ALL;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b3_tail_switch_parses_and_changes_the_b3_wheels_render() {
        assert_eq!(parse_on_off("--b3-tail", "Off"), Ok(false));
        assert_eq!(parse_on_off("--b3-tail", "on"), Ok(true));
        assert!(parse_on_off("--b3-tail", "x").is_err());
        let render = |tail: Option<bool>| {
            let mut e = Stage4Engine::new(48_000.0);
            apply_firmware_options(&mut e, Some(ToneWheelSource::B3Wheels), None, false);
            apply_b3_tail(&mut e, tail);
            e.organ_a.note_on(60);
            e.organ_a.rotary_enabled = false;
            (0..4_000)
                .map(|_| e.organ_a.render_frame()[0])
                .collect::<Vec<f32>>()
        };
        let (on, off) = (render(None), render(Some(false)));
        assert_eq!(on, render(Some(true)));
        assert!(on.iter().zip(&off).any(|(a, b)| (a - b).abs() > 1e-4));
    }

    #[test]
    fn parses_values_and_rejects_unknown() {
        assert_eq!(parse_tone_source("Vox"), Ok(ToneWheelSource::Vox));
        assert_eq!(parse_tone_source("b3wheels"), Ok(ToneWheelSource::B3Wheels));
        assert_eq!(parse_tone_source("pipe"), Ok(ToneWheelSource::Firmware));
        assert!(parse_tone_source("x").is_err());
        assert_eq!(parse_filter_engine("firmware"), Ok(FilterEngine::Firmware));
        assert!(parse_filter_engine("x").is_err());
    }

    #[test]
    fn synth_chain_parses_and_applies_to_every_layer() {
        assert_eq!(parse_synth_chain("strip"), Ok(SynthChain::FirmwareStrip));
        assert_eq!(parse_synth_chain("Legacy"), Ok(SynthChain::Legacy));
        assert!(parse_synth_chain("x").is_err());
        let mut e = Stage4Engine::new(48_000.0);
        apply_synth_chain(&mut e, None);
        assert_eq!(e.synth_b.chain, SynthChain::Legacy);
        apply_synth_chain(&mut e, Some(SynthChain::FirmwareStrip));
        assert!(e.synth_a.strip_chain() && e.synth_b.strip_chain() && e.synth_c.strip_chain());
    }

    #[test]
    fn piano_level_switch_reaches_both_layers() {
        let mut e = Stage4Engine::new(48_000.0);
        assert!(e.piano_a.firmware_velocity_level && e.piano_b.firmware_velocity_level);
        apply_piano_level(&mut e, false);
        assert!(!e.piano_a.firmware_velocity_level && !e.piano_b.firmware_velocity_level);
        apply_piano_level(&mut e, true);
        assert!(e.piano_a.firmware_velocity_level && e.piano_b.firmware_velocity_level);
    }

    #[test]
    fn apply_sets_all_layers_and_none_is_noop() {
        let mut e = Stage4Engine::new(48_000.0);
        apply_firmware_options(&mut e, None, None, false);
        assert_eq!(e.organ_a.tonewheel_source, ToneWheelSource::ModelDefault);
        assert_eq!(e.synth_c.filter_engine, FilterEngine::Zdf);
        assert_eq!(e.fx_chains[0].firmware_fx, FirmwareFx::OFF);
        apply_firmware_options(
            &mut e,
            Some(ToneWheelSource::Vox),
            Some(FilterEngine::Firmware),
            true,
        );
        assert_eq!(e.organ_b.tonewheel_source, ToneWheelSource::Vox);
        assert_eq!(e.synth_b.filter_engine, FilterEngine::Firmware);
        assert_eq!(e.fx_chains[6].firmware_fx, FirmwareFx::ALL);
    }
}

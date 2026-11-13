//! Opt-in firmware paths for `ns4 render-patch` (`--tone-source`, `--filter-engine`,
//! `--firmware-fx`). Defaults leave the engine untouched, so default renders stay bit-identical.

use crate::engine::Stage4Engine;
use crate::fx::FirmwareFx;
use crate::organ::ToneWheelSource;
use crate::synth::FilterEngine;

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
    fn parses_values_and_rejects_unknown() {
        assert_eq!(parse_tone_source("Vox"), Ok(ToneWheelSource::Vox));
        assert_eq!(parse_tone_source("b3wheels"), Ok(ToneWheelSource::B3Wheels));
        assert_eq!(parse_tone_source("pipe"), Ok(ToneWheelSource::Firmware));
        assert!(parse_tone_source("x").is_err());
        assert_eq!(parse_filter_engine("firmware"), Ok(FilterEngine::Firmware));
        assert!(parse_filter_engine("x").is_err());
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

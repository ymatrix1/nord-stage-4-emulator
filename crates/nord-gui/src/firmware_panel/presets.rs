//! Presets of the firmware bar: the whole-engine sets and the per-engine Classic / Firmware
//! buttons. A per-engine preset copies only that engine's fields from the whole-engine preset,
//! so the other engines keep what the user picked.

use nord_core::render_opts::FirmwareOptions;

/// Engine group with its own Classic / Firmware preset buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Tone source, B3 / Farfisa stages, rotary, Leslie, level.
    Organ,
    /// Level law, bus scale, layer law.
    Piano,
    /// Chain, stages, P table, filter, measured laws, shape, sample kernel / pitch.
    Synth,
    /// Amp path and every firmware FX port.
    Fx,
}

/// Pre-firmware engine: `--legacy-all` plus the engine amp path.
pub fn classic() -> FirmwareOptions {
    FirmwareOptions {
        amp_path: Some(false),
        ..FirmwareOptions::legacy_all()
    }
}

/// Every verified firmware path (`--firmware-all`).
pub fn firmware() -> FirmwareOptions {
    FirmwareOptions::firmware_all()
}

/// Copies the fields of `engine` from `src` into `dst`.
pub fn copy_engine(dst: &mut FirmwareOptions, src: &FirmwareOptions, engine: Engine) {
    match engine {
        Engine::Organ => {
            dst.tone_source = src.tone_source;
            dst.b3_tail = src.b3_tail;
            dst.farfisa_processor = src.farfisa_processor;
            dst.b3_bass_processor = src.b3_bass_processor;
            dst.rotary_drive = src.rotary_drive;
            dst.organ_level = src.organ_level;
            dst.leslie_firmware = src.leslie_firmware;
            dst.leslie_gain_mdb = src.leslie_gain_mdb;
            dst.leslie_ring = src.leslie_ring;
        }
        Engine::Piano => {
            dst.piano_firmware_level = src.piano_firmware_level;
            dst.piano_bus_scale = src.piano_bus_scale;
            dst.piano_chain_scale = src.piano_chain_scale;
            dst.piano_layer_law = src.piano_layer_law;
        }
        Engine::Synth => {
            dst.synth_chain = src.synth_chain;
            dst.synth_stages = src.synth_stages;
            dst.synth_knob_laws = src.synth_knob_laws;
            dst.synth_p = src.synth_p;
            dst.filter_engine = src.filter_engine;
            dst.measured_laws = src.measured_laws;
            dst.synth_shape_milli = src.synth_shape_milli;
            dst.sample_kernel_key = src.sample_kernel_key;
            dst.sample_pitch_fw = src.sample_pitch_fw;
        }
        Engine::Fx => {
            dst.fx = src.fx;
            dst.amp_path = src.amp_path;
        }
    }
}

/// Applies the Classic (`firmware == false`) or Firmware preset to one engine.
pub fn apply_engine(o: &mut FirmwareOptions, engine: Engine, firmware_set: bool) {
    let src = if firmware_set { firmware() } else { classic() };
    copy_engine(o, &src, engine);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINES: [Engine; 4] = [Engine::Organ, Engine::Piano, Engine::Synth, Engine::Fx];

    #[test]
    fn engine_presets_compose_to_the_whole_presets() {
        for (fw, whole) in [(true, firmware()), (false, classic())] {
            let mut o = if fw { classic() } else { firmware() };
            o.firmware_routing = whole.firmware_routing;
            for e in ENGINES {
                apply_engine(&mut o, e, fw);
            }
            assert_eq!(o, whole, "firmware={fw}");
        }
    }

    #[test]
    fn an_engine_preset_leaves_the_other_engines_alone() {
        let mut o = FirmwareOptions {
            leslie_firmware: true,
            piano_layer_law: true,
            sample_pitch_fw: true,
            ..classic()
        };
        apply_engine(&mut o, Engine::Fx, true);
        assert!(o.fx == FirmwareOptions::firmware_all().fx && o.amp_path.is_none());
        assert!(o.leslie_firmware && o.piano_layer_law && o.sample_pitch_fw);
        apply_engine(&mut o, Engine::Synth, false);
        assert!(!o.sample_pitch_fw && o.leslie_firmware, "synth reset only");
        apply_engine(&mut o, Engine::Organ, false);
        assert!(!o.leslie_firmware && o.piano_layer_law);
        apply_engine(&mut o, Engine::Piano, false);
        assert!(!o.piano_layer_law);
    }
}

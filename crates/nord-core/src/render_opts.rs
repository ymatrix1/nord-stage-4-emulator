//! Opt-in firmware paths shared by `ns4 render-patch` and the GUI (`--tone-source`,
//! `--filter-engine`, `--synth-chain`, `--firmware-fx`, `--measured-laws`, ...). Defaults leave the engine untouched, so default renders stay bit-identical.

use crate::engine::{FirmwareRouting, Stage4Engine};
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
        "farfisa" | "farfisawheels" => Ok(ToneWheelSource::FarfisaWheels),
        _ => Err(format!(
            "invalid --tone-source '{v}' (expected default|legacy|firmware|vox|b3wheels|farfisa)"
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

/// Selects the amp path of every layer (`Some(true)` firmware drive stage and cabinet, the
/// default; `Some(false)` the engine drive and RBJ cabinet; `None` leaves the engine untouched).
pub fn apply_amp_path(engine: &mut Stage4Engine, firmware: Option<bool>) {
    if let Some(on) = firmware {
        for chain in engine.fx_chains.iter_mut() {
            chain.firmware_fx.amp = on;
        }
    }
}

/// Selects the unity-gain rotary drive law `tanh(g x) / g` of both organ layers (`false` keeps the
/// engine law).
pub fn apply_rotary_unity(engine: &mut Stage4Engine, on: bool) {
    if on {
        engine.organ_a.rotary_drive_unity = true;
        engine.organ_b.rotary_drive_unity = true;
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

/// Every opt-in firmware switch in one `Copy` value shared by `ns4 render-patch` and the GUI.
///
/// `None` / `false` leave the engine default untouched. [`apply_to`](Self::apply_to) only ever
/// switches paths on (render-patch semantics); [`apply_exact`](Self::apply_exact) also switches
/// them back to the default so a running engine can be A/B toggled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareOptions {
    /// `--tone-source` (`None` = `ModelDefault`).
    pub tone_source: Option<ToneWheelSource>,
    /// `--filter-engine` (`None` = `Zdf`).
    pub filter_engine: Option<FilterEngine>,
    /// `--synth-chain` (`None` = `Legacy`).
    pub synth_chain: Option<SynthChain>,
    /// `--b3-tail on|off` (`None` = the `B3Wheels` default, tail on).
    pub b3_tail: Option<bool>,
    /// `--firmware-fx`: every `FirmwareFx` port on every FX chain.
    pub firmware_fx: bool,
    /// `--firmware-piano-level`: `PianoLayer::firmware_velocity_level`.
    pub piano_firmware_level: bool,
    /// `--firmware-piano-bus`: `PianoLayer::firmware_bus_scale` (opt-in, ASSUMED receiver scale).
    pub piano_bus_scale: bool,
    /// `--measured-laws`: `SynthLayer::measured_laws` (EMPIRICAL, Audible Genius clips).
    pub measured_laws: bool,
    /// `--synth-shape <0..1>`: strip shape word of the SHAPE oscillator (`fw_shape.rs`) in
    /// thousandths (`None` = the ASSUMED saw placeholder).
    pub synth_shape_milli: Option<u16>,
    /// `--firmware-routing`: section / layer enables and per-layer KB zones as the program stores
    /// them ([`FirmwareRouting`]); stays out of `firmware_all` until the zone codes are verified.
    pub firmware_routing: bool,
}

/// The engine defaults: every opt-in path off, except the firmware piano level, which is the
/// default for `.npno` sounds (D23).
impl Default for FirmwareOptions {
    fn default() -> Self {
        Self {
            tone_source: None,
            filter_engine: None,
            synth_chain: None,
            b3_tail: None,
            firmware_fx: false,
            piano_firmware_level: true,
            piano_bus_scale: false,
            measured_laws: false,
            synth_shape_milli: None,
            firmware_routing: false,
        }
    }
}

impl FirmwareOptions {
    /// Every opt-in path off and the legacy piano velocity response (`--legacy-all`).
    pub fn legacy_all() -> Self {
        Self {
            piano_firmware_level: false,
            ..Self::default()
        }
    }

    /// Every verified firmware path on (`--firmware-all`): firmware tone wheels, firmware filter,
    /// strip chain, firmware FX, firmware piano level. `measured_laws` (EMPIRICAL) stays off.
    pub fn firmware_all() -> Self {
        Self {
            tone_source: Some(ToneWheelSource::Firmware),
            filter_engine: Some(FilterEngine::Firmware),
            synth_chain: Some(SynthChain::FirmwareStrip),
            b3_tail: None,
            firmware_fx: true,
            piano_firmware_level: true,
            piano_bus_scale: false,
            measured_laws: false,
            synth_shape_milli: None,
            firmware_routing: false,
        }
    }

    /// Parses the shared flags from a CLI argument list; unknown arguments are ignored so a host
    /// binary can mix in its own flags. Presets (`--firmware-all`, `--legacy-all`) apply first,
    /// then individual flags override them.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let has = |name: &str| args.iter().any(|a| a == name);
        let value = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
        };
        let mut o = Self::default();
        if has("--firmware-all") {
            o = Self::firmware_all();
        }
        if has("--legacy-all") {
            o = Self::legacy_all();
        }
        if let Some(v) = value("--tone-source") {
            o.tone_source = Some(parse_tone_source(v)?);
        }
        if let Some(v) = value("--filter-engine") {
            o.filter_engine = Some(parse_filter_engine(v)?);
        }
        if let Some(v) = value("--synth-chain") {
            o.synth_chain = Some(parse_synth_chain(v)?);
        }
        if let Some(v) = value("--b3-tail") {
            o.b3_tail = Some(parse_on_off("--b3-tail", v)?);
        }
        o.firmware_fx |= has("--firmware-fx");
        o.piano_firmware_level |= has("--firmware-piano-level");
        if has("--legacy-piano-level") {
            o.piano_firmware_level = false;
        }
        o.piano_bus_scale |= has("--firmware-piano-bus");
        o.measured_laws |= has("--measured-laws");
        o.firmware_routing |= has("--firmware-routing");
        if has("--legacy-routing") {
            o.firmware_routing = false;
        }
        if let Some(v) = value("--synth-shape") {
            let x: f32 = v
                .parse()
                .map_err(|_| format!("invalid --synth-shape '{v}'"))?;
            o.synth_shape_milli = Some((x.clamp(0.0, 1.0) * 1000.0).round() as u16);
        }
        Ok(o)
    }

    /// Switches the selected paths on (identical to the individual `apply_*` calls render-patch
    /// used before this struct existed).
    pub fn apply_to(&self, engine: &mut Stage4Engine) {
        apply_firmware_options(
            engine,
            self.tone_source,
            self.filter_engine,
            self.firmware_fx,
        );
        apply_piano_level(engine, self.piano_firmware_level);
        engine.piano_a.firmware_bus_scale = self.piano_bus_scale;
        engine.piano_b.firmware_bus_scale = self.piano_bus_scale;
        apply_b3_tail(engine, self.b3_tail);
        apply_synth_chain(engine, self.synth_chain);
        self.set_shape(engine);
        if self.measured_laws {
            self.set_measured_laws(engine);
        }
        if self.firmware_routing {
            engine.set_firmware_routing(FirmwareRouting::On);
        }
    }

    /// Sets every path to exactly this value, including back to the engine default. Runs on
    /// the audio thread between blocks; `set_b3_tail` rebuilds the wheel tables (setup-time cost),
    /// the other switches are plain field writes.
    pub fn apply_exact(&self, engine: &mut Stage4Engine) {
        let tone = self.tone_source.unwrap_or(ToneWheelSource::ModelDefault);
        let filter = self.filter_engine.unwrap_or(FilterEngine::Zdf);
        let chain = self.synth_chain.unwrap_or(SynthChain::Legacy);
        engine.organ_a.tonewheel_source = tone;
        engine.organ_b.tonewheel_source = tone;
        for s in [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ] {
            s.filter_engine = filter;
            s.chain = chain;
        }
        let fx = if self.firmware_fx {
            FirmwareFx::ALL
        } else {
            FirmwareFx::OFF
        };
        for c in engine.fx_chains.iter_mut() {
            c.firmware_fx = fx;
        }
        engine.piano_a.firmware_velocity_level = self.piano_firmware_level;
        engine.piano_b.firmware_velocity_level = self.piano_firmware_level;
        apply_b3_tail(engine, Some(self.b3_tail.unwrap_or(true)));
        self.set_measured_laws(engine);
        engine.set_firmware_routing(if self.firmware_routing {
            FirmwareRouting::On
        } else {
            FirmwareRouting::Off
        });
        self.set_shape(engine);
    }

    fn set_shape(&self, engine: &mut Stage4Engine) {
        let shape = self.synth_shape_milli.map(|m| f32::from(m) / 1000.0);
        engine.synth_a.strip_shape = shape;
        engine.synth_b.strip_shape = shape;
        engine.synth_c.strip_shape = shape;
    }

    fn set_measured_laws(&self, engine: &mut Stage4Engine) {
        engine.synth_a.measured_laws = self.measured_laws;
        engine.synth_b.measured_laws = self.measured_laws;
        engine.synth_c.measured_laws = self.measured_laws;
    }

    /// Short human-readable summary of the active paths (`"defaults"` when none is on).
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(t) = self.tone_source {
            parts.push(format!("tone={t:?}"));
        }
        if let Some(f) = self.filter_engine {
            parts.push(format!("filter={f:?}"));
        }
        if let Some(c) = self.synth_chain {
            parts.push(format!("chain={c:?}"));
        }
        if let Some(b) = self.b3_tail {
            parts.push(format!("b3-tail={}", if b { "on" } else { "off" }));
        }
        if self.firmware_fx {
            parts.push("fx".into());
        }
        if !self.piano_firmware_level {
            parts.push("legacy-piano".into());
        }
        if self.measured_laws {
            parts.push("measured".into());
        }
        if self.firmware_routing {
            parts.push("routing".into());
        }
        if parts.is_empty() {
            "defaults".into()
        } else {
            parts.join(" ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn from_args_parses_flags_presets_and_ignores_unknown() {
        assert_eq!(
            FirmwareOptions::from_args(&args(&["gui", "--program", "x.ns4p"])),
            Ok(FirmwareOptions::default())
        );
        let o = FirmwareOptions::from_args(&args(&[
            "--tone-source",
            "b3wheels",
            "--filter-engine",
            "firmware",
            "--synth-chain",
            "strip",
            "--b3-tail",
            "off",
            "--firmware-fx",
            "--firmware-piano-level",
            "--measured-laws",
        ]))
        .unwrap();
        assert_eq!(o.tone_source, Some(ToneWheelSource::B3Wheels));
        assert_eq!(o.b3_tail, Some(false));
        assert!(o.firmware_fx && o.piano_firmware_level && o.measured_laws);
        assert!(!o.piano_bus_scale, "the bus scale is opt-in");
        let bus = FirmwareOptions::from_args(&args(&["--firmware-piano-bus"])).unwrap();
        assert!(bus.piano_bus_scale);
        let mut e = Stage4Engine::new(48_000.0);
        bus.apply_to(&mut e);
        assert!(e.piano_a.firmware_bus_scale && e.piano_b.firmware_bus_scale);
        let all = FirmwareOptions::from_args(&args(&["--firmware-all"])).unwrap();
        assert_eq!(all, FirmwareOptions::firmware_all());
        assert!(!all.measured_laws);
        let over = FirmwareOptions::from_args(&args(&[
            "--firmware-all",
            "--tone-source",
            "vox",
            "--legacy-piano-level",
        ]))
        .unwrap();
        assert_eq!(over.tone_source, Some(ToneWheelSource::Vox));
        assert!(!over.piano_firmware_level);
        let legacy = FirmwareOptions::from_args(&args(&["--firmware-all", "--legacy-all"]));
        assert_eq!(legacy, Ok(FirmwareOptions::legacy_all()));
        assert!(FirmwareOptions::from_args(&args(&["--tone-source", "bogus"])).is_err());
    }

    #[test]
    fn apply_to_matches_the_individual_apply_calls() {
        let mut a = Stage4Engine::new(48_000.0);
        let mut b = Stage4Engine::new(48_000.0);
        FirmwareOptions::default().apply_to(&mut a);
        assert_eq!(a.organ_a.tonewheel_source, ToneWheelSource::ModelDefault);
        assert!(!a.synth_a.measured_laws && a.piano_a.firmware_velocity_level);
        FirmwareOptions::firmware_all().apply_to(&mut a);
        apply_firmware_options(
            &mut b,
            Some(ToneWheelSource::Firmware),
            Some(FilterEngine::Firmware),
            true,
        );
        apply_piano_level(&mut b, true);
        apply_synth_chain(&mut b, Some(SynthChain::FirmwareStrip));
        assert_eq!(a.organ_b.tonewheel_source, b.organ_b.tonewheel_source);
        assert_eq!(a.synth_c.filter_engine, b.synth_c.filter_engine);
        assert_eq!(a.synth_c.chain, b.synth_c.chain);
        assert_eq!(a.fx_chains[3].firmware_fx, FirmwareFx::ALL);
        assert!(a.piano_b.firmware_velocity_level);
        let m = FirmwareOptions {
            measured_laws: true,
            ..Default::default()
        };
        m.apply_to(&mut a);
        assert!(a.synth_a.measured_laws && a.synth_b.measured_laws && a.synth_c.measured_laws);
    }

    #[test]
    fn synth_shape_flag_sets_the_strip_shape_word() {
        let args: Vec<String> = ["--synth-shape", "0.25"].map(String::from).to_vec();
        let o = FirmwareOptions::from_args(&args).unwrap();
        assert_eq!(o.synth_shape_milli, Some(250));
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert_eq!(e.synth_b.strip_shape, Some(0.25));
        FirmwareOptions::default().apply_exact(&mut e);
        assert_eq!(e.synth_b.strip_shape, None);
        assert!(FirmwareOptions::from_args(&["--synth-shape".into(), "x".into()]).is_err());
    }

    #[test]
    fn apply_exact_round_trips_back_to_the_defaults() {
        let mut e = Stage4Engine::new(48_000.0);
        let on = FirmwareOptions {
            measured_laws: true,
            b3_tail: Some(false),
            ..FirmwareOptions::firmware_all()
        };
        on.apply_exact(&mut e);
        assert_eq!(e.synth_b.filter_engine, FilterEngine::Firmware);
        assert!(e.synth_c.measured_laws && e.piano_a.firmware_velocity_level);
        FirmwareOptions::default().apply_exact(&mut e);
        assert_eq!(e.organ_a.tonewheel_source, ToneWheelSource::ModelDefault);
        assert_eq!(e.synth_a.filter_engine, FilterEngine::Zdf);
        assert_eq!(e.synth_a.chain, SynthChain::Legacy);
        assert_eq!(e.fx_chains[0].firmware_fx, FirmwareFx::OFF);
        assert!(!e.synth_a.measured_laws && e.piano_b.firmware_velocity_level);
        FirmwareOptions::legacy_all().apply_exact(&mut e);
        assert!(!e.piano_a.firmware_velocity_level && !e.piano_b.firmware_velocity_level);
    }

    #[test]
    fn firmware_routing_flag_parses_and_switches_the_engine() {
        let o = FirmwareOptions::from_args(&args(&["--firmware-routing"])).unwrap();
        assert!(o.firmware_routing && !FirmwareOptions::firmware_all().firmware_routing);
        let off = FirmwareOptions::from_args(&args(&["--firmware-routing", "--legacy-routing"]));
        assert_eq!(off, Ok(FirmwareOptions::default()));
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert_eq!(e.firmware_routing(), FirmwareRouting::On);
        assert!(o.summary().contains("routing"));
        FirmwareOptions::default().apply_exact(&mut e);
        assert_eq!(e.firmware_routing(), FirmwareRouting::Off);
    }

    #[test]
    fn summary_lists_active_paths() {
        assert_eq!(FirmwareOptions::default().summary(), "defaults");
        let s = FirmwareOptions::firmware_all().summary();
        assert!(s.contains("tone=Firmware") && s.contains("fx") && !s.contains("legacy-piano"));
        assert!(FirmwareOptions::legacy_all()
            .summary()
            .contains("legacy-piano"));
    }

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
        assert_eq!(
            parse_tone_source("farfisa"),
            Ok(ToneWheelSource::FarfisaWheels)
        );
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
        assert_eq!(e.fx_chains[0].firmware_fx, FirmwareFx::DEFAULT);
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

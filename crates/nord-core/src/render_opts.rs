//! Opt-in firmware paths shared by `ns4 render-patch` and the GUI (`--tone-source`,
//! `--filter-engine`, `--synth-chain`, `--firmware-fx`, `--measured-laws`, ...). Defaults leave the engine untouched, so default renders stay bit-identical.

use crate::engine::{FirmwareRouting, Stage4Engine};
use crate::fx::FirmwareFx;
use crate::organ::{OutputLevel, ToneWheelSource};
use crate::synth::{FilterEngine, PSpec, PTableSpec, SynthChain};

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

/// Parses an `--organ-level` value: `default | legacy | firmware | chain`.
pub fn parse_organ_level(v: &str) -> Result<OutputLevel, String> {
    match v.to_ascii_lowercase().as_str() {
        "default" | "model" => Ok(OutputLevel::ModelDefault),
        "legacy" => Ok(OutputLevel::Legacy),
        "firmware" => Ok(OutputLevel::Firmware),
        "chain" => Ok(OutputLevel::Chain),
        _ => Err(format!(
            "invalid --organ-level '{v}' (expected default|legacy|firmware|chain)"
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

/// Switches the Farfisa processor stage of both organ layers (`None` leaves the engine untouched).
pub fn apply_farfisa_processor(engine: &mut Stage4Engine, on: Option<bool>) {
    if let Some(on) = on {
        engine.organ_a.farfisa_processor = on;
        engine.organ_b.farfisa_processor = on;
    }
}

/// Switches the B3 Bass processor stage of both organ layers (`None` leaves the engine untouched).
pub fn apply_b3_bass_processor(engine: &mut Stage4Engine, on: Option<bool>) {
    if let Some(on) = on {
        engine.organ_a.set_b3_bass_processor(on);
        engine.organ_b.set_b3_bass_processor(on);
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

/// Opt-in firmware Leslie on both organ layers (`--leslie firmware`): the instruction-level port of
/// the PNO core 1 processor replaces the engine rotary ([`crate::fx::FwLeslie`], `fx-leslie.md`).
/// `gain_db` is applied after it (the booted firmware level is about -50 dB, so the level stage that
/// makes it up in the hardware is open; `0.0` keeps the firmware level).
pub fn apply_leslie_firmware(engine: &mut Stage4Engine, on: bool, gain_db: f32) {
    apply_leslie_firmware_ring(engine, on, gain_db, crate::fx::RingAdvance::ADVANCE);
}

/// [`apply_leslie_firmware`] with an explicit history ring behaviour (`--leslie-ring`).
pub fn apply_leslie_firmware_ring(
    engine: &mut Stage4Engine,
    on: bool,
    gain_db: f32,
    ring: crate::fx::RingAdvance,
) {
    if on {
        for organ in [&mut engine.organ_a, &mut engine.organ_b] {
            organ.leslie_fw = Some(Box::new(crate::fx::FwLeslie::with_ring(ring)));
            organ.leslie_gain = 10f32.powf(gain_db / 20.0);
        }
    }
}

/// Parses a `--leslie-ring` value: `firmware | advance | frozen`.
pub fn parse_leslie_ring(v: &str) -> Result<crate::fx::RingAdvance, String> {
    use crate::fx::RingAdvance;
    match v {
        "firmware" => Ok(RingAdvance::PASS_RESET8),
        "advance" => Ok(RingAdvance::ADVANCE),
        "frozen" => Ok(RingAdvance::FROZEN),
        _ => Err(format!(
            "invalid --leslie-ring '{v}' (expected firmware|advance|frozen)"
        )),
    }
}

/// Name of a ring behaviour as `--leslie-ring` spells it (`custom` for other values).
pub fn leslie_ring_name(r: crate::fx::RingAdvance) -> &'static str {
    use crate::fx::RingAdvance;
    if r == RingAdvance::PASS_RESET8 {
        "firmware"
    } else if r == RingAdvance::ADVANCE {
        "advance"
    } else if r == RingAdvance::FROZEN {
        "frozen"
    } else {
        "custom"
    }
}

/// Rescales the program's rotary drive from the preset unit (0..10, `cbin/organ_preset.rs`) to the
/// engine unit (0..1: `rotary.rs` uses `gain = 1 + 3 drive`). The default program path passes the
/// preset unit through unchanged, so a stored drive of 2.44 reaches the tube law as 2.44 (gain
/// 8.3) instead of 0.244 (gain 1.73): EMPIRICAL, see `docs/research/b3-rotary.md`.
pub fn apply_rotary_scaled(engine: &mut Stage4Engine) {
    for organ in [&mut engine.organ_a, &mut engine.organ_b] {
        organ.rotary_drive = (organ.rotary_drive / 10.0).clamp(0.0, 1.0);
    }
}

/// Rotary drive law (`--rotary-drive engine|unity|scaled`). Both opt-in laws are flags on the organ
/// layer, so they can be switched live and back (EMPIRICAL, `docs/research/b3-rotary.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RotaryDrive {
    /// `tanh(g x)` with the program drive passed through unchanged.
    #[default]
    Engine,
    /// `tanh(g x) / g` (unity small-signal gain).
    Unity,
    /// Program drive rescaled from the preset unit 0..10 to the engine unit 0..1.
    Scaled,
}

/// Parses a `--rotary-drive` value: `engine | unity | scaled`.
pub fn parse_rotary_drive(v: &str) -> Result<RotaryDrive, String> {
    match v.to_ascii_lowercase().as_str() {
        "engine" | "default" => Ok(RotaryDrive::Engine),
        "unity" => Ok(RotaryDrive::Unity),
        "scaled" => Ok(RotaryDrive::Scaled),
        _ => Err(format!(
            "invalid --rotary-drive '{v}' (expected engine|unity|scaled)"
        )),
    }
}

/// Parses an `--amp-path` value: `engine | firmware` (`true` = firmware drive stage).
pub fn parse_amp_path(v: &str) -> Result<bool, String> {
    match v.to_ascii_lowercase().as_str() {
        "firmware" => Ok(true),
        "engine" => Ok(false),
        _ => Err(format!(
            "invalid --amp-path '{v}' (expected engine|firmware)"
        )),
    }
}

/// Parses `--sample-kernel`: `key` (firmware rule, true) or `ratio` (ASSUMED rule, false).
pub fn parse_sample_kernel(v: &str) -> Result<bool, String> {
    match v.to_ascii_lowercase().as_str() {
        "key" => Ok(true),
        "ratio" => Ok(false),
        _ => Err(format!(
            "invalid --sample-kernel '{v}' (expected key|ratio)"
        )),
    }
}

/// Parses `--sample-pitch`: `fw` (firmware exponent tables, true) or `equal` (exact `2^(n/12)`).
pub fn parse_sample_pitch(v: &str) -> Result<bool, String> {
    match v.to_ascii_lowercase().as_str() {
        "fw" | "firmware" => Ok(true),
        "equal" => Ok(false),
        _ => Err(format!("invalid --sample-pitch '{v}' (expected fw|equal)")),
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
    /// `--farfisa-processor on|off` (`None` = off): the Farfisa key-down weights and measured line
    /// gain of the processor `0x122B85` on `--tone-source farfisa`-style `FarfisaWheels` layers.
    pub farfisa_processor: Option<bool>,
    /// `--b3-bass-processor on|off` (`None` = off): the B3 Bass key stage (generator weights,
    /// window words and measured line gain of the processor `0x122296`); needs `--tone-source
    /// b3wheels` on a B3 Bass layer.
    pub b3_bass_processor: Option<bool>,
    /// The `FirmwareFx` ports of every FX chain (engine default: only `amp`). `--firmware-fx` sets
    /// every port, `--firmware-reverb` the `reverb` port (Spring and, types 2..11, the multi-tap
    /// routines), `--firmware-fx-ports a,b` the listed ones; `amp` is overridden by `amp_path`.
    pub fx: FirmwareFx,
    /// `--firmware-piano-level`: `PianoLayer::firmware_velocity_level`.
    pub piano_firmware_level: bool,
    /// `--firmware-piano-bus`: `PianoLayer::firmware_bus_scale` (opt-in, ASSUMED receiver scale).
    pub piano_bus_scale: bool,
    /// `--firmware-piano-chain`: `PianoLayer::receiver_chain_scale` (verified FX path 0.7464 per
    /// `y / 8192`; opt-in, engine unit = DAC fraction ASSUMED).
    pub piano_chain_scale: bool,
    /// `--firmware-piano-layer-law`: the piano layer fader follows `(v / 127)^2` (opt-in).
    pub piano_layer_law: bool,
    /// `--measured-laws`: `SynthLayer::measured_laws` (EMPIRICAL, Audible Genius clips).
    pub measured_laws: bool,
    /// `--synth-shape <0..1>`: strip shape word of the SHAPE oscillator (`fw_shape.rs`) in
    /// thousandths (`None` = the ASSUMED saw placeholder).
    pub synth_shape_milli: Option<u16>,
    /// `--synth-p fitted|gain,w2,r2,shape,cutoff`: the EMPIRICAL oscillator P block of the strip
    /// chain (`synth/p_block.rs`); `None` keeps the single idle oscillator.
    pub synth_p: Option<PSpec>,
    /// `--synth-stages`: `SynthLayer::strip_stages` (noise, Shape Square, SUPER, MULTI firmware stages
    /// on the strip chain; opt-in, voice ratios and weights ASSUMED).
    pub synth_stages: bool,
    /// `--sample-kernel key|ratio`: the `SAMPLES` oscillator picks its 12-tap kernel with the
    /// firmware key rule (`sinc::key_rule`; VERIFIED run and key field, D40) instead of the
    /// ASSUMED ratio rule. On by default; `ratio` (and `--legacy-all`) selects the old rule.
    pub sample_kernel_key: bool,
    /// `--sample-pitch fw|equal`: the `SAMPLES` oscillator step follows the ARM exponent tables
    /// (`sinc::pitch_law`, VERIFIED law; root and sample-rate fields ASSUMED). Off by default.
    pub sample_pitch_fw: bool,
    /// `--firmware-routing`: section / layer enables and per-layer KB zones as the program stores
    /// them ([`FirmwareRouting`]); stays out of `firmware_all` until the zone codes are verified.
    pub firmware_routing: bool,
    /// `--amp-path engine|firmware`: `FirmwareFx::amp` on every chain (`None` = the engine default,
    /// firmware drive stage on; `Some(false)` = engine drive and RBJ cabinet).
    pub amp_path: Option<bool>,
    /// `--rotary-drive engine|unity|scaled`.
    pub rotary_drive: RotaryDrive,
    /// `--organ-level firmware|legacy`: the organ output level law ([`OutputLevel`]): the swell
    /// word, the squared layer level and the bus gain `ORGAN_BUS_GAIN` on the firmware organ
    /// paths.
    pub organ_level: OutputLevel,
    /// `--leslie firmware|engine`: the firmware Leslie processor instead of the engine rotary.
    pub leslie_firmware: bool,
    /// `--leslie-gain-db <dB>`: gain after the firmware Leslie in millidecibel (default 0).
    pub leslie_gain_mdb: i32,
    /// `--leslie-ring firmware|advance|frozen`: the history ring pointers of the firmware Leslie
    /// (default `advance`: 4 words per call; `firmware`: re-based every 8 calls like the booted
    /// pass routine; `frozen`: the first oracle state, `fx-leslie.md` s10).
    pub leslie_ring: crate::fx::RingAdvance,
}

/// The engine defaults: every opt-in path off, except the firmware piano level, which is the
/// default for `.npno` sounds (D23).
impl Default for FirmwareOptions {
    fn default() -> Self {
        Self {
            tone_source: None,
            filter_engine: None,
            synth_chain: Some(SynthChain::FirmwareStrip),
            b3_tail: None,
            farfisa_processor: None,
            b3_bass_processor: None,
            fx: FirmwareFx::DEFAULT,
            piano_firmware_level: true,
            piano_bus_scale: false,
            piano_chain_scale: false,
            piano_layer_law: false,
            measured_laws: false,
            synth_shape_milli: None,
            synth_p: Some(PSpec::Table(PTableSpec::BUILTIN)),
            synth_stages: true,
            sample_kernel_key: true,
            sample_pitch_fw: false,
            firmware_routing: false,
            amp_path: None,
            rotary_drive: RotaryDrive::Engine,
            organ_level: OutputLevel::ModelDefault,
            leslie_firmware: false,
            leslie_gain_mdb: 0,
            leslie_ring: crate::fx::RingAdvance::ADVANCE,
        }
    }
}

impl FirmwareOptions {
    /// Every opt-in path off and the legacy piano velocity response (`--legacy-all`).
    pub fn legacy_all() -> Self {
        Self {
            synth_chain: Some(SynthChain::Legacy),
            synth_p: None,
            synth_stages: false,
            piano_firmware_level: false,
            sample_kernel_key: false,
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
            farfisa_processor: None,
            b3_bass_processor: None,
            fx: FirmwareFx::ALL,
            piano_firmware_level: true,
            piano_bus_scale: false,
            piano_chain_scale: false,
            piano_layer_law: false,
            measured_laws: false,
            synth_shape_milli: None,
            synth_p: Some(PSpec::Table(PTableSpec::BUILTIN)),
            synth_stages: true,
            sample_kernel_key: true,
            sample_pitch_fw: false,
            firmware_routing: false,
            amp_path: None,
            rotary_drive: RotaryDrive::Engine,
            organ_level: OutputLevel::ModelDefault,
            leslie_firmware: false,
            leslie_gain_mdb: 0,
            leslie_ring: crate::fx::RingAdvance::ADVANCE,
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
        if let Some(v) = value("--farfisa-processor") {
            o.farfisa_processor = Some(parse_on_off("--farfisa-processor", v)?);
        }
        if let Some(v) = value("--b3-bass-processor") {
            o.b3_bass_processor = Some(parse_on_off("--b3-bass-processor", v)?);
        }
        if has("--firmware-fx") {
            o.fx = FirmwareFx::ALL;
        }
        if has("--firmware-reverb") {
            o.fx.reverb = true;
        }
        if let Some(v) = value("--firmware-fx-ports") {
            o.fx.enable_ports(v)?;
        }
        o.piano_firmware_level |= has("--firmware-piano-level");
        if has("--legacy-piano-level") {
            o.piano_firmware_level = false;
        }
        o.piano_bus_scale |= has("--firmware-piano-bus");
        o.piano_chain_scale |= has("--firmware-piano-chain");
        o.piano_layer_law |= has("--firmware-piano-layer-law");
        o.measured_laws |= has("--measured-laws");
        o.synth_stages |= has("--synth-stages");
        if has("--no-synth-stages") {
            o.synth_stages = false;
        }
        if let Some(v) = value("--sample-kernel") {
            o.sample_kernel_key = parse_sample_kernel(v)?;
        }
        if let Some(v) = value("--sample-pitch") {
            o.sample_pitch_fw = parse_sample_pitch(v)?;
        }
        o.firmware_routing |= has("--firmware-routing");
        if has("--legacy-routing") {
            o.firmware_routing = false;
        }
        if let Some(v) = value("--amp-path") {
            o.amp_path = Some(parse_amp_path(v)?);
        }
        if let Some(v) = value("--rotary-drive") {
            o.rotary_drive = parse_rotary_drive(v)?;
        }
        if let Some(v) = value("--organ-level") {
            o.organ_level = parse_organ_level(v)?;
        }
        if let Some(v) = value("--leslie") {
            o.leslie_firmware = match v {
                "firmware" => true,
                "engine" => false,
                _ => return Err(format!("invalid --leslie '{v}' (firmware|engine)")),
            };
        }
        if let Some(v) = value("--leslie-ring") {
            o.leslie_ring = parse_leslie_ring(v)?;
        }
        if let Some(v) = value("--leslie-gain-db") {
            let db: f32 = v
                .parse()
                .map_err(|_| format!("invalid --leslie-gain-db '{v}'"))?;
            o.leslie_gain_mdb = (db * 1000.0).round() as i32;
        }
        if let Some(v) = value("--synth-shape") {
            let x: f32 = v
                .parse()
                .map_err(|_| format!("invalid --synth-shape '{v}'"))?;
            o.synth_shape_milli = Some((x.clamp(0.0, 1.0) * 1000.0).round() as u16);
        }
        if let Some(v) = value("--synth-p") {
            o.synth_p = if v.eq_ignore_ascii_case("off") {
                None
            } else {
                Some(PSpec::parse(v)?)
            };
        }
        Ok(o)
    }

    /// Switches the selected paths on (identical to the individual `apply_*` calls render-patch
    /// used before this struct existed).
    pub fn apply_to(&self, engine: &mut Stage4Engine) {
        apply_firmware_options(engine, self.tone_source, self.filter_engine, false);
        for c in engine.fx_chains.iter_mut() {
            c.firmware_fx.merge_on(&self.fx);
        }
        apply_piano_level(engine, self.piano_firmware_level);
        engine.piano_a.firmware_bus_scale = self.piano_bus_scale;
        engine.piano_b.firmware_bus_scale = self.piano_bus_scale;
        engine.piano_a.receiver_chain_scale = self.piano_chain_scale;
        engine.piano_b.receiver_chain_scale = self.piano_chain_scale;
        engine.piano_a.firmware_layer_law = self.piano_layer_law;
        engine.piano_b.firmware_layer_law = self.piano_layer_law;
        apply_b3_tail(engine, self.b3_tail);
        apply_farfisa_processor(engine, self.farfisa_processor);
        apply_b3_bass_processor(engine, self.b3_bass_processor);
        apply_leslie_firmware_ring(
            engine,
            self.leslie_firmware,
            self.leslie_gain_mdb as f32 / 1000.0,
            self.leslie_ring,
        );
        apply_synth_chain(engine, self.synth_chain);
        self.set_shape(engine);
        self.set_p_block(engine);
        self.set_stages(engine);
        self.set_sample_kernel(engine);
        if self.measured_laws {
            self.set_measured_laws(engine);
        }
        if self.firmware_routing {
            engine.set_firmware_routing(FirmwareRouting::On);
        }
        apply_amp_path(engine, self.amp_path);
        self.set_rotary(engine);
        self.set_organ_level(engine);
    }

    /// Sets every path to exactly this value, including back to the engine default. Runs on
    /// the audio thread between blocks; `set_b3_tail` rebuilds the wheel tables (setup-time cost),
    /// the other switches are plain field writes.
    pub fn apply_exact(&self, engine: &mut Stage4Engine) {
        let tone = self.tone_source.unwrap_or(ToneWheelSource::ModelDefault);
        let filter = self.filter_engine.unwrap_or(FilterEngine::Zdf);
        let chain = self.synth_chain.unwrap_or(SynthChain::FirmwareStrip);
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
        let mut fx = self.fx;
        if let Some(amp) = self.amp_path {
            fx.amp = amp;
        }
        for c in engine.fx_chains.iter_mut() {
            c.firmware_fx = fx;
        }
        engine.piano_a.firmware_velocity_level = self.piano_firmware_level;
        engine.piano_b.firmware_velocity_level = self.piano_firmware_level;
        engine.piano_a.firmware_bus_scale = self.piano_bus_scale;
        engine.piano_b.firmware_bus_scale = self.piano_bus_scale;
        engine.piano_a.receiver_chain_scale = self.piano_chain_scale;
        engine.piano_b.receiver_chain_scale = self.piano_chain_scale;
        engine.piano_a.firmware_layer_law = self.piano_layer_law;
        engine.piano_b.firmware_layer_law = self.piano_layer_law;
        self.set_rotary(engine);
        let bass = self.b3_bass_processor == Some(true);
        apply_b3_tail(engine, Some(!bass && self.b3_tail.unwrap_or(true)));
        for organ in [&mut engine.organ_a, &mut engine.organ_b] {
            if organ.b3_bass_processor_on() != bass {
                organ.set_b3_bass_processor(bass);
            }
        }
        self.set_measured_laws(engine);
        engine.set_firmware_routing(if self.firmware_routing {
            FirmwareRouting::On
        } else {
            FirmwareRouting::Off
        });
        self.set_shape(engine);
        self.set_p_block(engine);
        self.set_stages(engine);
        self.set_sample_kernel(engine);
        self.set_organ_level(engine);
        self.set_leslie(engine);
        for organ in [&mut engine.organ_a, &mut engine.organ_b] {
            organ.farfisa_processor = self.farfisa_processor == Some(true);
        }
    }

    /// Firmware Leslie on both organ layers, or back to the engine rotary. The processor is only
    /// rebuilt (one allocation) when it is switched on or its ring behaviour changes, so repeated
    /// identical commands keep the rotor state.
    fn set_leslie(&self, engine: &mut Stage4Engine) {
        for organ in [&mut engine.organ_a, &mut engine.organ_b] {
            if !self.leslie_firmware {
                organ.leslie_fw = None;
                organ.leslie_gain = 1.0;
                continue;
            }
            if organ.leslie_fw.as_ref().map(|l| l.ring()) != Some(self.leslie_ring) {
                organ.leslie_fw = Some(Box::new(crate::fx::FwLeslie::with_ring(self.leslie_ring)));
            }
            organ.leslie_gain = 10f32.powf(self.leslie_gain_mdb as f32 / 20_000.0);
        }
    }

    fn set_organ_level(&self, engine: &mut Stage4Engine) {
        engine.organ_a.output_level = self.organ_level;
        engine.organ_b.output_level = self.organ_level;
    }

    fn set_rotary(&self, engine: &mut Stage4Engine) {
        for organ in [&mut engine.organ_a, &mut engine.organ_b] {
            organ.rotary_drive_unity = self.rotary_drive == RotaryDrive::Unity;
            organ.rotary_drive_scaled = self.rotary_drive == RotaryDrive::Scaled;
        }
    }

    fn set_shape(&self, engine: &mut Stage4Engine) {
        let shape = self.synth_shape_milli.map(|m| f32::from(m) / 1000.0);
        engine.synth_a.strip_shape = shape;
        engine.synth_b.strip_shape = shape;
        engine.synth_c.strip_shape = shape;
    }

    fn set_p_block(&self, engine: &mut Stage4Engine) {
        let table = self.synth_p.and_then(PSpec::table);
        let p = self.synth_p.filter(|_| table.is_none()).map(PSpec::block);
        for s in [
            &mut engine.synth_a,
            &mut engine.synth_b,
            &mut engine.synth_c,
        ] {
            s.p_block = p;
            s.p_table = table;
        }
    }

    fn set_sample_kernel(&self, engine: &mut Stage4Engine) {
        engine.synth_a.sample_kernel_by_key = self.sample_kernel_key;
        engine.synth_b.sample_kernel_by_key = self.sample_kernel_key;
        engine.synth_c.sample_kernel_by_key = self.sample_kernel_key;
        engine.synth_a.sample_pitch_fw = self.sample_pitch_fw;
        engine.synth_b.sample_pitch_fw = self.sample_pitch_fw;
        engine.synth_c.sample_pitch_fw = self.sample_pitch_fw;
    }

    fn set_stages(&self, engine: &mut Stage4Engine) {
        engine.synth_a.strip_stages = self.synth_stages;
        engine.synth_b.strip_stages = self.synth_stages;
        engine.synth_c.strip_stages = self.synth_stages;
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
        if self.synth_chain != Self::default().synth_chain {
            parts.push(format!("chain={:?}", self.synth_chain));
        }
        if let Some(b) = self.b3_tail {
            parts.push(format!("b3-tail={}", if b { "on" } else { "off" }));
        }
        if let Some(b) = self.farfisa_processor {
            parts.push(format!(
                "farfisa-processor={}",
                if b { "on" } else { "off" }
            ));
        }
        if let Some(b) = self.b3_bass_processor {
            parts.push(format!(
                "b3-bass-processor={}",
                if b { "on" } else { "off" }
            ));
        }
        let ports = self.fx.ports_on();
        if ports.len() == crate::fx::FX_PORT_NAMES.len() {
            parts.push("fx".into());
        } else {
            parts.extend(ports.iter().map(|p| (*p).to_string()));
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
        if self.piano_bus_scale {
            parts.push("piano-bus".into());
        }
        if self.piano_chain_scale {
            parts.push("piano-chain".into());
        }
        if self.piano_layer_law {
            parts.push("piano-layer-law".into());
        }
        if let Some(m) = self.synth_shape_milli {
            parts.push(format!("shape={:.2}", f32::from(m) / 1000.0));
        }
        if self.synth_p != Self::default().synth_p {
            parts.push(format!("synth-p={:?}", self.synth_p));
        }
        if !self.synth_stages {
            parts.push("no-stages".into());
        }
        if self.leslie_firmware {
            parts.push(format!(
                "leslie=firmware ring={} gain={:+.1}dB",
                leslie_ring_name(self.leslie_ring),
                self.leslie_gain_mdb as f32 / 1000.0
            ));
        }
        if !self.sample_kernel_key {
            parts.push("sample-kernel=ratio".into());
        }
        if self.sample_pitch_fw {
            parts.push("sample-pitch=fw".into());
        }
        if let Some(a) = self.amp_path {
            parts.push(format!("amp={}", if a { "firmware" } else { "engine" }));
        }
        if self.rotary_drive != RotaryDrive::Engine {
            parts.push(format!("rotary={:?}", self.rotary_drive));
        }
        if self.organ_level != OutputLevel::ModelDefault {
            parts.push(format!("organ-level={:?}", self.organ_level));
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
    #[test]
    fn rotary_drive_scaled_maps_the_preset_unit_to_the_engine_unit() {
        let mut e = Stage4Engine::new(48_000.0);
        e.organ_a.rotary_drive = 2.44;
        e.organ_b.rotary_drive = 12.0;
        apply_rotary_scaled(&mut e);
        assert!((e.organ_a.rotary_drive - 0.244).abs() < 1e-6);
        assert_eq!(e.organ_b.rotary_drive, 1.0, "clamped to the engine range");
    }

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
        assert!(o.fx == FirmwareFx::ALL && o.piano_firmware_level && o.measured_laws);
        assert!(!o.piano_bus_scale, "the bus scale is opt-in");
        let bus = FirmwareOptions::from_args(&args(&["--firmware-piano-bus"])).unwrap();
        assert!(bus.piano_bus_scale);
        assert!(!o.piano_chain_scale, "the receiver chain scale is opt-in");
        let chain = FirmwareOptions::from_args(&args(&["--firmware-piano-chain"])).unwrap();
        assert!(chain.piano_chain_scale && chain.summary().contains("piano-chain"));
        let mut e = Stage4Engine::new(48_000.0);
        chain.apply_to(&mut e);
        assert!(e.piano_a.receiver_chain_scale && e.piano_b.receiver_chain_scale);
        FirmwareOptions::default().apply_exact(&mut e);
        assert!(!e.piano_a.receiver_chain_scale);
        assert!(!o.piano_layer_law, "the layer law is opt-in");
        let law = FirmwareOptions::from_args(&args(&["--firmware-piano-layer-law"])).unwrap();
        assert!(law.piano_layer_law && law.summary().contains("piano-layer-law"));
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
    fn sample_kernel_flag_selects_the_firmware_key_rule_on_every_layer() {
        assert!(FirmwareOptions::default().sample_kernel_key);
        assert!(!FirmwareOptions::default()
            .summary()
            .contains("sample-kernel"));
        let o = FirmwareOptions::from_args(&["--sample-kernel".into(), "key".into()]).unwrap();
        assert!(o.sample_kernel_key);
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert!(e.synth_a.sample_kernel_by_key && e.synth_b.sample_kernel_by_key);
        assert!(e.synth_c.sample_kernel_by_key);
        let r = FirmwareOptions::from_args(&["--sample-kernel".into(), "ratio".into()]).unwrap();
        assert!(!r.sample_kernel_key && r.summary().contains("sample-kernel=ratio"));
        r.apply_to(&mut e);
        assert!(!e.synth_a.sample_kernel_by_key && !e.synth_c.sample_kernel_by_key);
        assert!(!FirmwareOptions::legacy_all().sample_kernel_key);
        assert!(FirmwareOptions::firmware_all().sample_kernel_key);
        assert!(parse_sample_kernel("bogus").is_err());
    }

    #[test]
    fn sample_pitch_flag_selects_the_firmware_exponent_law_on_every_layer() {
        assert!(!FirmwareOptions::default().sample_pitch_fw);
        let o = FirmwareOptions::from_args(&["--sample-pitch".into(), "fw".into()]).unwrap();
        assert!(o.sample_pitch_fw && o.summary().contains("sample-pitch=fw"));
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert!(
            e.synth_a.sample_pitch_fw && e.synth_b.sample_pitch_fw && e.synth_c.sample_pitch_fw
        );
        let r = FirmwareOptions::from_args(&["--sample-pitch".into(), "equal".into()]).unwrap();
        assert!(!r.sample_pitch_fw);
        assert!(parse_sample_pitch("bogus").is_err());
    }

    #[test]
    fn no_synth_stages_flag_turns_the_stage_oscillators_off() {
        let o = FirmwareOptions::from_args(&["--no-synth-stages".into()]).unwrap();
        assert!(!o.synth_stages && o.summary().contains("no-stages"));
        let mut e = Stage4Engine::new(48_000.0);
        assert!(e.synth_a.strip_stages, "the stages are the default (D22)");
        o.apply_to(&mut e);
        assert!(!e.synth_a.strip_stages && !e.synth_c.strip_stages);
        FirmwareOptions::default().apply_exact(&mut e);
        assert!(e.synth_b.strip_stages);
        assert!(!FirmwareOptions::legacy_all().synth_stages);
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
    fn synth_p_flag_sets_and_clears_the_p_block() {
        let args: Vec<String> = ["--synth-p", "fitted"].map(String::from).to_vec();
        let o = FirmwareOptions::from_args(&args).unwrap();
        assert_eq!(o.synth_p, Some(PSpec::Fitted));
        assert!(o.summary().contains("synth-p"));
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert_eq!(e.synth_c.p_block, Some(crate::synth::PBlock::FITTED));
        let custom = FirmwareOptions::from_args(&["--synth-p".into(), "1,0,1,0.5,1".into()]);
        assert_eq!(
            custom.unwrap().synth_p,
            Some(PSpec::Custom([1000, 0, 1000, 500, 1000]))
        );
        FirmwareOptions::default().apply_exact(&mut e);
        assert_eq!(e.synth_c.p_block, None);
        assert_eq!(e.synth_c.p_table, Some(crate::synth::PTable::FITTED));
        assert!(FirmwareOptions::from_args(&["--synth-p".into(), "x".into()]).is_err());
        assert_eq!(
            FirmwareOptions::firmware_all().synth_p,
            FirmwareOptions::default().synth_p,
            "the table is the default"
        );
        let off = FirmwareOptions::from_args(&["--synth-p".into(), "off".into()]).unwrap();
        assert_eq!(off.synth_p, None);
        off.apply_to(&mut e);
        assert_eq!((e.synth_a.p_block, e.synth_a.p_table), (None, None));
        let table =
            FirmwareOptions::from_args(&["--synth-p".into(), "table:saw=1/0/1/.5/1".into()])
                .unwrap();
        table.apply_to(&mut e);
        let row = e
            .synth_a
            .p_table
            .unwrap()
            .block(crate::synth::PWave::PureSaw);
        assert_eq!((row.gain, row.cutoff_scale), (1.0, 1.0));
    }

    #[test]
    fn apply_exact_round_trips_back_to_the_defaults() {
        let mut e = Stage4Engine::new(48_000.0);
        let on = FirmwareOptions {
            measured_laws: true,
            b3_tail: Some(false),
            farfisa_processor: None,
            b3_bass_processor: None,
            ..FirmwareOptions::firmware_all()
        };
        on.apply_exact(&mut e);
        assert_eq!(e.synth_b.filter_engine, FilterEngine::Firmware);
        assert!(e.synth_c.measured_laws && e.piano_a.firmware_velocity_level);
        FirmwareOptions::default().apply_exact(&mut e);
        assert_eq!(e.organ_a.tonewheel_source, ToneWheelSource::ModelDefault);
        assert_eq!(e.synth_a.filter_engine, FilterEngine::Zdf);
        assert_eq!(e.synth_a.chain, SynthChain::FirmwareStrip);
        assert!(e.synth_a.strip_stages && e.synth_a.p_table.is_some());
        assert_eq!(e.fx_chains[0].firmware_fx, FirmwareFx::DEFAULT);
        assert!(!e.synth_a.measured_laws && e.piano_b.firmware_velocity_level);
        FirmwareOptions::legacy_all().apply_exact(&mut e);
        assert!(!e.piano_a.firmware_velocity_level && !e.piano_b.firmware_velocity_level);
        assert_eq!(e.synth_a.chain, SynthChain::Legacy);
        assert!(!e.synth_a.strip_stages && e.synth_a.p_table.is_none());
    }

    #[test]
    fn firmware_reverb_flag_switches_only_the_reverb() {
        let o = FirmwareOptions::from_args(&args(&["--firmware-reverb"])).unwrap();
        assert!(o.fx.reverb && o.fx.ports_on() == ["reverb"]);
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert!(e.fx_chains.iter().all(|c| c.firmware_fx.reverb));
        assert!(e.fx_chains.iter().all(|c| !c.firmware_fx.delay_laws));
        assert!(o.summary().contains("reverb"));
        o.apply_exact(&mut e);
        assert!(e
            .fx_chains
            .iter()
            .all(|c| c.firmware_fx.reverb && !c.firmware_fx.compressor));
        FirmwareOptions::default().apply_exact(&mut e);
        assert!(e.fx_chains.iter().all(|c| !c.firmware_fx.reverb));
    }

    #[test]
    fn organ_level_flag_selects_the_output_level_law() {
        let o = FirmwareOptions::from_args(&args(&["--organ-level", "firmware"])).unwrap();
        assert_eq!(o.organ_level, OutputLevel::Firmware);
        let c = FirmwareOptions::from_args(&args(&["--organ-level", "chain"])).unwrap();
        assert_eq!(c.organ_level, OutputLevel::Chain);
        assert_eq!(
            FirmwareOptions::default().organ_level,
            OutputLevel::ModelDefault
        );
        assert!(FirmwareOptions::from_args(&args(&["--organ-level", "x"])).is_err());
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert_eq!(e.organ_b.output_level, OutputLevel::Firmware);
        assert!(o.summary().contains("organ-level=Firmware"));
        FirmwareOptions::default().apply_exact(&mut e);
        assert_eq!(e.organ_a.output_level, OutputLevel::ModelDefault);
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
    fn amp_path_and_rotary_drive_parse_apply_and_revert() {
        let o = FirmwareOptions::from_args(&args(&[
            "--amp-path",
            "engine",
            "--rotary-drive",
            "scaled",
        ]))
        .unwrap();
        assert_eq!(
            (o.amp_path, o.rotary_drive),
            (Some(false), RotaryDrive::Scaled)
        );
        assert!(FirmwareOptions::from_args(&args(&["--amp-path", "x"])).is_err());
        assert!(FirmwareOptions::from_args(&args(&["--rotary-drive", "x"])).is_err());
        assert_eq!(parse_rotary_drive("Unity"), Ok(RotaryDrive::Unity));
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_to(&mut e);
        assert!(!e.fx_chains[2].firmware_fx.amp && e.organ_a.rotary_drive_scaled);
        let s = o.summary();
        assert!(s.contains("amp=engine") && s.contains("rotary=Scaled"));
        let u = FirmwareOptions {
            rotary_drive: RotaryDrive::Unity,
            amp_path: Some(true),
            piano_bus_scale: true,
            ..Default::default()
        };
        u.apply_exact(&mut e);
        assert!(e.fx_chains[2].firmware_fx.amp && e.organ_b.rotary_drive_unity);
        assert!(!e.organ_b.rotary_drive_scaled && e.piano_a.firmware_bus_scale);
        FirmwareOptions::default().apply_exact(&mut e);
        assert!(!e.organ_a.rotary_drive_unity && !e.piano_b.firmware_bus_scale);
        assert_eq!(
            e.organ_a.rotary_drive, 0.2,
            "the stored drive is never rewritten"
        );
        assert!(u.summary().contains("piano-bus"));
    }

    #[test]
    fn repeated_apply_exact_keeps_b3_wheels_and_their_phases() {
        let o = FirmwareOptions {
            tone_source: Some(ToneWheelSource::B3Wheels),
            b3_tail: Some(false),
            farfisa_processor: None,
            b3_bass_processor: None,
            ..Default::default()
        };
        let mut e = Stage4Engine::new(48_000.0);
        o.apply_exact(&mut e);
        e.organ_a.rotary_enabled = false;
        e.organ_a.note_on(60);
        let first: Vec<f32> = (0..2_000).map(|_| e.organ_a.render_frame()[0]).collect();
        o.apply_exact(&mut e); // same options again while playing: no table rebuild, no phase reset
        let mut fresh = Stage4Engine::new(48_000.0);
        o.apply_exact(&mut fresh);
        fresh.organ_a.rotary_enabled = false;
        fresh.organ_a.note_on(60);
        let again: Vec<f32> = (0..4_000)
            .map(|_| fresh.organ_a.render_frame()[0])
            .collect();
        let cont: Vec<f32> = (0..2_000).map(|_| e.organ_a.render_frame()[0]).collect();
        assert_eq!(first, again[..2_000]);
        assert_eq!(cont, again[2_000..]);
    }

    #[test]
    fn summary_lists_active_paths() {
        assert_eq!(FirmwareOptions::default().summary(), "defaults");
        let s = FirmwareOptions::firmware_all().summary();
        assert!(s.contains("tone=Firmware") && s.contains("fx") && !s.contains("legacy-piano"));
        let shaped = FirmwareOptions {
            synth_shape_milli: Some(250),
            ..Default::default()
        };
        assert!(shaped.summary().contains("shape=0.25"));
        assert!(FirmwareOptions::legacy_all()
            .summary()
            .contains("legacy-piano"));
    }

    #[test]
    fn b3_bass_processor_switch_parses_applies_and_describes() {
        let o = FirmwareOptions::from_args(&args(&["--b3-bass-processor", "on"])).unwrap();
        assert_eq!(o.b3_bass_processor, Some(true));
        assert!(o.summary().contains("b3-bass-processor=on"));
        let render = |on: Option<bool>| {
            let mut e = Stage4Engine::new(48_000.0);
            apply_b3_bass_processor(&mut e, on);
            e.organ_a.b3_bass_mode = true;
            e.organ_a.tonewheel_source = crate::organ::ToneWheelSource::B3Wheels;
            e.organ_a.rotary_enabled = false;
            e.organ_a.note_on(48);
            (0..6_000)
                .map(|_| e.organ_a.render_frame()[0])
                .collect::<Vec<f32>>()
        };
        let (off, on) = (render(None), render(Some(true)));
        assert!(on.iter().all(|v| v.is_finite()) && on.iter().any(|v| v.abs() > 1e-5));
        assert_ne!(off, on);
        assert!(parse_on_off("--b3-bass-processor", "x").is_err());
    }

    #[test]
    fn farfisa_processor_switch_parses_applies_and_describes() {
        let o = FirmwareOptions::from_args(&args(&["--farfisa-processor", "on"])).unwrap();
        assert_eq!(o.farfisa_processor, Some(true));
        assert!(o.summary().contains("farfisa-processor=on"));
        let mut e = Stage4Engine::new(48_000.0);
        apply_farfisa_processor(&mut e, None);
        assert!(!e.organ_a.farfisa_processor);
        apply_farfisa_processor(&mut e, Some(true));
        assert!(e.organ_a.farfisa_processor && e.organ_b.farfisa_processor);
        assert!(parse_on_off("--farfisa-processor", "x").is_err());
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
        assert_eq!(
            e.synth_b.chain,
            SynthChain::FirmwareStrip,
            "the D22 default"
        );
        apply_synth_chain(&mut e, None);
        assert_eq!(
            e.synth_b.chain,
            SynthChain::FirmwareStrip,
            "None leaves the engine default"
        );
        apply_synth_chain(&mut e, Some(SynthChain::Legacy));
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

#[cfg(test)]
#[path = "render_opts_ports_tests.rs"]
mod ports_tests;

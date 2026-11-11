//! 2-Layer Piano Engine & Modal Sympathetic String Resonance (`DSPF`).
//!
//! Spec: `docs/specs/piano-engine.md`

use crate::cbin::piano_sample::{decode_4bit_dpcm, NpnoInstrument};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::path::Path;
use std::sync::Arc;

/// Maximum simultaneous polyphonic voices per Piano layer (`24` pre-allocated in core; scalable to 120).
pub const MAX_PIANO_VOICES: usize = 24;

/// Number of chromatic resonant sympathetic strings (12 octave-distributed strings across the keyboard).
pub const NUM_SYMPATHETIC_STRINGS: usize = 12;

/// Piano instrument categories on the Nord Stage 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PianoCategory {
    Grand,
    Upright,
    Electric,
    Clav,
    Digital,
    Misc,
}

/// Keyboard velocity curve (`KBD TOUCH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VelocityTouch {
    Light,
    Medium,
    Heavy,
}

impl VelocityTouch {
    /// Maps incoming MIDI velocity (`1..=127`) to normalized excitation (`0.0..=1.0`).
    pub fn map_velocity(self, velocity: u8) -> f32 {
        let v = f32::from(velocity.clamp(1, 127)) / 127.0;
        match self {
            Self::Light => v.powf(0.65),
            Self::Medium => v,
            Self::Heavy => v.powf(1.55),
        }
    }
}

/// Timbre spectral shaping presets (`PIANO TIMBRE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PianoTimbre {
    Off,
    Soft,
    Mid,
    Bright,
    Dyno1,
    Dyno2,
}

/// Pre-decoded audio stroke waveform ready for lock-free real-time playback.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPianoStroke {
    pub root_key: u8,
    pub sample_rate: f32,
    pub pcm: Arc<[f32]>,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
}

/// Multi-velocity and key-off release zone descriptor for SFZ/WAV multisample instruments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PianoSampleZone {
    pub key_low: u8,
    pub key_high: u8,
    pub vel_low: u8,
    pub vel_high: u8,
    pub root_key: u8,
    pub tune_cents: i8,
    pub trigger_release: bool,
    pub stroke_idx: usize,
}

/// Pre-decoded playable piano instrument loaded from a `.npno` or open SFZ/WAV multisample bank.
#[derive(Debug, Clone, PartialEq)]
pub struct NpnoSound {
    pub name: String,
    pub key_map: [u8; 128],
    pub tuning_cents: [i8; 128],
    pub stroke_by_key: [Option<usize>; 128],
    pub strokes: Vec<LoadedPianoStroke>,
    /// Optional multi-velocity and `trigger=release` zone list (used by SFZ multisamples).
    pub zones: Vec<PianoSampleZone>,
}

impl NpnoSound {
    /// Creates a playable `NpnoSound` from parsed `NpnoInstrument` and raw file bytes.
    pub fn from_npno_bytes(bytes: &[u8]) -> Result<Self, String> {
        let inst = NpnoInstrument::parse(bytes)?;
        let mut strokes = Vec::new();
        for s in &inst.strokes {
            if s.data_offset + s.data_size <= bytes.len() {
                let raw_payload = &bytes[s.data_offset..s.data_offset + s.data_size];
                let pcm = decode_4bit_dpcm(raw_payload, 0.001);
                strokes.push(LoadedPianoStroke {
                    root_key: s.root_key,
                    sample_rate: s.sample_rate as f32,
                    pcm: pcm.into(),
                    loop_start: s.loop_start.map(|x| x as usize),
                    loop_end: s.loop_end.map(|x| x as usize),
                });
            }
        }

        let mut stroke_by_key = [None; 128];
        for (note, slot) in stroke_by_key.iter_mut().enumerate() {
            if let Some(root) = inst.root_key_for_note(note as u8) {
                if let Some(pos) = strokes.iter().position(|st| st.root_key == root) {
                    *slot = Some(pos);
                } else if !strokes.is_empty() {
                    *slot = Some(0);
                }
            }
        }

        Ok(Self {
            name: inst.name,
            key_map: inst.key_map,
            tuning_cents: inst.tuning_cents,
            stroke_by_key,
            strokes,
            zones: Vec::new(),
        })
    }

    /// Creates a synthetic in-memory sound fixture for fast, deterministic testing.
    pub fn new_synthetic(name: &str, root_key: u8, pcm: &[f32], sample_rate: f32) -> Self {
        let key_map = [root_key; 128];
        let tuning_cents = [0i8; 128];
        let stroke = LoadedPianoStroke {
            root_key,
            sample_rate,
            pcm: pcm.to_vec().into(),
            loop_start: None,
            loop_end: None,
        };
        let strokes = vec![stroke];
        let stroke_by_key = [Some(0); 128];
        Self {
            name: name.to_string(),
            key_map,
            tuning_cents,
            stroke_by_key,
            strokes,
            zones: Vec::new(),
        }
    }

    /// Loads an open SFZ instrument from text and a callback that resolves WAV paths into RAM bytes.
    ///
    /// Shared `sample=` paths are decoded once into `Arc<[f32]>` RAM buffers. Supports multi-velocity
    /// (`lovel`/`hivel`), key ranges (`lokey`/`hikey`/`key`/`pitch_keycenter`), fine `tune`,
    /// loops (`loop_start`/`loop_end`), and key-off release samples (`trigger=release`).
    pub fn from_sfz_str<F>(name: &str, sfz_text: &str, mut load_sample: F) -> Result<Self, String>
    where
        F: FnMut(&str) -> Result<Vec<u8>, String>,
    {
        let parsed = parse_sfz_regions(sfz_text);
        if parsed.is_empty() {
            return Err("no SFZ <region> entries found".to_string());
        }

        let mut strokes: Vec<LoadedPianoStroke> = Vec::new();
        let mut zones: Vec<PianoSampleZone> = Vec::new();
        let mut cache: HashMap<(String, u8, Option<usize>, Option<usize>), usize> = HashMap::new();
        let mut key_map = [60u8; 128];
        let mut tuning_cents = [0i8; 128];
        let mut stroke_by_key = [None; 128];

        for reg in parsed {
            let Some(sample_rel) = reg.sample else {
                continue;
            };
            let full_path = if reg.default_path.is_empty() {
                sample_rel.clone()
            } else {
                format!("{}{}", reg.default_path, sample_rel)
            };
            let cache_key = (
                full_path.clone(),
                reg.root_key,
                reg.loop_start,
                reg.loop_end,
            );
            let stroke_idx = if let Some(&idx) = cache.get(&cache_key) {
                idx
            } else {
                let wav_bytes = load_sample(&full_path)?;
                let (sample_rate, pcm) = parse_wav_mono(&wav_bytes)?;
                let idx = strokes.len();
                strokes.push(LoadedPianoStroke {
                    root_key: reg.root_key,
                    sample_rate,
                    pcm: pcm.into(),
                    loop_start: reg.loop_start,
                    loop_end: reg.loop_end,
                });
                cache.insert(cache_key, idx);
                idx
            };

            zones.push(PianoSampleZone {
                key_low: reg.key_low,
                key_high: reg.key_high,
                vel_low: reg.vel_low,
                vel_high: reg.vel_high,
                root_key: reg.root_key,
                tune_cents: reg.tune_cents,
                trigger_release: reg.trigger_release,
                stroke_idx,
            });

            if !reg.trigger_release {
                for note in reg.key_low..=reg.key_high {
                    key_map[note as usize] = reg.root_key;
                    tuning_cents[note as usize] = reg.tune_cents;
                    if stroke_by_key[note as usize].is_none() {
                        stroke_by_key[note as usize] = Some(stroke_idx);
                    }
                }
            }
        }

        if strokes.is_empty() {
            return Err("SFZ regions did not load any audio samples".to_string());
        }

        Ok(Self {
            name: name.to_string(),
            key_map,
            tuning_cents,
            stroke_by_key,
            strokes,
            zones,
        })
    }

    /// Loads an SFZ file and its referenced WAV multisamples from disk into RAM.
    pub fn from_sfz_path(sfz_path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(sfz_path)
            .map_err(|e| format!("failed to read SFZ {}: {e}", sfz_path.display()))?;
        let base_dir = sfz_path.parent().unwrap_or_else(|| Path::new("."));
        let name = sfz_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("SFZ Piano");
        Self::from_sfz_str(name, &text, |rel| {
            let normalized = rel.replace('\\', "/");
            let full = base_dir.join(normalized);
            std::fs::read(&full).map_err(|e| format!("failed to read WAV {}: {e}", full.display()))
        })
    }

    /// Resolves the best `(stroke_idx, tune_cents)` for `(note, velocity, trigger_release)`.
    #[inline]
    pub fn select_stroke(
        &self,
        note: u8,
        velocity: u8,
        trigger_release: bool,
    ) -> Option<(usize, i8)> {
        let vel = velocity.clamp(1, 127);
        if let Some(zone) = self.zones.iter().find(|z| {
            z.trigger_release == trigger_release
                && note >= z.key_low
                && note <= z.key_high
                && vel >= z.vel_low
                && vel <= z.vel_high
        }) {
            return Some((zone.stroke_idx, zone.tune_cents));
        }
        if trigger_release {
            None
        } else {
            let idx = self.stroke_by_key[(note as usize).min(127)]?;
            Some((idx, self.tuning_cents[(note as usize).min(127)]))
        }
    }
}

#[derive(Debug, Clone)]
struct SfzRegionSpec {
    default_path: String,
    sample: Option<String>,
    key_low: u8,
    key_high: u8,
    vel_low: u8,
    vel_high: u8,
    root_key: u8,
    tune_cents: i8,
    trigger_release: bool,
    loop_start: Option<usize>,
    loop_end: Option<usize>,
}

impl Default for SfzRegionSpec {
    fn default() -> Self {
        Self {
            default_path: String::new(),
            sample: None,
            key_low: 0,
            key_high: 127,
            vel_low: 1,
            vel_high: 127,
            root_key: 60,
            tune_cents: 0,
            trigger_release: false,
            loop_start: None,
            loop_end: None,
        }
    }
}

fn parse_sfz_note(raw: &str) -> Option<u8> {
    let trimmed = raw.trim();
    if let Ok(num) = trimmed.parse::<u8>() {
        return Some(num.min(127));
    }
    let lower = trimmed.to_ascii_lowercase();
    let mut chars = lower.chars().peekable();
    let base = match chars.next()? {
        'c' => 0i32,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let acc = match chars.peek().copied() {
        Some('#') => {
            chars.next();
            1
        }
        Some('b') => {
            chars.next();
            -1
        }
        _ => 0,
    };
    let oct_str: String = chars.collect();
    let oct: i32 = oct_str.parse().ok()?;
    let midi = (oct + 1) * 12 + base + acc;
    Some(midi.clamp(0, 127) as u8)
}

fn apply_sfz_opcode(spec: &mut SfzRegionSpec, key: &str, val: &str) {
    match key {
        "default_path" => spec.default_path = val.to_string(),
        "sample" => spec.sample = Some(val.to_string()),
        "key" => {
            if let Some(n) = parse_sfz_note(val) {
                spec.key_low = n;
                spec.key_high = n;
                spec.root_key = n;
            }
        }
        "lokey" => {
            if let Some(n) = parse_sfz_note(val) {
                spec.key_low = n;
            }
        }
        "hikey" => {
            if let Some(n) = parse_sfz_note(val) {
                spec.key_high = n;
            }
        }
        "pitch_keycenter" => {
            if let Some(n) = parse_sfz_note(val) {
                spec.root_key = n;
            }
        }
        "lovel" => {
            if let Ok(v) = val.parse::<u8>() {
                spec.vel_low = v.clamp(1, 127);
            }
        }
        "hivel" => {
            if let Ok(v) = val.parse::<u8>() {
                spec.vel_high = v.clamp(1, 127);
            }
        }
        "tune" => {
            if let Ok(c) = val.parse::<i16>() {
                spec.tune_cents = c.clamp(-128, 127) as i8;
            }
        }
        "trigger" => {
            spec.trigger_release = val.eq_ignore_ascii_case("release");
        }
        "loop_start" => {
            if let Ok(v) = val.parse::<usize>() {
                spec.loop_start = Some(v);
            }
        }
        "loop_end" => {
            if let Ok(v) = val.parse::<usize>() {
                spec.loop_end = Some(v);
            }
        }
        _ => {}
    }
}

fn parse_sfz_regions(text: &str) -> Vec<SfzRegionSpec> {
    let mut regions = Vec::new();
    let mut global_spec = SfzRegionSpec::default();
    let mut group_spec = SfzRegionSpec::default();
    let mut current: Option<SfzRegionSpec> = None;
    let mut in_group = false;
    let mut in_global = false;

    for raw_line in text.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        // Scan tags and opcodes on the line
        let mut rest = line;
        while !rest.is_empty() {
            rest = rest.trim_start();
            if rest.starts_with('<') {
                if let Some(end_tag) = rest.find('>') {
                    let tag = &rest[..=end_tag];
                    if tag.eq_ignore_ascii_case("<global>") || tag.eq_ignore_ascii_case("<control>")
                    {
                        if let Some(r) = current.take() {
                            regions.push(r);
                        }
                        in_global = true;
                        in_group = false;
                    } else if tag.eq_ignore_ascii_case("<group>") {
                        if let Some(r) = current.take() {
                            regions.push(r);
                        }
                        group_spec = global_spec.clone();
                        in_group = true;
                        in_global = false;
                    } else if tag.eq_ignore_ascii_case("<region>") {
                        if let Some(r) = current.take() {
                            regions.push(r);
                        }
                        current = Some(if in_group {
                            group_spec.clone()
                        } else {
                            global_spec.clone()
                        });
                        in_global = false;
                    }
                    rest = &rest[end_tag + 1..];
                    continue;
                }
            }

            if let Some(eq_pos) = rest.find('=') {
                let key = rest[..eq_pos].trim().to_ascii_lowercase();
                let after_eq = &rest[eq_pos + 1..];
                let next_boundary = find_next_sfz_token(after_eq);
                let val = after_eq[..next_boundary].trim();
                if let Some(ref mut reg) = current {
                    apply_sfz_opcode(reg, &key, val);
                } else if in_group {
                    apply_sfz_opcode(&mut group_spec, &key, val);
                } else if in_global {
                    apply_sfz_opcode(&mut global_spec, &key, val);
                }
                rest = &after_eq[next_boundary..];
            } else {
                break;
            }
        }
    }
    if let Some(r) = current {
        regions.push(r);
    }
    regions
}

fn find_next_sfz_token(after_eq: &str) -> usize {
    let bytes = after_eq.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            return i;
        }
        if bytes[i].is_ascii_whitespace() {
            let tail = after_eq[i..].trim_start();
            if tail.is_empty() || tail.starts_with('<') {
                return i;
            }
            if let Some(next_space_or_eq) = tail.find(['=', ' ', '\t']) {
                if tail.as_bytes()[next_space_or_eq] == b'=' {
                    return i;
                }
            }
        }
        i += 1;
    }
    after_eq.len()
}

/// Clean-room RIFF/WAVE parser decoding 8/16/24-bit integer PCM or 32-bit IEEE float into mono `f32`.
pub fn parse_wav_mono(bytes: &[u8]) -> Result<(f32, Vec<f32>), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("invalid RIFF/WAVE header".to_string());
    }
    let mut pos = 12usize;
    let mut format_tag = 0u16;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut data_slice: Option<&[u8]> = None;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let payload_start = pos + 8;
        let payload_end = payload_start.saturating_add(size).min(bytes.len());
        let chunk = &bytes[payload_start..payload_end];

        if id == b"fmt " && chunk.len() >= 16 {
            format_tag = u16::from_le_bytes(chunk[0..2].try_into().unwrap());
            channels = u16::from_le_bytes(chunk[2..4].try_into().unwrap());
            sample_rate = u32::from_le_bytes(chunk[4..8].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(chunk[14..16].try_into().unwrap());
        } else if id == b"data" {
            data_slice = Some(chunk);
        }

        let padded = size + (size & 1);
        pos = payload_start.saturating_add(padded);
    }

    let data = data_slice.ok_or_else(|| "WAV missing data chunk".to_string())?;
    if channels == 0 || sample_rate == 0 {
        return Err("WAV missing valid fmt chunk".to_string());
    }
    let ch = channels as usize;
    let bytes_per_sample = (bits_per_sample as usize) / 8;
    if bytes_per_sample == 0 {
        return Err("invalid WAV bit depth".to_string());
    }
    let frame_bytes = ch * bytes_per_sample;
    let frame_count = data.len() / frame_bytes;
    let mut out = Vec::with_capacity(frame_count);

    for frame in 0..frame_count {
        let mut acc = 0.0f32;
        for c in 0..ch {
            let off = frame * frame_bytes + c * bytes_per_sample;
            let sample = match (format_tag, bits_per_sample) {
                (1, 8) => (f32::from(data[off]) - 128.0) / 128.0,
                (1, 16) => {
                    let s = i16::from_le_bytes([data[off], data[off + 1]]);
                    f32::from(s) / 32768.0
                }
                (1, 24) => {
                    let raw = (i32::from(data[off]))
                        | (i32::from(data[off + 1]) << 8)
                        | (i32::from(data[off + 2] as i8) << 16);
                    (raw as f32) / 8_388_608.0
                }
                (3, 32) => {
                    f32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
                }
                _ => {
                    return Err(format!(
                        "unsupported WAV format tag {format_tag} / {bits_per_sample}-bit"
                    ));
                }
            };
            acc += sample;
        }
        out.push((acc / ch as f32).clamp(-1.0, 1.0));
    }

    Ok((sample_rate as f32, out))
}

#[derive(Debug, Clone, Copy, Default)]
struct PianoVoice {
    active: bool,
    sustained: bool,
    sostenuto_held: bool,
    releasing: bool,
    soft_release: bool,
    is_release_sample: bool,
    note: u8,
    last_velocity: u8,
    freq: f32,
    phase: f32,
    phase_detune: f32,
    amp: f32,
    env: f32,
    gain: f32,
    stroke_idx: Option<usize>,
    sample_pos: f64,
    sample_pos_detune: f64,
    sample_step: f64,
    sample_step_detune: f64,
}

/// 2-pole modal harmonic string resonator for Advanced String Resonance.
#[derive(Debug, Clone, Copy, Default)]
struct ModalResonator {
    y1: f32,
    y2: f32,
    cos_w: f32,
    sin_w: f32,
    r: f32,
}

impl ModalResonator {
    fn configure(&mut self, freq_hz: f32, t60_sec: f32, sample_rate: f32) {
        let w = TAU * (freq_hz / sample_rate).clamp(0.001, 0.45);
        self.cos_w = w.cos();
        self.sin_w = w.sin();
        self.r = (-6.907755 / (t60_sec * sample_rate))
            .exp()
            .clamp(0.9, 0.99995);
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = (1.0 - self.r * self.r) * self.sin_w * x + 2.0 * self.r * self.cos_w * self.y1
            - self.r * self.r * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Single Piano Layer (`Piano A` or `Piano B`) with modal sympathetic string resonance and pedal noise.
#[derive(Debug, Clone)]
pub struct PianoLayer {
    pub sample_rate: f32,
    pub category: PianoCategory,
    pub touch: VelocityTouch,
    pub timbre: PianoTimbre,
    pub string_resonance_enabled: bool,
    pub pedal_noise_enabled: bool,
    pub soft_release: bool,
    /// Piano Unison hammer/string detune spread (`0` = Off, `1..=3`).
    pub unison: u8,
    /// Piano Dynamic Range Compressor (`0` = Off, `1..=3`).
    pub dyn_comp: u8,
    /// Keyboard octave transposition (`-2..=+2`).
    pub octave_shift: i8,
    pub damper_pedal_down: bool,
    pub sostenuto_pedal_down: bool,
    pub soft_pedal_down: bool,
    pub sound: Option<Arc<NpnoSound>>,
    voices: [PianoVoice; MAX_PIANO_VOICES],
    resonators: [ModalResonator; NUM_SYMPATHETIC_STRINGS],
    undamped_keys: [bool; 128],
    pedal_noise_env: f32,
    pedal_brush_env: f32,
    pedal_cutoff_hz: f32,
    pedal_lp_state: f32,
    rng_state: u32,
    timbre_lp: f32,
    timbre_bp_state: [f32; 2],
    timbre_low_lp: f32,
    dyn_comp_env: f32,
}

impl PianoLayer {
    #[cfg(test)]
    pub(crate) fn voice_excitation_for_test(&self, note: u8) -> Option<(f32, f32, bool)> {
        self.voices
            .iter()
            .find(|v| v.active && !v.is_release_sample && v.note == note)
            .map(|v| (v.amp, v.gain, v.releasing))
    }

    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate.max(8000.0);
        let mut resonators = [ModalResonator::default(); NUM_SYMPATHETIC_STRINGS];
        // Tune 12 sympathetic resonance modes across the keyboard (C2 up to B7)
        for (i, res) in resonators.iter_mut().enumerate() {
            let midi_note = 24.0 + (i as f32) * 7.0;
            let f = 440.0 * 2.0f32.powf((midi_note - 69.0) / 12.0);
            let t60 = (5.0 - 0.15 * i as f32).clamp(1.5, 5.0);
            res.configure(f, t60, fs);
        }
        let pcm_len = (fs * 1.5) as usize;
        let mut pcm = Vec::with_capacity(pcm_len);
        for n in 0..pcm_len {
            let t = n as f32 / fs;
            let decay = (-3.0 * t).exp();
            let sample = ((2.0 * std::f32::consts::PI * 261.63 * t).sin()
                + 0.5 * (2.0 * std::f32::consts::PI * 523.25 * t).sin()
                + 0.25 * (2.0 * std::f32::consts::PI * 784.88 * t).sin()
                + 0.125 * (2.0 * std::f32::consts::PI * 1046.50 * t).sin())
                * decay
                * 0.4;
            pcm.push(sample);
        }
        let sound = Some(Arc::new(NpnoSound::new_synthetic(
            "Default Synthetic Grand",
            60,
            &pcm,
            fs,
        )));

        Self {
            sample_rate: fs,
            category: PianoCategory::Grand,
            touch: VelocityTouch::Medium,
            timbre: PianoTimbre::Off,
            string_resonance_enabled: true,
            pedal_noise_enabled: true,
            soft_release: false,
            unison: 0,
            dyn_comp: 0,
            octave_shift: 0,
            damper_pedal_down: false,
            sostenuto_pedal_down: false,
            soft_pedal_down: false,
            sound,
            voices: [PianoVoice::default(); MAX_PIANO_VOICES],
            resonators,
            undamped_keys: [false; 128],
            pedal_noise_env: 0.0,
            pedal_brush_env: 0.0,
            pedal_cutoff_hz: 140.0,
            pedal_lp_state: 0.0,
            rng_state: 0xA511_E9B3,
            timbre_lp: 0.0,
            timbre_bp_state: [0.0; 2],
            timbre_low_lp: 0.0,
            dyn_comp_env: 0.0,
        }
    }

    /// Assigns or unloads a pre-decoded `.npno` or SFZ sound library.
    pub fn set_sound(&mut self, sound: Option<Arc<NpnoSound>>) {
        self.sound = sound;
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.note_on_with_gain(note, velocity, 1.0);
    }

    fn allocate_voice_slot(&self, match_note: Option<u8>) -> usize {
        if let Some(n) = match_note {
            if let Some(pos) = self
                .voices
                .iter()
                .position(|v| v.active && !v.is_release_sample && v.note == n)
            {
                return pos;
            }
        }
        if let Some(pos) = self.voices.iter().position(|v| !v.active) {
            return pos;
        }
        let mut best = 0;
        let mut min_score = f32::MAX;
        for (i, v) in self.voices.iter().enumerate() {
            let mut score = if v.releasing { v.env * 0.1 } else { v.env };
            if v.is_release_sample {
                score *= 0.05;
            }
            if score < min_score {
                min_score = score;
                best = i;
            }
        }
        best
    }

    pub fn note_on_with_gain(&mut self, note: u8, velocity: u8, gain: f32) {
        let base_amp = self.touch.map_velocity(velocity);
        let compressed_amp = match self.dyn_comp {
            1 => base_amp.powf(0.82),
            2 => base_amp.powf(0.68),
            3 => base_amp.powf(0.55),
            _ => base_amp,
        };
        let una_corda_scale = if self.soft_pedal_down { 0.76 } else { 1.0 };
        let amp = (compressed_amp * una_corda_scale).clamp(0.0, 1.0);

        let effective_note =
            (i32::from(note) + i32::from(self.octave_shift.clamp(-2, 2)) * 12).clamp(0, 127) as u8;
        let freq = 440.0 * 2.0f32.powf((f32::from(effective_note) - 69.0) / 12.0);
        let note_idx = (note as usize).min(127);
        self.undamped_keys[note_idx] = true;

        let unison_cents = match self.unison {
            1 => 3.5,
            2 => 7.0,
            3 => 11.5,
            _ => 0.0,
        };
        let detune_ratio = 2.0f64.powf(unison_cents / 1200.0);

        let (stroke_idx, sample_step) = if let Some(snd) = &self.sound {
            if let Some((idx, cents)) = snd.select_stroke(effective_note, velocity, false) {
                if let Some(st) = snd.strokes.get(idx) {
                    let root = st.root_key;
                    let semitones =
                        (f32::from(effective_note) - f32::from(root)) + (f32::from(cents) / 100.0);
                    let pitch_ratio = 2.0f32.powf(semitones / 12.0);
                    let step = (pitch_ratio as f64)
                        * ((st.sample_rate as f64) / (self.sample_rate as f64));
                    (Some(idx), step)
                } else {
                    (None, 0.0)
                }
            } else {
                (None, 0.0)
            }
        } else {
            (None, 0.0)
        };

        let idx = self.allocate_voice_slot(Some(note));
        self.voices[idx] = PianoVoice {
            active: true,
            sustained: false,
            sostenuto_held: false,
            releasing: false,
            soft_release: false,
            is_release_sample: false,
            note,
            last_velocity: velocity,
            freq,
            phase: 0.0,
            phase_detune: 0.0,
            amp,
            env: 1.0,
            gain: gain.clamp(0.0, 1.0),
            stroke_idx,
            sample_pos: 0.0,
            sample_pos_detune: 0.0,
            sample_step,
            sample_step_detune: sample_step * detune_ratio,
        };
    }

    fn spawn_release_sample(&mut self, note: u8, velocity: u8, amp: f32, gain: f32) {
        let effective_note =
            (i32::from(note) + i32::from(self.octave_shift.clamp(-2, 2)) * 12).clamp(0, 127) as u8;
        let Some((rel_idx, cents, st_sr, root)) = self.sound.as_ref().and_then(|snd| {
            let (idx, c) = snd.select_stroke(effective_note, velocity, true)?;
            let st = snd.strokes.get(idx)?;
            Some((idx, c, st.sample_rate, st.root_key))
        }) else {
            return;
        };

        let semitones = (f32::from(effective_note) - f32::from(root)) + (f32::from(cents) / 100.0);
        let pitch_ratio = 2.0f32.powf(semitones / 12.0);
        let step = (pitch_ratio as f64) * ((st_sr as f64) / (self.sample_rate as f64));
        let slot = self.allocate_voice_slot(None);
        let rel_scale = if self.soft_release { 0.45 } else { 0.30 };
        self.voices[slot] = PianoVoice {
            active: true,
            sustained: false,
            sostenuto_held: false,
            releasing: true,
            soft_release: self.soft_release,
            is_release_sample: true,
            note,
            last_velocity: velocity,
            freq: 0.0,
            phase: 0.0,
            phase_detune: 0.0,
            amp: amp * rel_scale,
            env: 1.0,
            gain,
            stroke_idx: Some(rel_idx),
            sample_pos: 0.0,
            sample_pos_detune: 0.0,
            sample_step: step,
            sample_step_detune: step,
        };
    }

    pub fn note_off(&mut self, note: u8) {
        let note_idx = (note as usize).min(127);
        self.undamped_keys[note_idx] = false;
        let mut release_trigger = None;
        for v in &mut self.voices {
            if v.active && !v.is_release_sample && v.note == note {
                if self.damper_pedal_down {
                    v.sustained = true;
                } else if v.sostenuto_held {
                    // Sostenuto holds the note until CC66 lifts.
                } else if !v.releasing {
                    v.releasing = true;
                    v.soft_release = self.soft_release;
                    release_trigger = Some((v.last_velocity, v.amp, v.gain));
                }
            }
        }
        if let Some((vel, amp, gain)) = release_trigger {
            self.spawn_release_sample(note, vel, amp, gain);
        }
    }

    /// Updates Damper (`CC64`) sustain pedal state and velocity-sensitive pedal noise.
    pub fn set_damper_pedal(&mut self, down: bool, velocity: u8) {
        if down == self.damper_pedal_down {
            return;
        }
        self.damper_pedal_down = down;
        if self.pedal_noise_enabled {
            let v_norm = f32::from(velocity.max(24)) / 127.0;
            if down {
                self.pedal_noise_env = v_norm * 0.14;
                self.pedal_brush_env = v_norm * 0.08;
                self.pedal_cutoff_hz = 140.0;
            } else {
                self.pedal_noise_env = v_norm * 0.16;
                self.pedal_brush_env = 0.0;
                self.pedal_cutoff_hz = 95.0;
            }
        }
        if !down {
            let mut released_notes: [(u8, u8, f32, f32); MAX_PIANO_VOICES] =
                [(0, 0, 0.0, 0.0); MAX_PIANO_VOICES];
            let mut rel_count = 0usize;
            for v in &mut self.voices {
                if v.sustained {
                    v.sustained = false;
                    if !v.sostenuto_held && !v.releasing {
                        v.releasing = true;
                        v.soft_release = self.soft_release;
                        if rel_count < MAX_PIANO_VOICES {
                            released_notes[rel_count] = (v.note, v.last_velocity, v.amp, v.gain);
                            rel_count += 1;
                        }
                    }
                }
            }
            for &(note, vel, amp, gain) in &released_notes[..rel_count] {
                self.spawn_release_sample(note, vel, amp, gain);
            }
        }
    }

    /// Updates Sostenuto (`CC66`) pedal state, capturing active notes and triggering pedal noise.
    pub fn set_sostenuto_pedal(&mut self, down: bool, velocity: u8) {
        if down == self.sostenuto_pedal_down {
            return;
        }
        self.sostenuto_pedal_down = down;
        if self.pedal_noise_enabled {
            let v_norm = f32::from(velocity.max(24)) / 127.0;
            self.pedal_noise_env = self.pedal_noise_env.max(v_norm * 0.10);
            self.pedal_cutoff_hz = 120.0;
        }
        if down {
            for v in &mut self.voices {
                if v.active && !v.is_release_sample && !v.releasing {
                    v.sostenuto_held = true;
                }
            }
        } else {
            let mut released_notes: [(u8, u8, f32, f32); MAX_PIANO_VOICES] =
                [(0, 0, 0.0, 0.0); MAX_PIANO_VOICES];
            let mut rel_count = 0usize;
            for v in &mut self.voices {
                if v.sostenuto_held {
                    v.sostenuto_held = false;
                    let key_down = self.undamped_keys[(v.note as usize).min(127)];
                    if !key_down && !self.damper_pedal_down && !v.releasing {
                        v.releasing = true;
                        v.soft_release = self.soft_release;
                        if rel_count < MAX_PIANO_VOICES {
                            released_notes[rel_count] = (v.note, v.last_velocity, v.amp, v.gain);
                            rel_count += 1;
                        }
                    }
                }
            }
            for &(note, vel, amp, gain) in &released_notes[..rel_count] {
                self.spawn_release_sample(note, vel, amp, gain);
            }
        }
    }

    /// Updates Soft / Una Corda (`CC67`) pedal state and hammer-rail shift pedal noise.
    pub fn set_soft_pedal(&mut self, down: bool, velocity: u8) {
        if down == self.soft_pedal_down {
            return;
        }
        self.soft_pedal_down = down;
        if self.pedal_noise_enabled {
            let v_norm = f32::from(velocity.max(24)) / 127.0;
            self.pedal_noise_env = self.pedal_noise_env.max(v_norm * 0.085);
            self.pedal_cutoff_hz = 160.0;
        }
    }

    #[inline]
    fn sample_stroke_at(stroke: &LoadedPianoStroke, pos: &mut f64, step: f64) -> Option<f32> {
        let len = stroke.pcm.len();
        let idx0 = *pos as usize;
        if idx0 + 1 < len {
            let frac = (*pos - idx0 as f64) as f32;
            let s0 = stroke.pcm[idx0];
            let s1 = stroke.pcm[idx0 + 1];
            *pos += step;
            Some(s0 + frac * (s1 - s0))
        } else if idx0 < len {
            *pos += step;
            Some(stroke.pcm[idx0])
        } else {
            None
        }
    }

    #[inline]
    fn next_noise(&mut self) -> f32 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        ((x as i32) as f32) * (1.0 / i32::MAX as f32)
    }

    fn process_timbre(&mut self, sum_l: f32, sum_r: f32, inv_fs: f32) -> [f32; 2] {
        let mono = 0.5 * (sum_l + sum_r);
        let lp_hz = match self.timbre {
            PianoTimbre::Soft => 1800.0,
            PianoTimbre::Bright => 2500.0,
            _ => 3800.0,
        };
        let lp_a = (TAU * lp_hz * inv_fs).clamp(0.02, 0.75);
        self.timbre_lp += lp_a * (mono - self.timbre_lp);
        let high = mono - self.timbre_lp;

        // Low-warmth shelf (120 Hz) for Dyno preamps
        let low_a = (TAU * 120.0 * inv_fs).clamp(0.005, 0.25);
        self.timbre_low_lp += low_a * (mono - self.timbre_low_lp);

        // Resonant midrange presence bell (1.2 kHz for Mid, 800 Hz scoop for Dyno2)
        let bell_hz = if self.timbre == PianoTimbre::Dyno2 {
            800.0
        } else {
            1200.0
        };
        let w = (TAU * bell_hz * inv_fs).clamp(0.01, 0.45);
        self.timbre_bp_state[0] +=
            w * (mono - self.timbre_bp_state[0] - 0.65 * self.timbre_bp_state[1]);
        self.timbre_bp_state[1] += w * self.timbre_bp_state[0];
        let mid_bell = self.timbre_bp_state[0];

        let delta = match self.timbre {
            PianoTimbre::Off => 0.0,
            PianoTimbre::Soft => -0.50 * high,
            PianoTimbre::Mid => 0.50 * mid_bell,
            PianoTimbre::Bright => 0.65 * high,
            PianoTimbre::Dyno1 => 0.42 * self.timbre_low_lp + 0.75 * high,
            PianoTimbre::Dyno2 => 0.45 * self.timbre_low_lp - 0.35 * mid_bell + 0.90 * high,
        };
        [sum_l + delta, sum_r + delta]
    }

    pub fn render_frame(&mut self) -> [f32; 2] {
        let inv_fs = 1.0 / self.sample_rate;
        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        let unison_on = self.unison > 0;
        let unison_detune_ratio = match self.unison {
            1 => 2.0f32.powf(3.5 / 1200.0),
            2 => 2.0f32.powf(7.0 / 1200.0),
            3 => 2.0f32.powf(11.5 / 1200.0),
            _ => 1.0,
        };

        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let (sig_l, sig_r) = if let (Some(snd), Some(s_idx)) = (&self.sound, v.stroke_idx) {
                if let Some(stroke) = snd.strokes.get(s_idx) {
                    let main_s = Self::sample_stroke_at(stroke, &mut v.sample_pos, v.sample_step);
                    if let Some(m) = main_s {
                        if unison_on && !v.is_release_sample {
                            let d = Self::sample_stroke_at(
                                stroke,
                                &mut v.sample_pos_detune,
                                v.sample_step_detune,
                            )
                            .unwrap_or(0.0);
                            (0.75 * m + 0.25 * d, 0.25 * m + 0.75 * d)
                        } else {
                            (m, m)
                        }
                    } else {
                        v.active = false;
                        (0.0, 0.0)
                    }
                } else {
                    let p = v.phase;
                    v.phase = (p + TAU * v.freq * inv_fs) % TAU;
                    let m = p.sin()
                        + 0.45 * v.amp * (2.0 * p).sin()
                        + 0.20 * v.amp * v.amp * (3.0 * p).sin();
                    (m, m)
                }
            } else {
                // Harmonic piano wave model (fundamental + 2nd + 3rd partials with velocity brightness)
                let p = v.phase;
                v.phase = (p + TAU * v.freq * inv_fs) % TAU;
                let m = p.sin()
                    + 0.45 * v.amp * (2.0 * p).sin()
                    + 0.20 * v.amp * v.amp * (3.0 * p).sin();
                if unison_on && !v.is_release_sample {
                    let pd = v.phase_detune;
                    v.phase_detune = (pd + TAU * v.freq * unison_detune_ratio * inv_fs) % TAU;
                    let d = pd.sin() + 0.45 * v.amp * (2.0 * pd).sin();
                    (0.75 * m + 0.25 * d, 0.25 * m + 0.75 * d)
                } else {
                    (m, m)
                }
            };

            let voice_gain = v.amp * v.env * v.gain * 0.2;
            sum_l += sig_l * voice_gain;
            sum_r += sig_r * voice_gain;

            let decay_time = if v.is_release_sample {
                0.30
            } else if v.releasing {
                if v.soft_release {
                    0.180
                } else {
                    0.080
                }
            } else {
                1.8
            };
            v.env *= (-inv_fs / decay_time).exp();
            if v.env < 1e-4 {
                v.active = false;
                v.releasing = false;
                v.sostenuto_held = false;
            }
        }

        // Dynamic pedal thump + felt brush transient (CC64/CC66/CC67)
        let mut brush_excite = 0.0f32;
        if self.pedal_noise_env > 1e-5 || self.pedal_brush_env > 1e-5 {
            let noise = self.next_noise();
            let raw_thump = (self.pedal_noise_env * 280.0).sin() * self.pedal_noise_env
                + noise * self.pedal_brush_env;
            let lp_a = (TAU * self.pedal_cutoff_hz * inv_fs).clamp(0.005, 0.35);
            self.pedal_lp_state += lp_a * (raw_thump - self.pedal_lp_state);
            sum_l += self.pedal_lp_state;
            sum_r += self.pedal_lp_state;
            brush_excite = self.pedal_brush_env * 0.25 * noise;
            self.pedal_noise_env *= (-inv_fs / 0.055).exp();
            self.pedal_brush_env *= (-inv_fs / 0.035).exp();
        }

        // Timbre spectral contour (Soft, Mid, Bright, Dyno1, Dyno2)
        let [mut shaped_l, mut shaped_r] = self.process_timbre(sum_l, sum_r, inv_fs);

        // Una Corda subtle hammer warmth when soft_pedal_down is active
        if self.soft_pedal_down {
            shaped_l *= 0.90;
            shaped_r *= 0.90;
        }

        // Dynamic Range Compression (`DYN COMP` 1, 2, 3)
        if self.dyn_comp > 0 {
            let peak = shaped_l.abs().max(shaped_r.abs());
            let atk_tau = 0.004;
            let rel_tau = 0.090;
            let coeff = if peak > self.dyn_comp_env {
                (inv_fs / atk_tau).min(1.0)
            } else {
                (inv_fs / rel_tau).min(1.0)
            };
            self.dyn_comp_env += coeff * (peak - self.dyn_comp_env);
            let amt = f32::from(self.dyn_comp.min(3)) / 3.0;
            let comp_gain = if self.dyn_comp_env > 0.12 {
                1.0 / (1.0 + amt * 2.8 * (self.dyn_comp_env - 0.12))
            } else {
                1.0 + 0.35 * amt
            };
            shaped_l *= comp_gain;
            shaped_r *= comp_gain;
        }

        // Sympathetic string resonance: fires either across all 12 strings when damper pedal is down,
        // or specifically across undamped held keys (Nord Advanced String Resonance Gen 2)
        let shaped_mono = 0.5 * (shaped_l + shaped_r) + brush_excite;
        let mut res_sum = 0.0f32;
        if self.string_resonance_enabled {
            if self.damper_pedal_down {
                for r in &mut self.resonators {
                    res_sum += r.process(shaped_mono * 0.008);
                }
            } else {
                for (i, r) in self.resonators.iter_mut().enumerate() {
                    let midi_note = 24 + i * 7;
                    if midi_note < 128 && self.undamped_keys[midi_note] {
                        res_sum += r.process(shaped_mono * 0.015);
                    }
                }
            }
        }

        [shaped_l + res_sum, shaped_r - res_sum * 0.5]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_wav_i16(sample_rate: u32, freq_hz: f32, frames: usize) -> Vec<u8> {
        let data_bytes = (frames * 2) as u32;
        let riff_size = 36 + data_bytes;
        let mut out = Vec::with_capacity(44 + frames * 2);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&riff_size.to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        out.extend_from_slice(&2u16.to_le_bytes()); // block align
        out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_bytes.to_le_bytes());
        for i in 0..frames {
            let s = ((i as f32 / sample_rate as f32) * freq_hz * TAU).sin() * 0.7;
            let q = (s * 32767.0).round() as i16;
            out.extend_from_slice(&q.to_le_bytes());
        }
        out
    }

    #[test]
    fn test_velocity_touch_ordering() {
        let light = VelocityTouch::Light.map_velocity(64);
        let medium = VelocityTouch::Medium.map_velocity(64);
        let heavy = VelocityTouch::Heavy.map_velocity(64);
        assert!(heavy < medium && medium < light);
    }

    #[test]
    fn test_sfz_wav_multivelocity_and_release_loader() {
        let sfz = r#"
<group> lokey=c4 hikey=b4 pitch_keycenter=60
<region> sample=soft.wav lovel=1 hivel=63
<region> sample=loud.wav lovel=64 hivel=127
<region> sample=rel.wav trigger=release lovel=1 hivel=127
"#;
        let wav_soft = build_test_wav_i16(48_000, 261.63, 4096);
        let wav_loud = build_test_wav_i16(48_000, 523.25, 4096);
        let wav_rel = build_test_wav_i16(48_000, 180.0, 2048);

        let sound = Arc::new(
            NpnoSound::from_sfz_str("Test SFZ Piano", sfz, |name| match name {
                "soft.wav" => Ok(wav_soft.clone()),
                "loud.wav" => Ok(wav_loud.clone()),
                "rel.wav" => Ok(wav_rel.clone()),
                other => Err(format!("missing fixture {other}")),
            })
            .expect("SFZ parses cleanly"),
        );

        assert_eq!(sound.strokes.len(), 3);
        let (soft_idx, _) = sound.select_stroke(60, 40, false).unwrap();
        let (loud_idx, _) = sound.select_stroke(60, 110, false).unwrap();
        let (rel_idx, _) = sound.select_stroke(60, 110, true).unwrap();
        assert_ne!(soft_idx, loud_idx);
        assert_ne!(loud_idx, rel_idx);

        let mut piano = PianoLayer::new(48_000.0);
        piano.set_sound(Some(sound));
        piano.note_on(60, 110);
        assert_eq!(piano.voices[0].stroke_idx, Some(loud_idx));
        piano.note_off(60);
        // Key-off release sample voice is spawned cleanly in pre-allocated voice pool
        assert!(piano
            .voices
            .iter()
            .any(|v| v.active && v.is_release_sample && v.stroke_idx == Some(rel_idx)));
    }

    #[test]
    fn test_sostenuto_soft_pedal_noise_unison_and_dyn_comp() {
        // 1. Sostenuto holds notes active at pedal-down, not notes pressed afterward
        let mut piano = PianoLayer::new(48_000.0);
        piano.pedal_noise_enabled = true;
        piano.note_on(60, 100);
        piano.set_sostenuto_pedal(true, 110);
        assert!(piano.pedal_noise_env > 0.05);
        piano.note_on(64, 100);
        piano.note_off(60);
        piano.note_off(64);
        assert!(!piano.voice_excitation_for_test(60).unwrap().2); // sustained by CC66
        assert!(piano.voice_excitation_for_test(64).unwrap().2); // releasing normally
        piano.set_sostenuto_pedal(false, 80);
        assert!(piano.voice_excitation_for_test(60).unwrap().2);

        // 2. Soft pedal (CC67) reduces strike amplitude and triggers pedal noise
        let mut soft = PianoLayer::new(48_000.0);
        soft.set_soft_pedal(true, 90);
        assert!(soft.pedal_noise_env > 0.04);
        soft.note_on(60, 100);
        let soft_amp = soft.voice_excitation_for_test(60).unwrap().0;
        let mut normal = PianoLayer::new(48_000.0);
        normal.note_on(60, 100);
        let normal_amp = normal.voice_excitation_for_test(60).unwrap().0;
        assert!(soft_amp < normal_amp);

        // 3. Unison stereo hammer detune creates stereo difference L != R even without string res
        let mut uni = PianoLayer::new(48_000.0);
        uni.string_resonance_enabled = false;
        uni.unison = 3;
        uni.note_on(60, 110);
        let mut lr_diff = 0.0f32;
        for _ in 0..512 {
            let [l, r] = uni.render_frame();
            lr_diff += (l - r).abs();
        }
        assert!(lr_diff > 0.01, "piano unison must create stereo width");

        // 4. DYN COMP raises quiet note strike level relative to uncompressed
        let mut comp = PianoLayer::new(48_000.0);
        comp.dyn_comp = 3;
        comp.note_on(60, 32);
        let comp_amp = comp.voice_excitation_for_test(60).unwrap().0;
        let mut raw = PianoLayer::new(48_000.0);
        raw.note_on(60, 32);
        let raw_amp = raw.voice_excitation_for_test(60).unwrap().0;
        assert!(comp_amp > raw_amp * 1.4);
    }

    #[test]
    fn test_piano_string_resonance_and_sustain_pedal() {
        let mut piano = PianoLayer::new(48_000.0);
        piano.set_damper_pedal(true, 100);
        piano.note_on(60, 110);
        piano.note_off(60);
        // Voice stays sustained while damper pedal is down
        let mut energy = 0.0f32;
        for _ in 0..256 {
            let [l, r] = piano.render_frame();
            assert!(l.is_finite() && r.is_finite());
            energy += l * l + r * r;
        }
        assert!(energy > 0.005);

        piano.set_damper_pedal(false, 64);
        // Releasing pedal triggers smooth felt damper decay; active during decay
        assert!(piano.voices.iter().any(|v| v.active && v.releasing));

        // After decay period (~0.8s = 40,000 samples at 48kHz), all voices have decayed
        for _ in 0..50_000 {
            piano.render_frame();
        }
        assert!(piano.voices.iter().all(|v| !v.active));
    }

    #[test]
    fn note_off_starts_release_instead_of_cutting_voice() {
        let mut piano = PianoLayer::new(48_000.0);
        piano.note_on(60, 100);
        piano.note_off(60);
        assert!(piano.voices.iter().any(|v| v.active && v.releasing));
        for _ in 0..50_000 {
            piano.render_frame();
        }
        assert!(piano.voices.iter().all(|v| !v.active));
    }

    #[test]
    fn test_piano_exponential_damping_and_soft_release() {
        let mut piano_normal = PianoLayer::new(48_000.0);
        piano_normal.soft_release = false;
        piano_normal.note_on(60, 100);
        piano_normal.note_off(60);

        let mut piano_soft = PianoLayer::new(48_000.0);
        piano_soft.soft_release = true;
        piano_soft.note_on(60, 100);
        piano_soft.note_off(60);

        // Render 2000 samples (~42ms)
        for _ in 0..2000 {
            piano_normal.render_frame();
            piano_soft.render_frame();
        }

        let env_normal = piano_normal.voices[0].env;
        let env_soft = piano_soft.voices[0].env;

        // Soft release felt descends gentler, preserving more string energy
        assert!(env_soft > env_normal);
        assert!(env_normal > 0.0);
        assert!(piano_normal.voices[0].releasing);
        assert!(piano_soft.voices[0].releasing);
    }

    #[test]
    fn test_npno_multisample_stroke_playback_and_pitch_transposition() {
        let mut piano = PianoLayer::new(48_000.0);
        // Generate a 1-second 440 Hz test tone at root key 69 (A4)
        let sample_rate = 48_000.0;
        let mut pcm = vec![0.0f32; 48_000];
        for (i, sample) in pcm.iter_mut().enumerate() {
            let t = i as f32 / sample_rate;
            *sample = (t * 440.0 * TAU).sin();
        }

        let sound = Arc::new(NpnoSound::new_synthetic(
            "Test Grand",
            69,
            &pcm,
            sample_rate,
        ));
        piano.set_sound(Some(sound));

        // Note 69 (A4, matching root key) -> pitch_ratio = 1.0
        piano.note_on(69, 100);
        let v0 = piano.voices[0];
        assert_eq!(v0.stroke_idx, Some(0));
        assert!((v0.sample_step - 1.0).abs() < 1e-5);

        // Note 81 (A5, +12 semitones = 1 octave up) -> pitch_ratio = 2.0
        piano.note_on(81, 100);
        let v1 = piano.voices[1];
        assert_eq!(v1.stroke_idx, Some(0));
        assert!((v1.sample_step - 2.0).abs() < 1e-4);

        // Note 57 (A3, -12 semitones = 1 octave down) -> pitch_ratio = 0.5
        piano.note_on(57, 100);
        let v2 = piano.voices[2];
        assert_eq!(v2.stroke_idx, Some(0));
        assert!((v2.sample_step - 0.5).abs() < 1e-4);

        // Render frames and verify linear interpolation and energy
        let mut energy = 0.0f32;
        for _ in 0..512 {
            let [l, r] = piano.render_frame();
            assert!(l.is_finite() && r.is_finite());
            energy += l * l + r * r;
        }
        assert!(energy > 0.01);
    }

    #[test]
    fn test_piano_voice_stealing_priority() {
        let mut piano = PianoLayer::new(48_000.0);

        // Fill all 24 polyphonic voices with active notes
        for note in 40..64u8 {
            piano.note_on(note, 90);
        }
        assert!(piano.voices.iter().all(|v| v.active));

        // Release note 45 so it enters releasing state
        piano.note_off(45);
        let idx_releasing = piano.voices.iter().position(|v| v.note == 45).unwrap();
        assert!(piano.voices[idx_releasing].releasing);

        // Advance 200 samples so releasing envelope drops
        for _ in 0..200 {
            piano.render_frame();
        }

        // Trigger note 70 (25th voice, exceeding MAX_PIANO_VOICES = 24)
        piano.note_on(70, 100);

        // The voice allocator must steal the decaying releasing voice (note 45)
        assert_eq!(piano.voices[idx_releasing].note, 70);
        assert!(!piano.voices[idx_releasing].releasing);
    }
}

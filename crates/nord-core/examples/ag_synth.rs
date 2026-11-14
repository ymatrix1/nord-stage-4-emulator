//! Renders one Synth layer scenario to a 16-bit stereo WAV for the Audible Genius lesson-video
//! comparisons (`tools/ag_compare.py`, `docs/research/audible-genius-synth-lessons.md`).
//!
//! Usage: `cargo run --release -p nord-core --example ag_synth -- out.wav key=value ...`
//!
//! Keys (panel units where the panel has them): `note` (MIDI, 67), `notes` (comma list, overrides note), `vel` (100), `hold` (seconds
//! the key is down, 2.0), `len` (total seconds, hold + 1.0), `wf` (saw|supersaw|triangle|sine),
//! `cutoff` (Hz), `res` (0..1), `filter` (lp12|lp24|ladder|bp|hp), `fenv_amt` (panel 0..10),
//! `fenv_atk`/`fenv_dec`/`fenv_rel` (seconds, displayed values), `detune` (panel 0..10),
//! `unison` (0..3), `oenv_amt` (-1..1), `oenv_dec` (s), `to_pitch` (0|1), `vib`
//! (off|on|delay|wheel|aftertouch), `vib_rate` (Hz), `vib_amt` (panel 0..10), `vib_delay` (s),
//! `vib_ctrl` (0..1), `lfo` (tri|saw|sawdown|square|sh), `lfo_dest` (filter|pitch|ctrl), `lfo_rate` (Hz), `lfo_amt` (panel 0..10), `chain` (legacy|strip), `engine` (zdf|firmware), `measured` (0|1), `amp_dec` (s, 45 = hold).

use nord_core::synth::{
    FilterEngine, FilterModel, LfoDestination, LfoShape, SynthChain, SynthLayer, VaWaveform,
    VibratoMode,
};
use std::collections::HashMap;
use std::io::Write;

const SR: f32 = 48_000.0;

fn num(kv: &HashMap<String, String>, k: &str, d: f32) -> f32 {
    kv.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn configure(kv: &HashMap<String, String>) -> SynthLayer {
    let mut s = SynthLayer::new(SR);
    s.waveform = match kv.get("wf").map(String::as_str) {
        Some("supersaw") => VaWaveform::SuperSaw,
        Some("triangle") => VaWaveform::Triangle,
        Some("sine") => VaWaveform::Sine,
        Some("pulse") => VaWaveform::Pulse,
        _ => VaWaveform::Saw,
    };
    s.filter_model = match kv.get("filter").map(String::as_str) {
        Some("lp12") => FilterModel::Lp12,
        Some("ladder") => FilterModel::Ladder24,
        Some("bp") => FilterModel::BandPass,
        Some("hp") => FilterModel::HighPass,
        _ => FilterModel::Lp24,
    };
    s.filter_on = kv.get("filter").map(String::as_str) != Some("off");
    s.cutoff_hz = num(kv, "cutoff", 20_000.0);
    s.resonance = num(kv, "res", 0.0);
    s.filter_drive = 0.0;
    s.filter_env_amount = num(kv, "fenv_amt", 0.0) / 10.0;
    s.filter_env_dedicated = kv.contains_key("fenv_dec");
    s.filter_env_attack_sec = num(kv, "fenv_atk", 0.0005);
    s.filter_env_decay_sec = num(kv, "fenv_dec", 1.0);
    s.filter_env_release_sec = num(kv, "fenv_rel", 0.003);
    s.attack_sec = num(kv, "amp_atk", 0.0005);
    let amp_dec = num(kv, "amp_dec", 45.0);
    s.decay_sec = amp_dec;
    s.sustain_level = if amp_dec >= 44.0 { 1.0 } else { 0.0 };
    s.release_sec = num(kv, "amp_rel", 0.05);
    s.supersaw_detune = (num(kv, "detune", 0.0) / 10.0).clamp(0.0, 1.0);
    s.unison = num(kv, "unison", 0.0) as u8;
    s.osc_env_amount = num(kv, "oenv_amt", 0.0);
    s.osc_env_attack_sec = num(kv, "oenv_atk", 0.0005);
    s.osc_env_decay_sec = num(kv, "oenv_dec", 0.1);
    s.osc_env_release_sec = num(kv, "oenv_rel", 3.64);
    s.osc_env_to_pitch = num(kv, "to_pitch", 0.0) > 0.5;
    s.vibrato_mode = match kv.get("vib").map(String::as_str) {
        Some("on") => VibratoMode::On,
        Some("delay") => VibratoMode::Delay,
        Some("wheel") => VibratoMode::Wheel,
        Some("aftertouch") => VibratoMode::Aftertouch,
        _ => VibratoMode::Off,
    };
    s.vibrato_rate_hz = num(kv, "vib_rate", 5.0);
    s.vibrato_depth_cents = num(kv, "vib_amt", 5.0) / 10.0 * 92.0;
    s.vibrato_delay_sec = num(kv, "vib_delay", 0.7);
    s.vibrato_ctrl = num(kv, "vib_ctrl", 0.0);
    s.lfo_shape = match kv.get("lfo").map(String::as_str) {
        Some("saw") => LfoShape::Saw,
        Some("sawdown") => LfoShape::SawDown,
        Some("square") => LfoShape::Square,
        Some("sh") => LfoShape::SampleAndHold,
        _ => LfoShape::Triangle,
    };
    s.lfo_dest = match kv.get("lfo_dest").map(String::as_str) {
        Some("pitch") => LfoDestination::OscPitch,
        Some("ctrl") => LfoDestination::OscCtrl,
        _ => LfoDestination::FilterCutoff,
    };
    s.lfo_rate_hz = num(kv, "lfo_rate", 3.5);
    s.lfo_amount = num(kv, "lfo_amt", 0.0) / 10.0;
    s.measured_laws = num(kv, "measured", 0.0) > 0.5;
    s.chain = if kv.get("chain").map(String::as_str) == Some("strip") {
        SynthChain::FirmwareStrip
    } else {
        SynthChain::Legacy
    };
    if kv.get("engine").map(String::as_str) == Some("firmware") {
        s.filter_engine = FilterEngine::Firmware;
    }
    s
}

fn write_wav(path: &str, frames: &[[f32; 2]]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let data = (frames.len() * 4) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&(SR as u32).to_le_bytes())?;
    f.write_all(&(SR as u32 * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data.to_le_bytes())?;
    for fr in frames {
        for c in fr {
            f.write_all(&((c.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("usage: ag_synth out.wav key=value ...");
    let kv: HashMap<String, String> = args
        .filter_map(|a| {
            a.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();
    let mut s = configure(&kv);
    let note = num(&kv, "note", 67.0) as u8;
    let hold = (num(&kv, "hold", 2.0) * SR) as usize;
    let total = (num(&kv, "len", num(&kv, "hold", 2.0) + 1.0) * SR) as usize;
    let notes: Vec<u8> = kv
        .get("notes")
        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect())
        .unwrap_or_else(|| vec![note]);
    for &n in &notes {
        s.note_on(n, num(&kv, "vel", 100.0) as u8);
    }
    let mut frames = Vec::with_capacity(total);
    for i in 0..total {
        if i == hold {
            for &n in &notes {
                s.note_off(n);
            }
        }
        frames.push(s.render_frame());
    }
    write_wav(&out, &frames).expect("write wav");
}

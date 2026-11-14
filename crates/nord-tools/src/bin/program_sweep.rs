//! Renders every `.ns4p` program of a Nord Stage 4 backup (`.ns4b`) twice (engine defaults and
//! `--firmware-all --firmware-routing`) and prints one JSON line per program with the decoded
//! program, the resolved sounds, the routing facts and the output-health statistics.
//!
//! Usage: `cargo run --release -p nord-tools --bin program_sweep -- <backup.ns4b>
//! [--frames N] [--limit N] [--only <substring>] [--every K] [--notes a,b,..] [--velocity V]`;
//! `tools/program_sweep.py` drives it. Output
//! holds derived numbers only (no audio, no firmware or sample content). Release build only: a
//! debug build overflows the stack in the FX chains.

use nord_core::cbin::{CbinFile, Ns4BackupArchive, ProgramDecoded, ProgramSoundRefs};
use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;
use nord_core::piano::NpnoSound;
use nord_core::render::{render_timeline, TimedEvent};
use nord_core::render_opts::FirmwareOptions;
use nord_core::sinc::key_rule::kernel_for_key;
use nord_core::sweep::{analyze, rms_delta_db};
use nord_core::synth::SynthSampleSound;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

const FS: f32 = 48_000.0;
/// Standard note set: C3, G3, E4, C5 (spans the Low / Mid / High split regions).
const NOTES: [u8; 4] = [48, 55, 64, 72];
const VELOCITY: u8 = 100;

/// Loaded `.npno` sound and its class byte (file offset 0x44), `None` when it failed to load.
type PianoCache = HashMap<String, Option<(Arc<NpnoSound>, u8)>>;
type SynthCache = HashMap<String, Option<Arc<SynthSampleSound>>>;

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
}

/// Kernel indices (`sinc::key_rule`) reachable over the key spans of a multisample's zones.
fn sample_kernels(s: &SynthSampleSound) -> Vec<usize> {
    let mut seen = [false; 6];
    for z in &s.zones {
        for key in z.key_low..=z.key_high {
            seen[kernel_for_key(key)] = true;
        }
    }
    (0..6).filter(|&k| seen[k]).collect()
}

fn events(notes: &[u8], velocity: u8, hold: usize) -> Vec<TimedEvent> {
    let mut v = Vec::new();
    for &note in notes {
        v.push(TimedEvent {
            frame: 0,
            event: MidiEvent::NoteOn {
                channel: 0,
                note,
                velocity,
            },
        });
        v.push(TimedEvent {
            frame: hold,
            event: MidiEvent::NoteOff {
                channel: 0,
                note,
                velocity: 0,
            },
        });
    }
    v
}

fn piano_of<'a>(c: &'a PianoCache, r: &Option<String>) -> Option<&'a (Arc<NpnoSound>, u8)> {
    r.as_ref().and_then(|r| c.get(r)).and_then(Option::as_ref)
}

fn synth_of<'a>(c: &'a SynthCache, r: &Option<String>) -> Option<&'a Arc<SynthSampleSound>> {
    r.as_ref().and_then(|r| c.get(r)).and_then(Option::as_ref)
}

/// Builds an engine for `prog` with the cached sounds installed (mirrors
/// `load_program_bytes_with_sounds_from_backup`) and `opts` applied.
fn build_engine(
    prog: &ProgramDecoded,
    refs: &ProgramSoundRefs,
    pianos: &PianoCache,
    synths: &SynthCache,
    opts: &FirmwareOptions,
) -> Stage4Engine {
    let mask = prog.global.scene_1_mask | prog.global.scene_2_mask;
    let needed = |on: bool, bit: u8| on || mask == 0 || mask & (1 << bit) != 0;
    let mut engine = Stage4Engine::new(FS);
    engine.apply_program(prog);
    if needed(prog.piano.layer_a.enabled, 2) {
        if let Some((s, _)) = piano_of(pianos, &refs.piano_a) {
            engine.piano_a.set_sound(Some(Arc::clone(s)));
        }
    }
    if needed(prog.piano.layer_b.enabled, 3) {
        if let Some((s, _)) = piano_of(pianos, &refs.piano_b) {
            engine.piano_b.set_sound(Some(Arc::clone(s)));
        }
    }
    if needed(prog.synth.layer_a.enabled, 4) {
        if let Some(s) = synth_of(synths, &refs.synth_a) {
            engine.synth_a.load_sample(Some(Arc::clone(s)));
        }
    }
    if needed(prog.synth.layer_b.enabled, 5) {
        if let Some(s) = synth_of(synths, &refs.synth_b) {
            engine.synth_b.load_sample(Some(Arc::clone(s)));
        }
    }
    if needed(prog.synth.layer_c.enabled, 6) {
        if let Some(s) = synth_of(synths, &refs.synth_c) {
            engine.synth_c.load_sample(Some(Arc::clone(s)));
        }
    }
    opts.apply_to(&mut engine);
    engine
}

fn panic_message(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>()
        .cloned()
        .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_default()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: program_sweep <backup.ns4b> [--frames N] [--limit N] [--only S] [--every K] [--notes a,b] [--velocity V]")?;
    let frames: usize = flag(&args, "--frames")
        .and_then(|v| v.parse().ok())
        .unwrap_or(96_000);
    let limit: usize = flag(&args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let only = flag(&args, "--only");
    let hold = frames * 3 / 4;
    let notes: Vec<u8> = flag(&args, "--notes").map_or_else(
        || NOTES.to_vec(),
        |v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect(),
    );
    let velocity: u8 = flag(&args, "--velocity")
        .and_then(|v| v.parse().ok())
        .unwrap_or(VELOCITY)
        .min(127);
    let every: usize = flag(&args, "--every")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let archive = Ns4BackupArchive::open(path).map_err(|e| e.to_string())?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut pianos: PianoCache = HashMap::new();
    let mut synths: SynthCache = HashMap::new();
    let names: Vec<String> = archive
        .entries_with_ext(".ns4p")
        .iter()
        .map(|e| e.name.clone())
        .filter(|n| only.is_none_or(|s| n.contains(s.as_str())))
        .step_by(every)
        .take(limit)
        .collect();
    std::panic::set_hook(Box::new(|_| {}));
    let modes = [
        ("default", FirmwareOptions::default()),
        (
            "firmware",
            FirmwareOptions {
                firmware_routing: true,
                ..FirmwareOptions::firmware_all()
            },
        ),
    ];
    for name in names {
        let bytes = archive
            .read_entry(&mut file, &name)
            .map_err(|e| e.to_string())?;
        let prog = CbinFile::parse(&bytes)
            .and_then(|c| c.decode_program())
            .map_err(|e| format!("{name}: {e}"))?;
        let refs = archive.resolve_program_sounds(&prog);
        for r in [&refs.piano_a, &refs.piano_b].into_iter().flatten() {
            if !pianos.contains_key(r) {
                let loaded = archive.read_entry(&mut file, r).ok().and_then(|b| {
                    let class = b.get(0x44).copied().unwrap_or(0);
                    NpnoSound::from_npno_bytes(&b)
                        .ok()
                        .map(|s| (Arc::new(s), class))
                });
                pianos.insert(r.clone(), loaded);
            }
        }
        for r in [&refs.synth_a, &refs.synth_b, &refs.synth_c]
            .into_iter()
            .flatten()
        {
            if !synths.contains_key(r) {
                let loaded = archive
                    .read_entry(&mut file, r)
                    .ok()
                    .and_then(|b| SynthSampleSound::from_nsmp4_bytes(&b).ok().map(Arc::new));
                synths.insert(r.clone(), loaded);
            }
        }
        let mut runs = serde_json::Map::new();
        for (label, opts) in &modes {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut engine = build_engine(&prog, &refs, &pianos, &synths, opts);
                let (l, r) = render_timeline(&mut engine, &events(&notes, velocity, hold), frames);
                analyze(&l, &r)
            }));
            let value = match result {
                Ok(s) => serde_json::to_value(s).map_err(|e| e.to_string())?,
                Err(p) => json!({ "panic": panic_message(p) }),
            };
            runs.insert((*label).to_string(), value);
        }
        let rms = |k: &str| runs[k]["rms"].as_f64().map(|x| x as f32);
        let delta = rms("default")
            .zip(rms("firmware"))
            .and_then(|(a, b)| rms_delta_db(a, b));
        let routing = {
            let mut e = Stage4Engine::new(FS);
            e.apply_program(&prog);
            let r = e.program_routing();
            json!({ "exact_masks": r.exact_masks, "legacy_masks": r.legacy_masks,
                    "kb_zone": r.kb_zone, "split_active": r.split_active })
        };
        let class =
            |r: &Option<String>| piano_of(&pianos, r).map_or(Value::Null, |(_, c)| json!(c));
        let sound = |r: &Option<String>, ok: bool| json!({ "ref": r.is_some(), "loaded": ok });
        // SAMPLES layers: kernels the firmware key rule reaches over the key span of the zones.
        let kernels = |r: &Option<String>| {
            synth_of(&synths, r).map_or(Value::Null, |s| json!(sample_kernels(s)))
        };
        let line = json!({
            "entry": name,
            "program": serde_json::from_str::<Value>(&prog.to_json().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
            "sounds": {
                "piano_a": sound(&refs.piano_a, piano_of(&pianos, &refs.piano_a).is_some()),
                "piano_b": sound(&refs.piano_b, piano_of(&pianos, &refs.piano_b).is_some()),
                "synth_a": sound(&refs.synth_a, synth_of(&synths, &refs.synth_a).is_some()),
                "synth_a_kernels": kernels(&refs.synth_a),
                "synth_b": sound(&refs.synth_b, synth_of(&synths, &refs.synth_b).is_some()),
                "synth_b_kernels": kernels(&refs.synth_b),
                "synth_c": sound(&refs.synth_c, synth_of(&synths, &refs.synth_c).is_some()),
                "synth_c_kernels": kernels(&refs.synth_c),
                "piano_a_class": class(&refs.piano_a),
                "piano_b_class": class(&refs.piano_b),
            },
            "routing": routing,
            "renders": runs,
            "rms_delta_db": delta,
        });
        println!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flag_returns_the_value_after_the_name() {
        let a = args(&["x.ns4b", "--frames", "100", "--limit"]);
        assert_eq!(flag(&a, "--frames").map(String::as_str), Some("100"));
        assert_eq!(flag(&a, "--limit"), None, "no value after the last flag");
        assert_eq!(flag(&a, "--only"), None);
    }

    #[test]
    fn events_pair_each_note_on_at_zero_with_a_note_off_at_hold() {
        let ev = events(&[48, 60], 90, 1000);
        assert_eq!(ev.len(), 4);
        assert!(ev.iter().step_by(2).all(|e| e.frame == 0));
        assert!(ev.iter().skip(1).step_by(2).all(|e| e.frame == 1000));
        assert!(matches!(
            ev[0].event,
            MidiEvent::NoteOn {
                note: 48,
                velocity: 90,
                ..
            }
        ));
    }

    #[test]
    fn panic_message_reads_string_and_str_payloads() {
        assert_eq!(panic_message(Box::new(String::from("a"))), "a");
        assert_eq!(panic_message(Box::new("b")), "b");
        assert_eq!(panic_message(Box::new(3u8)), "");
    }

    #[test]
    fn sample_kernels_lists_ascending_reachable_kernel_indices() {
        let pcm: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin()).collect();
        let s = SynthSampleSound::new_synthetic("T", 60, &pcm, FS, None, 32);
        let k = sample_kernels(&s);
        assert!(!k.is_empty());
        assert!(k.windows(2).all(|w| w[0] < w[1]));
        assert!(k.iter().all(|&i| i < 6));
    }
}

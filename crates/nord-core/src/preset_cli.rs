//! `ns4 preset-check`: decodes every Preset Library entry of a `.ns4b` backup, checks that it
//! re-encodes bit-identically (AGENTS.md rule 5), that an edit changes only the edited bits, and
//! that it loads into the engine.
//!
//! Spec: `docs/specs/cbin-and-presets.md` section 5.6. Python counterpart for the structural
//! checks (sizes, schema ids, CRC-32): `tools/preset_check.py`.

use crate::cbin::{CbinFile, Ns4BackupArchive, PresetSection, SectionPreset};
use crate::engine::{invert_fx_chain, FxParams, Stage4Engine};
use crate::midi::MidiEvent;
use crate::program_check::check_program;
use crate::synth_params_export::invert_decoded_layer;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;

/// Usage of `ns4 preset-check`.
pub const CHECK_USAGE: &str = "usage: ns4 preset-check <backup.ns4b>";

/// Outcome of checking one preset entry.
#[derive(Debug, Default)]
pub struct EntryReport {
    /// Failed checks; empty when everything holds.
    pub problems: Vec<String>,
    /// The decoder is lossy for this entry: `pack(decode(file))` differs from the file bits
    /// (handled by the bit merge of `SectionPreset::to_file_like`, so not a failure).
    pub lossy: bool,
}

/// A small edit of the first layer's level (any section), to test the bit-merging encoder.
fn nudge(p: &mut SectionPreset) {
    let level = |v: f32| if v > 0.5 { v - 0.25 } else { v + 0.25 };
    match p {
        SectionPreset::Organ(o) => o.layer_a_level = level(o.layer_a_level),
        SectionPreset::Piano(o) => o.layer_a.level = level(o.layer_a.level),
        SectionPreset::Synth(o) => o.layer_a.level = level(o.layer_a.level),
    }
}

/// Edits `preset` and checks the merged file against `file`: only bits of the edited field change.
fn check_edit(preset: &SectionPreset, file: &CbinFile, problems: &mut Vec<String>) {
    let mut edited = preset.clone();
    nudge(&mut edited);
    let (Ok(out), Ok(base), Ok(new)) = (edited.to_file_like(file), preset.pack(), edited.pack())
    else {
        problems.push("edit + encode failed".to_string());
        return;
    };
    let allowed = base.iter().zip(&new).map(|(a, b)| a ^ b);
    let outside = out
        .payload
        .iter()
        .zip(&file.payload)
        .zip(allowed)
        .any(|((o, f), a)| (o ^ f) & !a != 0);
    if outside {
        problems.push("an edit changed bits outside the edited field".to_string());
    }
    if out.payload == file.payload || !out.checksum_ok() {
        problems.push("edited file lost the edit or has a stale checksum".to_string());
    }
}

/// Loads `preset` into a fresh engine, plays a note and reports a non-finite output.
fn renders_finite(preset: &SectionPreset, sample_rate: f32) -> bool {
    let mut engine = Box::new(Stage4Engine::new(sample_rate));
    engine.load_section_preset(preset, None);
    let mut l = [0.0f32; 256];
    let mut r = [0.0f32; 256];
    engine.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 100,
    });
    engine.process_block(&mut l, &mut r);
    l.iter().chain(r.iter()).all(|v| v.is_finite())
}

/// The unconditional inverse of the engine mapping (`invert_decoded_layer`, `invert_fx_chain`)
/// must return the decoded preset: `pack(inverse(engine(preset))) == pack(preset)`, i.e. every
/// field the inverse writes round-trips to the same bits.
fn check_inverse(preset: &SectionPreset, sample_rate: f32, problems: &mut Vec<String>) {
    let mut engine = Box::new(Stage4Engine::new(sample_rate));
    engine.load_section_preset(preset, None);
    let fx = |i: usize| FxParams::capture(&engine.fx_chains[i]);
    let mut back = preset.clone();
    match &mut back {
        SectionPreset::Organ(o) => invert_fx_chain(&fx(0), &mut o.fx),
        SectionPreset::Piano(p) => {
            invert_fx_chain(&fx(2), &mut p.fx_a);
            invert_fx_chain(&fx(3), &mut p.fx_b);
        }
        SectionPreset::Synth(s) => {
            let layers = [&mut s.layer_a, &mut s.layer_b, &mut s.layer_c];
            let synths = [&engine.synth_a, &engine.synth_b, &engine.synth_c];
            for (i, (dec, synth)) in layers.into_iter().zip(synths).enumerate() {
                invert_decoded_layer(synth, &engine.synth_cutoff_morph[i], dec);
            }
            invert_fx_chain(&fx(4), &mut s.fx_a);
            invert_fx_chain(&fx(5), &mut s.fx_b);
            invert_fx_chain(&fx(6), &mut s.fx_c);
        }
    }
    if back.pack().ok() != preset.pack().ok() {
        problems.push("the engine inverse does not return the decoded bits".to_string());
    }
}

/// STORE round trip: the preset loaded into an engine and captured again must give the file back
/// byte for byte (the Synth sound and the FX chains come back through `synth_params_export` /
/// `program_fx_export`), and one panel edit must reach the stored preset without touching the rest.
fn check_store(
    preset: &SectionPreset,
    file: &CbinFile,
    sample_rate: f32,
    problems: &mut Vec<String>,
) {
    let mut engine = Box::new(Stage4Engine::new(sample_rate));
    engine.load_section_preset(preset, None);
    let section = preset.section();
    let same = engine.capture_section_preset(section, Some(preset));
    match same.to_file_like(file) {
        Ok(f) if f.to_bytes() == file.to_bytes() => {}
        Ok(_) => problems.push("an unedited STORE changed the file".to_string()),
        Err(e) => problems.push(format!("store: {e}")),
    }
    if let SectionPreset::Synth(base) = preset {
        let hz = base.layer_a.filter_cutoff_hz;
        engine.synth_a.cutoff_hz = if hz > 1000.0 { hz * 0.5 } else { hz * 2.0 };
        let SectionPreset::Synth(edited) = engine.capture_section_preset(section, Some(preset))
        else {
            return;
        };
        let moved = (edited.layer_a.filter_cutoff_hz / hz - 1.0).abs() > 0.1;
        let mut rest = (*edited).clone();
        rest.layer_a.filter_cutoff_hz = hz;
        if !moved || SectionPreset::Synth(Box::new(rest)) != *preset {
            problems.push("a cutoff edit did not reach the stored preset alone".to_string());
        }
    }
}

/// Checks one preset entry: header checksum, decode, bit-identical re-encode, a merged edit and a
/// finite engine render after loading it.
pub fn check_entry(bytes: &[u8], sample_rate: f32) -> EntryReport {
    let mut rep = EntryReport::default();
    let file = match CbinFile::parse(bytes) {
        Ok(f) => f,
        Err(e) => {
            rep.problems.push(format!("parse: {e}"));
            return rep;
        }
    };
    if !file.checksum_ok() {
        rep.problems
            .push("header checksum is not the CRC-32 of bytes 0x2C..".to_string());
    }
    let preset = match SectionPreset::from_file(&file) {
        Ok(p) => p,
        Err(e) => {
            rep.problems.push(format!("decode: {e}"));
            return rep;
        }
    };
    match preset.pack() {
        Ok(p) => rep.lossy = p != file.payload,
        Err(e) => rep.problems.push(format!("pack: {e}")),
    }
    match preset.to_file_like(&file) {
        Ok(f) if f.to_bytes() == bytes => {}
        Ok(_) => rep
            .problems
            .push("to_file_like is not byte-identical".to_string()),
        Err(e) => rep.problems.push(format!("to_file_like: {e}")),
    }
    check_edit(&preset, &file, &mut rep.problems);
    check_inverse(&preset, sample_rate, &mut rep.problems);
    check_store(&preset, &file, sample_rate, &mut rep.problems);
    if !renders_finite(&preset, sample_rate) {
        rep.problems.push("engine output is not finite".to_string());
    }
    rep
}

/// Stack of the checking thread: a debug build captures whole engine states on the stack.
const CHECK_STACK_BYTES: usize = 64 << 20;

/// `ns4 preset-check`: `args` are the words after the subcommand.
pub fn preset_check(args: &[String]) -> Result<String, String> {
    let args = args.to_vec();
    std::thread::Builder::new()
        .stack_size(CHECK_STACK_BYTES)
        .spawn(move || check_archive(&args))
        .map_err(|e| format!("preset-check thread: {e}"))?
        .join()
        .map_err(|_| "preset-check panicked".to_string())?
}

fn check_archive(args: &[String]) -> Result<String, String> {
    let path = args.first().ok_or_else(|| CHECK_USAGE.to_string())?;
    let archive = Ns4BackupArchive::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut reader = BufReader::new(File::open(path).map_err(|e| format!("{path}: {e}"))?);
    let mut counts: BTreeMap<PresetSection, usize> = BTreeMap::new();
    let mut lossy: BTreeMap<PresetSection, usize> = BTreeMap::new();
    let mut lines = Vec::new();
    let mut failed = 0usize;
    let mut programs = 0usize;
    for (i, e) in archive.entries().iter().enumerate() {
        let Some(section) = PresetSection::from_entry(&e.name) else {
            if e.name.starts_with("Program/") && e.name.ends_with(".ns4p") {
                let bytes = archive
                    .read_entry_by_index(&mut reader, i)
                    .map_err(|err| format!("{}: {err}", e.name))?;
                programs += 1;
                let problems = check_program(&bytes, 48_000.0);
                if !problems.is_empty() {
                    failed += 1;
                    lines.push(format!("PROBLEM {}: {}", e.name, problems.join("; ")));
                }
            }
            continue;
        };
        let bytes = archive
            .read_entry_by_index(&mut reader, i)
            .map_err(|err| format!("{}: {err}", e.name))?;
        *counts.entry(section).or_default() += 1;
        let rep = check_entry(&bytes, 48_000.0);
        if rep.lossy {
            *lossy.entry(section).or_default() += 1;
        }
        if !rep.problems.is_empty() {
            failed += 1;
            lines.push(format!("PROBLEM {}: {}", e.name, rep.problems.join("; ")));
        }
    }
    for (s, n) in &counts {
        let l = lossy.get(s).copied().unwrap_or(0);
        lines.push(format!("{s:?}: {n} presets, {l} with a lossy decode"));
    }
    let total: usize = counts.values().sum();
    lines.push(format!("{programs} programs (STORE round trip + edits)"));
    lines.push(format!("{total} presets; problems: {failed}"));
    if failed > 0 {
        return Err(lines.join("\n"));
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_presets_of_all_sections_pass_the_check_and_corruption_is_reported() {
        for s in PresetSection::ALL {
            let f = SectionPreset::init(s).to_file(0, 1, 3).unwrap();
            let rep = check_entry(&f.to_bytes(), 48_000.0);
            assert!(rep.problems.is_empty() && !rep.lossy, "{s:?}: {rep:?}");
            let mut bad = f.clone();
            bad.header.checksum ^= 1;
            let p = check_entry(&bad.to_bytes(), 48_000.0).problems;
            assert!(p.iter().any(|m| m.contains("checksum")), "{p:?}");
        }
        assert!(!check_entry(b"junk", 48_000.0).problems.is_empty());
    }

    #[test]
    fn check_needs_a_path_and_a_readable_archive() {
        assert_eq!(preset_check(&[]).unwrap_err(), CHECK_USAGE);
        assert!(preset_check(&["/nonexistent/x.ns4b".to_string()]).is_err());
    }
}

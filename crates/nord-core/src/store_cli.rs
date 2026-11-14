//! `ns4 store-program` and `ns4 export-live`: engine -> `.ns4p` / `.ns4l` persistence commands.
//!
//! Spec: `docs/specs/cbin-and-presets.md` "Engine export". Both load the base program into a
//! `Stage4Engine`, optionally edit it, and write the engine state back over the base program with
//! `Stage4Engine::export_program`, so unmapped fields stay bit-identical.

use crate::cbin::{CbinFile, ProgramDecoded};
use crate::engine::{LiveSlots, Stage4Engine, LIVE_SLOT_COUNT};
use std::fs;

const SAMPLE_RATE: f32 = 48_000.0;

/// Usage of `ns4 store-program`.
pub const STORE_USAGE: &str =
    "usage: ns4 store-program <base.ns4p> <out.ns4p> [--tempo BPM] [--bank N] [--loc N]";
/// Usage of `ns4 export-live`.
pub const LIVE_USAGE: &str =
    "usage: ns4 export-live <out-dir> <slot1.ns4p|-> ... <slot8.ns4p|-> (writes live-N.ns4l; '-' leaves a slot empty)";

fn load(path: &str) -> Result<(CbinFile, ProgramDecoded), String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    let file = CbinFile::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
    let prog = ProgramDecoded::from_cbin_file(&file).map_err(|e| format!("{path}: {e}"))?;
    Ok((file, prog))
}

fn flag<'a>(args: &'a [String], name: &str) -> Result<Option<&'a str>, String> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .map(|v| Some(v.as_str()))
            .ok_or_else(|| format!("{name} needs a value")),
    }
}

/// `ns4 store-program`: `args` are the words after the subcommand.
pub fn store_program(args: &[String]) -> Result<String, String> {
    let (base_path, out_path) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) if !a.starts_with("--") && !b.starts_with("--") => (a, b),
        _ => return Err(STORE_USAGE.to_string()),
    };
    let (template, prog) = load(base_path)?;
    let mut engine = Stage4Engine::new(SAMPLE_RATE);
    engine.apply_program(&prog);
    if let Some(t) = flag(args, "--tempo")? {
        engine.master_clock.bpm = t.parse::<f32>().map_err(|e| format!("--tempo: {e}"))?;
    }
    let mut out = engine.export_program(&prog);
    if let Some(b) = flag(args, "--bank")? {
        out.bank_index = b.parse().map_err(|e| format!("--bank: {e}"))?;
    }
    if let Some(l) = flag(args, "--loc")? {
        out.location_index = l.parse().map_err(|e| format!("--loc: {e}"))?;
    }
    let mut file = out
        .to_cbin_file_like(&template)
        .map_err(|e| e.to_string())?;
    file.header.bank_index = out.bank_index;
    file.header.location_index = out.location_index;
    let bytes = file.to_bytes();
    fs::write(out_path, &bytes).map_err(|e| format!("failed to write {out_path}: {e}"))?;
    Ok(format!(
        "wrote {out_path} ({} B, tempo {} BPM)",
        bytes.len(),
        out.global.tempo_bpm
    ))
}

/// `ns4 export-live`: `args` are the words after the subcommand.
pub fn export_live(args: &[String]) -> Result<String, String> {
    let dir = args.first().ok_or_else(|| LIVE_USAGE.to_string())?;
    let inputs = &args[1..];
    if inputs.is_empty() || inputs.len() > LIVE_SLOT_COUNT {
        return Err(LIVE_USAGE.to_string());
    }
    fs::create_dir_all(dir).map_err(|e| format!("failed to create {dir}: {e}"))?;
    let mut live = LiveSlots::default();
    let mut bases = Vec::new();
    for (slot, path) in inputs.iter().enumerate() {
        if path == "-" {
            continue;
        }
        let (_, prog) = load(path)?;
        live.import_program(slot, &prog, SAMPLE_RATE);
        bases.push((slot, prog));
    }
    let mut lines = Vec::new();
    for (slot, base) in &bases {
        let prog = live
            .export_slot(*slot, base)
            .ok_or_else(|| format!("slot {} is empty", slot + 1))?;
        let file = prog
            .to_live_cbin_file(*slot as u16)
            .map_err(|e| e.to_string())?;
        let path = format!("{dir}/live-{}.ns4l", slot + 1);
        fs::write(&path, file.to_bytes()).map_err(|e| format!("failed to write {path}: {e}"))?;
        lines.push(path);
    }
    Ok(format!(
        "wrote {} Live slot file(s):\n{}",
        lines.len(),
        lines.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::CbinKind;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ns4-store-cli-{}-{name}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn words(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn write_base(dir: &std::path::Path, tempo: f32) -> String {
        let mut p = ProgramDecoded::default();
        p.global.tempo_bpm = tempo;
        let path = dir.join("base.ns4p");
        fs::write(&path, p.to_cbin_file(1, 4).unwrap().to_bytes()).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn store_program_without_edits_is_byte_identical() {
        let dir = tmp("same");
        let base = write_base(&dir, 97.0);
        let out = dir.join("out.ns4p").to_string_lossy().into_owned();
        store_program(&words(&[&base, &out])).unwrap();
        assert_eq!(fs::read(&base).unwrap(), fs::read(&out).unwrap());
    }

    #[test]
    fn store_program_applies_tempo_and_location() {
        let dir = tmp("edit");
        let base = write_base(&dir, 97.0);
        let out = dir.join("out.ns4p").to_string_lossy().into_owned();
        store_program(&words(&[&base, &out, "--tempo", "141", "--loc", "9"])).unwrap();
        let (file, prog) = load(&out).unwrap();
        assert_eq!(prog.global.tempo_bpm, 141.0);
        assert_eq!(file.header.location_index, 9);
        assert!(store_program(&words(&[&base])).is_err());
        assert!(store_program(&words(&[&base, &out, "--tempo"])).is_err());
    }

    #[test]
    fn export_live_writes_one_file_per_filled_slot() {
        let dir = tmp("live");
        let base = write_base(&dir, 111.0);
        let out = dir.join("live").to_string_lossy().into_owned();
        let msg = export_live(&words(&[&out, &base, "-", &base])).unwrap();
        assert!(msg.contains("2 Live slot"));
        let f1 = CbinFile::parse(&fs::read(format!("{out}/live-1.ns4l")).unwrap()).unwrap();
        assert_eq!(f1.header.kind, CbinKind::Live);
        assert!(fs::metadata(format!("{out}/live-2.ns4l")).is_err());
        let f3 = CbinFile::parse(&fs::read(format!("{out}/live-3.ns4l")).unwrap()).unwrap();
        assert_eq!(f3.header.location_index, 2);
        assert_eq!(
            ProgramDecoded::from_cbin_file(&f3)
                .unwrap()
                .global
                .tempo_bpm,
            111.0
        );
        assert!(export_live(&words(&[&out])).is_err());
    }
}

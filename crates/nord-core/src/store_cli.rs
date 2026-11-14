//! `ns4 store-program` and `ns4 export-live`: engine -> `.ns4p` / `.ns4l` persistence commands.
//!
//! Spec: `docs/specs/cbin-and-presets.md` "Engine export". Both load the base program into a
//! `Stage4Engine`, optionally edit it, and write the engine state back over the base program with
//! `Stage4Engine::export_program`, so unmapped fields stay bit-identical.

use crate::cbin::{store_into_backup, CbinFile, LivePut, ProgramDecoded, ProgramPut};
use crate::engine::{LiveSlots, Stage4Engine, LIVE_SLOT_COUNT};
use std::fs;

const SAMPLE_RATE: f32 = 48_000.0;

/// Usage of `ns4 store-program`.
pub const STORE_USAGE: &str =
    "usage: ns4 store-program <base.ns4p> <out.ns4p> [--tempo BPM] [--bank N] [--loc N]";
/// Usage of `ns4 export-live`.
pub const LIVE_USAGE: &str =
    "usage: ns4 export-live <out-dir> <slot1.ns4p|-> ... <slot8.ns4p|-> (writes live-N.ns4l; '-' leaves a slot empty)";

/// Usage of `ns4 backup-store`.
pub const BACKUP_USAGE: &str = "usage: ns4 backup-store <in.ns4b> <out.ns4b> [--program <file.ns4p> [--bank A-H|0-7] [--loc 0-63] [--name <program name>]] [--live <file.ns4l> [--slot 0-7]] (no edit flags: verbatim copy)";

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

/// Bank `A`..`H` or `0`..`7` as an index.
fn parse_bank(v: &str) -> Result<u16, String> {
    let b = match v.as_bytes() {
        [c @ b'A'..=b'H'] => u16::from(c - b'A'),
        [c @ b'a'..=b'h'] => u16::from(c - b'a'),
        _ => v.parse::<u16>().map_err(|e| format!("--bank: {e}"))?,
    };
    (b < 8)
        .then_some(b)
        .ok_or_else(|| "--bank: expected A-H or 0-7".to_string())
}

fn parse_below(v: &str, name: &str, limit: u16) -> Result<u16, String> {
    let n = v.parse::<u16>().map_err(|e| format!("{name}: {e}"))?;
    (n < limit)
        .then_some(n)
        .ok_or_else(|| format!("{name}: expected 0-{}", limit - 1))
}

/// `ns4 backup-store`: writes a program and/or a Live slot file into a copy of a `.ns4b` backup.
/// Every other entry (and `meta.xml`) stays byte-identical.
pub fn backup_store(args: &[String]) -> Result<String, String> {
    let (src, dst) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) if !a.starts_with("--") && !b.starts_with("--") => (a, b),
        _ => return Err(BACKUP_USAGE.to_string()),
    };
    let (mut programs, mut lives) = (Vec::new(), Vec::new());
    if let Some(path) = flag(args, "--program")? {
        let (file, _) = load(path)?;
        let bank = match flag(args, "--bank")? {
            Some(b) => parse_bank(b)?,
            None => file.header.bank_index,
        };
        let loc = match flag(args, "--loc")? {
            Some(l) => parse_below(l, "--loc", 64)?,
            None => file.header.location_index,
        };
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        let name = flag(args, "--name")?.unwrap_or(stem);
        programs.push(ProgramPut::from_file(bank, loc, name, &file).map_err(|e| e.to_string())?);
    }
    if let Some(path) = flag(args, "--live")? {
        let (file, _) = load(path)?;
        let slot = match flag(args, "--slot")? {
            Some(n) => parse_below(n, "--slot", LIVE_SLOT_COUNT as u16)?,
            None => file.header.location_index,
        };
        let mut file = file;
        file.header.bank_index = 0;
        file.header.location_index = slot;
        lives.push(LivePut {
            slot,
            bytes: file.to_bytes(),
        });
    }
    // No edits: a verbatim copy (round-trip check of the writer).
    let rep = store_into_backup(src.as_ref(), dst.as_ref(), &programs, &lives)
        .map_err(|e| format!("{src}: {e}"))?;
    Ok(format!(
        "wrote {dst} ({} B): {} replaced, {} added, {} removed",
        rep.bytes_written, rep.replaced, rep.added, rep.removed
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

    #[test]
    fn backup_store_replaces_one_program_and_rejects_bad_arguments() {
        use crate::cbin::{rewrite_backup_file, EntryEdit, Ns4BackupArchive};
        let dir = tmp("backup");
        let prog = |t: f32| {
            let mut p = ProgramDecoded::default();
            p.global.tempo_bpm = t;
            p.to_cbin_file(1, 7).unwrap().to_bytes()
        };
        let src = dir.join("in.ns4b");
        let seed = dir.join("seed.ns4b");
        // Build a one-program archive by adding entries to an empty-ish seed archive.
        fs::write(
            &seed,
            crate::cbin::backup_writer::more_tests::build_zip(&[("meta.xml", b"<backup/>")]),
        )
        .unwrap();
        let put = |n: &str, d: Vec<u8>| EntryEdit::Put {
            name: n.into(),
            data: d,
        };
        rewrite_backup_file(
            &seed,
            &src,
            &[put("Program/Bank B/Old.ns4p", prog(90.0_f32))],
        )
        .unwrap();
        let edited = dir.join("New.ns4p");
        fs::write(&edited, prog(150.0_f32)).unwrap();
        let out = dir.join("out.ns4b");
        let (s, o, e) = (
            src.to_string_lossy().into_owned(),
            out.to_string_lossy().into_owned(),
            edited.to_string_lossy().into_owned(),
        );
        let msg = backup_store(&words(&[
            &s,
            &o,
            "--program",
            &e,
            "--bank",
            "B",
            "--loc",
            "7",
        ]))
        .unwrap();
        assert!(
            msg.contains("1 added") && msg.contains("1 removed"),
            "{msg}"
        );
        let a = Ns4BackupArchive::open(&out).unwrap();
        assert!(a.find("Program/Bank B/New.ns4p").is_some());
        assert!(a.find("Old.ns4p").is_none());
        assert!(a.find("meta.xml").is_some());
        let bytes = a.read_entry_from_file(&out, "meta.xml").unwrap();
        assert_eq!(bytes, b"<backup/>");
        let copy = dir.join("copy.ns4b").to_string_lossy().into_owned();
        backup_store(&words(&[&s, &copy])).unwrap();
        assert_eq!(fs::read(&s).unwrap(), fs::read(&copy).unwrap());
        assert!(backup_store(&words(&[&s])).is_err());
        assert!(backup_store(&words(&[&s, &o, "--program", &e, "--bank", "J"])).is_err());
        assert!(backup_store(&words(&[&s, &o, "--program", &e, "--loc", "64"])).is_err());
        assert!(backup_store(&words(&[&s, &s, "--program", &e])).is_err());
        assert_eq!(parse_bank("c").unwrap(), 2);
        assert_eq!(parse_bank("7").unwrap(), 7);
    }
}

//! STORE check of one program file (`.ns4p`), the program half of `ns4 preset-check`.
//!
//! The program is loaded into an engine and exported again (`Stage4Engine::export_program`, the
//! path of STORE, Export backup and `ns4 store-program`): an unedited program must come back byte
//! for byte, and one Synth sound edit and one effect edit must reach the decoded program alone.
//!
//! Spec: `docs/specs/cbin-and-presets.md` sections 5.5 to 5.7.

use crate::cbin::{CbinFile, ProgramDecoded};
use crate::engine::Stage4Engine;
use crate::fx::ReverbModel;

/// Checks one `.ns4p` entry; the list holds the failed checks (empty: all hold).
pub fn check_program(bytes: &[u8], sample_rate: f32) -> Vec<String> {
    let mut problems = Vec::new();
    let Ok(file) = CbinFile::parse(bytes) else {
        return vec!["parse failed".to_string()];
    };
    let Ok(prog) = ProgramDecoded::from_cbin_file(&file) else {
        return vec!["decode failed".to_string()];
    };
    if !file.checksum_ok() {
        problems.push("header checksum is not the CRC-32 of bytes 0x2C..".to_string());
    }
    let mut engine = Box::new(Stage4Engine::new(sample_rate));
    engine.apply_program(&prog);
    let same = engine.export_program(&prog);
    match same.to_cbin_file_like(&file) {
        Ok(f) if f.to_bytes() == bytes => {}
        _ => problems.push("an unedited STORE changed the file".to_string()),
    }
    // One sound edit and one effect edit, each must show up and nothing else may move.
    let hz = prog.synth.layer_a.filter_cutoff_hz;
    engine.synth_a.cutoff_hz = if hz > 1000.0 { hz * 0.5 } else { hz * 2.0 };
    engine.fx_chains[4].reverb_model = match prog.synth.fx_a.reverb_model() {
        ReverbModel::Hall => ReverbModel::Room,
        _ => ReverbModel::Hall,
    };
    let edited = engine.export_program(&prog);
    let mut rest = edited.clone();
    rest.synth.layer_a.filter_cutoff_hz = hz;
    rest.synth.fx_a = prog.synth.fx_a.clone();
    let moved = (edited.synth.layer_a.filter_cutoff_hz / hz - 1.0).abs() > 0.1
        && edited.synth.fx_a.reverb_model() == engine.fx_chains[4].reverb_model;
    if !moved || rest != prog {
        problems.push("a cutoff / reverb edit did not reach the program alone".to_string());
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_program_passes_and_junk_is_reported() {
        let file = ProgramDecoded::default().to_cbin_file(0, 1).unwrap();
        assert_eq!(
            check_program(&file.to_bytes(), 48_000.0),
            Vec::<String>::new()
        );
        assert!(!check_program(b"junk", 48_000.0).is_empty());
    }

    #[test]
    fn a_stale_checksum_is_reported() {
        let mut file = ProgramDecoded::default().to_cbin_file(0, 1).unwrap();
        file.header.checksum ^= 1;
        let p = check_program(&file.to_bytes(), 48_000.0);
        assert!(p.iter().any(|m| m.contains("checksum")), "{p:?}");
    }
}

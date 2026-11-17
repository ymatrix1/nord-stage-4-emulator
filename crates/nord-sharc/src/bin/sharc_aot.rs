//! `sharc-aot IMAGE ENTRY MAX_INSNS [--fast-float] [--fn NAME] [--out FILE]`
//! transpiles a SHARC VISA routine from a flat SRAM image into standalone Rust source (Level-3 AOT).
//!
//! `sharc-aot IMAGE START LAST --ops` lists the decoded ops of the instructions starting in
//! `START..=LAST`; `sharc-aot IMAGE START LAST --hot [--tier pe|guarded|approx] --out FILE` writes
//! (tiers: see `AotTier`) the hot-block module that `NORD_SHARC_AOT_HOT=FILE cargo build`
//! compiles into `nord-sharc` (see `docs/research/sharc-aot-hot-block.md`).
//! `sharc-aot IMAGE --ranges A:B,C:D,... [--tier T] --out FILE` does the same for a list of
//! ranges, in the given order (`sharc_jit_diff --aot-top N` enables the first N)
//! (`docs/research/sharc-aot-multi-block.md`).
use nord_sharc::core::{AotOptions, AotTier, Core};
use std::process::ExitCode;

fn parse_number(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

fn run_cli(args: &[String]) -> Result<Option<String>, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut opts = AotOptions::default();
    let mut out_path: Option<String> = None;
    let mut ops = false;
    let mut hot = false;
    let mut tier = AotTier::Pe;
    let mut ranges: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fast-float" => opts.fast_float = true,
            "--ops" => ops = true,
            "--hot" => hot = true,
            "--ranges" => ranges = Some(it.next().ok_or("--ranges needs A:B,C:D,...")?.clone()),
            "--tier" => {
                tier = match it.next().map(String::as_str) {
                    Some("pe") => AotTier::Pe,
                    Some("guarded") => AotTier::Guarded,
                    Some("approx") => AotTier::Approx,
                    _ => return Err("--tier needs pe, guarded or approx".to_string()),
                };
            }
            "--fn" => {
                opts.fn_name = it.next().ok_or("--fn needs NAME")?.clone();
            }
            "--out" => {
                out_path = Some(it.next().ok_or("--out needs FILE")?.clone());
            }
            _ => positional.push(a.clone()),
        }
    }
    if let Some(list) = ranges {
        let [image] = positional.as_slice() else {
            return Err(
                "usage: sharc-aot IMAGE --ranges A:B,C:D,... [--tier T] [--out FILE]".into(),
            );
        };
        let pairs = list
            .split(',')
            .map(|r| {
                let (a, b) = r.split_once(':').ok_or("range needs START:LAST")?;
                Ok((
                    parse_number(a).ok_or("bad range START")? as u32,
                    parse_number(b).ok_or("bad range LAST")? as u32,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let data = std::fs::read(image).map_err(|e| format!("{image}: {e}"))?;
        let src = Core::new(&data).emit_hot_blocks_rust(&pairs, tier)?;
        return match out_path {
            Some(path) => std::fs::write(&path, &src)
                .map(|()| None)
                .map_err(|e| format!("{path}: {e}")),
            None => Ok(Some(src)),
        };
    }
    let [image, entry, max_insns] = positional.as_slice() else {
        return Err(
            "usage: sharc-aot IMAGE ENTRY MAX_INSNS [--fast-float] [--fn NAME] [--out FILE]"
                .to_string(),
        );
    };
    let data = std::fs::read(image).map_err(|e| format!("{image}: {e}"))?;
    let entry = parse_number(entry).ok_or("bad ENTRY")? as u32;
    let max_insns = parse_number(max_insns).ok_or("bad MAX_INSNS")? as usize;
    let core = Core::new(&data);
    if hot {
        let src = core.emit_hot_block_rust(entry, max_insns as u32, tier)?;
        return match out_path {
            Some(path) => std::fs::write(&path, &src)
                .map(|()| None)
                .map_err(|e| format!("{path}: {e}")),
            None => Ok(Some(src)),
        };
    }
    if ops {
        let mut text = String::new();
        for (pc, next, op) in core.aot_describe_range(entry, max_insns as u32) {
            text.push_str(&format!(
                "0x{pc:x}..0x{next:x} {op}
"
            ));
        }
        return Ok(Some(text));
    }
    let src = core.emit_aot_rust(entry, max_insns, &opts)?;
    if let Some(path) = out_path {
        std::fs::write(&path, &src).map_err(|e| format!("{path}: {e}"))?;
        Ok(None)
    } else {
        Ok(Some(src))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run_cli(&args) {
        Ok(Some(src)) => {
            print!("{src}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_rejects_bad_arguments_and_missing_image() {
        assert!(run_cli(&[]).is_err());
        assert!(run_cli(&["missing.bin".into(), "0x120000".into(), "4".into()]).is_err());
    }

    #[test]
    fn cli_lists_ops_and_emits_a_hot_block_for_a_pc_range() {
        let dir = std::env::temp_dir();
        let img = dir.join(format!("sharc_aot_hot_test_{}.bin", std::process::id()));
        let mut sram = vec![0u8; 0x25_0000];
        // R1 = 7 (0x0f81 0x0007 is a 32-bit load); NOP at short-word 0x12_0002
        let parcels = [0x0f81u16, 0x0007, 0x0001];
        for (k, p) in parcels.iter().enumerate() {
            let o = (0x12_0000 + k) * 2;
            sram[o..o + 2].copy_from_slice(&p.to_le_bytes());
        }
        std::fs::write(&img, &sram).unwrap();
        let image = img.to_string_lossy().to_string();
        let ops = run_cli(&[
            image.clone(),
            "0x120000".into(),
            "0x120002".into(),
            "--ops".into(),
        ])
        .unwrap()
        .unwrap();
        assert!(ops.contains("LoadImm"), "{ops}");
        let image2 = image.clone();
        let hot = run_cli(&[image, "0x120000".into(), "0x120002".into(), "--hot".into()])
            .unwrap()
            .unwrap();
        assert!(hot.contains("steps: 2"), "{hot}");
        let both = run_cli(&[
            image2,
            "--ranges".into(),
            "0x120000:0x120002,0x120002:0x120002".into(),
        ])
        .unwrap()
        .unwrap();
        let _ = std::fs::remove_file(&img);
        assert!(both.contains("static BLOCKS: [AotBlock; 2]"), "{both}");
        assert!(run_cli(&["x".into(), "--ranges".into(), "bad".into()]).is_err());
    }

    #[test]
    fn cli_transpiles_synthetic_image_to_rust() {
        let dir = std::env::temp_dir();
        let img = dir.join("sharc_aot_test.bin");
        let mut sram = vec![0u8; 0x25_0000];
        // Place R1 = 7 (0x0f81, 0x0007) + RTS (0x0abe) at short-word 0x12_0000.
        let parcels = [0x0f81u16, 0x0007, 0x0abe];
        for (k, p) in parcels.iter().enumerate() {
            let o = (0x12_0000 + k) * 2;
            sram[o..o + 2].copy_from_slice(&p.to_le_bytes());
        }
        std::fs::write(&img, &sram).unwrap();
        let out = run_cli(&[
            img.to_string_lossy().into(),
            "0x120000".into(),
            "4".into(),
            "--fast-float".into(),
            "--fn".into(),
            "test_fn".into(),
        ])
        .unwrap()
        .unwrap();
        let _ = std::fs::remove_file(&img);
        assert!(out.contains("pub fn test_fn("));
        assert!(out.contains("core.r[1] = 0x00000007;"));
        assert!(out.contains("return;"));
    }
}

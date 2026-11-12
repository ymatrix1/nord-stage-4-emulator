//! `sharc-disasm IMAGE START END`: VISA listing of a flat SRAM image (byte offsets, hex or decimal).
//!
//! Same output as `python tools/sharc_visa.py IMAGE START END`, so the two can be diffed.
use nord_sharc::visa::{listing_line, parcels_le, tile};
use std::process::ExitCode;

fn parse_number(s: &str) -> Option<usize> {
    match s.strip_prefix("0x") {
        Some(hex) => usize::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let [image, start, end] = args else {
        return Err("usage: sharc-disasm IMAGE START END".to_string());
    };
    let data = std::fs::read(image).map_err(|e| format!("{image}: {e}"))?;
    let start = parse_number(start).ok_or("bad START")?;
    let end = parse_number(end).ok_or("bad END")?.min(data.len());
    if start > end {
        return Err("START is after END".to_string());
    }
    let (insns, exact) = tile(&parcels_le(&data[start..end]));
    let mut out = String::new();
    let mut offset = start;
    for insn in &insns {
        out.push_str(&listing_line(offset, insn));
        out.push('\n');
        offset += 2 * insn.len;
    }
    out.push_str(if exact {
        "tiles exactly"
    } else {
        "does NOT tile exactly"
    });
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
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
    fn parses_hex_and_decimal() {
        assert_eq!(parse_number("0x10"), Some(16));
        assert_eq!(parse_number("32"), Some(32));
        assert_eq!(parse_number("zz"), None);
    }

    #[test]
    fn lists_a_small_image_and_rejects_bad_arguments() {
        let path = std::env::temp_dir().join("nord_sharc_disasm_test.bin");
        // I5 = 0xc0440 (17a), then a NOP-like 21/22 word pair
        std::fs::write(&path, [0x15, 0x0F, 0x0C, 0x00, 0x40, 0x04]).unwrap();
        let args = [
            path.to_string_lossy().to_string(),
            "0".to_string(),
            "6".to_string(),
        ];
        let text = run(&args).unwrap();
        assert!(text.contains("I5 = 0xc0440") && text.ends_with("tiles exactly"));
        assert!(run(&args[..2]).is_err());
        assert!(run(&[args[0].clone(), "9".to_string(), "4".to_string()]).is_err());
        assert!(run(&[
            "/nonexistent/file".to_string(),
            "0".to_string(),
            "4".to_string()
        ])
        .is_err());
        std::fs::remove_file(path).ok();
    }
}

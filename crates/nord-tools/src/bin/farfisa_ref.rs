//! `cargo run --release -p nord-tools --bin farfisa_ref -- [FRAMES] [OUT.csv]`
//!
//! Writes the free-running Farfisa divider (`FarfisaWheels`, spec 3.1.8) at the DSP frame rate as
//! `frame,row,oscillator,value` lines, the form `tools/organ_farfisa_oracle.py compare --rust`
//! reads to report the error against the SHARC oracle ring series.
use nord_core::organ::{FarfisaWheels, FARFISA_OSCILLATORS, FARFISA_ROWS};
use std::fmt::Write as _;

/// `frame,row,oscillator,value` lines for the first `frames` frames of the divider.
fn csv(frames: usize) -> String {
    let mut wheels = FarfisaWheels::new();
    let mut rows = [[0.0f32; FARFISA_OSCILLATORS]; FARFISA_ROWS];
    let mut text = String::new();
    for frame in 0..frames {
        wheels.advance_rows(&mut rows);
        for (r, row) in rows.iter().enumerate() {
            for (n, v) in row.iter().enumerate() {
                if 12 * r + n < 0x62 && (r < 8 || n < 2) {
                    let _ = writeln!(text, "{frame},{r},{n},{v:e}");
                }
            }
        }
    }
    text
}

fn main() {
    let mut args = std::env::args().skip(1);
    let frames: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(4000);
    let out = args.next();
    let text = csv(frames);
    match out {
        Some(path) => std::fs::write(path, text).expect("write csv"),
        None => print!("{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_has_four_fields_per_line_and_only_the_oracle_cells() {
        let text = csv(3);
        assert!(!text.is_empty());
        for line in text.lines() {
            let f: Vec<&str> = line.split(',').collect();
            assert_eq!(f.len(), 4, "{line}");
            let (r, n): (usize, usize) = (f[1].parse().unwrap(), f[2].parse().unwrap());
            assert!(12 * r + n < 0x62 && (r < 8 || n < 2), "{line}");
            assert!(f[3].parse::<f64>().unwrap().is_finite());
        }
    }

    #[test]
    fn csv_is_deterministic_and_grows_with_frames() {
        assert_eq!(csv(2), csv(2));
        assert!(csv(4).len() > csv(2).len());
    }
}

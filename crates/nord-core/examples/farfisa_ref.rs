//! `cargo run --release -p nord-core --example farfisa_ref -- [FRAMES] [OUT.csv]`
//!
//! Writes the free-running Farfisa divider (`FarfisaWheels`, spec 3.1.8) at the DSP frame rate as
//! `frame,row,oscillator,value` lines, the form `tools/organ_farfisa_oracle.py compare --rust`
//! reads to report the error against the SHARC oracle ring series.
use nord_core::organ::{FarfisaWheels, FARFISA_OSCILLATORS, FARFISA_ROWS};
use std::fmt::Write as _;

fn main() {
    let mut args = std::env::args().skip(1);
    let frames: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(4000);
    let out = args.next();
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
    match out {
        Some(path) => std::fs::write(path, text).expect("write csv"),
        None => print!("{text}"),
    }
}

//! `cargo run --release -p nord-core --example organ_ref -- [options]`
//!
//! Renders the organ tone source of the Rust engine for a chord, in the form the oracle chain
//! (`sharc-organ`, `tools/organ_oracle_audio.py`) produces, so that `tools/ns4_oracle_compare.py`
//! can compare the two. Method and results: `docs/research/organ-oracle-audio.md`.
//!
//! ```text
//! --mode layer       OrganLayer with percussion, crosstalk, rotary and scanner off (default)
//! --mode vox         the Vox note bank (`ToneWheelSource::Vox`, spec 3.1.7): weight 1 per key, the
//!                     output stage `0x1229B3`; drawbars, --weights and --detuned do not apply
//! --mode b3          the B3 per-key weight path (`ToneWheelSource::B3Wheels`, spec 3.1.6): wheel
//!                     tables times the captured `A * W` weights; --drawbars act (the oracle replay of
//!                     `keydown-b3-c4e4g4.script` has the 16', 5 1/3', 8' and 4' items: `888800000`)
//! --vibrato N        vox mode: vibrato type 0 (off), 1..3 (V1..V3: Vox depths 0.70, 1.15, 1.76 %)
//! --notes 60,64,67    MIDI notes (default C4 E4 G4)       --drawbars 888000000
//! --frames N          frames to render (default 16384)    --detuned   vibrato level 1: detuned table,
//!                     stereo-blend lanes in L/R (compare per lane with `organ_oracle_audio.py compare --lanes`)
//! --rate HZ           sample rate / bank frame rate (default: the rate at which wheel 0 equals the
//!                     firmware increment of the exact table at the image's unity tune, 43,156 Hz:
//!                     the oracle-matching rate; the real DSP frame rate is 43,402.34 Hz,
//!                     `docs/research/dsp-frame-rate.md`, and Vox runs need `--rate 43402.34375`)
//! --linear            DrawbarLaw::Linear instead of the firmware curve
//! --weights firmware WeightMode::Firmware: ARM per-wheel weights, one-pole smoothing, absolute scale
//!                     (default `legacy`); also drops the pickup tables and the key click
//! --level L           mix-stage level `L` in firmware mode (default 1.0)
//! --velocity V        MIDI velocity 1..127 of every key (firmware mode; default 127)
//! --keyoff-at N       releases all keys at frame N (default: never)
//! --out PREFIX        writes PREFIX.wav (float32 stereo) and PREFIX.csv (index,L,R)
//! ```
use nord_core::organ::{
    drawbar_gain_law, DrawbarLaw, OrganLayer, OrganModel, RotarySpeed, ScannerMode,
    ToneWheelSource, WeightMode,
};
use std::fmt::Write as _;

/// ARM exact 12-TET increment of element 0: `bank0_CODE@0x444188[0] << 8`
/// (`docs/research/data/arm-bank-increments.csv`).
const INC0_EXACT: u32 = 0x0063_5300;
/// Frequency of MIDI note 24 (wheel 0) in the engine (`vibrato_bank::wheel_hz(0, 0)`).
const WHEEL0_HZ: f64 = 32.703_195_66;

struct Opts {
    mode: String,
    notes: Vec<u8>,
    drawbars: [u8; 9],
    frames: usize,
    detuned: bool,
    rate: f64,
    linear: bool,
    firmware_weights: bool,
    level: f32,
    velocity: u8,
    keyoff_at: Option<usize>,
    vibrato: u8,
    out: String,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    // Cycles per frame of wheel 0 in the oracle: inc * tune / 2^32 with tune 0.5.
    let rate0 = WHEEL0_HZ / (f64::from(INC0_EXACT) * 0.5 / 4_294_967_296.0);
    let mut o = Opts {
        mode: "layer".into(),
        notes: vec![60, 64, 67],
        drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
        frames: 16384,
        detuned: false,
        rate: rate0,
        linear: false,
        firmware_weights: false,
        level: 1.0,
        velocity: 127,
        keyoff_at: None,
        vibrato: 0,
        out: "organ_ref".into(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |what: &str| it.next().cloned().ok_or(format!("{a} needs {what}"));
        match a.as_str() {
            "--mode" => o.mode = val("layer|bank")?,
            "--notes" => {
                o.notes = val("N,N")?
                    .split(',')
                    .map(|s| s.parse().map_err(|_| "bad note".to_string()))
                    .collect::<Result<_, _>>()?;
            }
            "--drawbars" => {
                let d = val("nine digits")?;
                if d.len() != 9 {
                    return Err("--drawbars needs nine digits".into());
                }
                for (slot, c) in o.drawbars.iter_mut().zip(d.chars()) {
                    *slot = c.to_digit(9).ok_or("drawbar digits are 0..8")? as u8;
                }
            }
            "--frames" => o.frames = val("N")?.parse().map_err(|_| "bad --frames")?,
            "--rate" => o.rate = val("HZ")?.parse().map_err(|_| "bad --rate")?,
            "--out" => o.out = val("PREFIX")?,
            "--detuned" => o.detuned = true,
            "--linear" => o.linear = true,
            "--weights" => {
                o.firmware_weights = match val("legacy|firmware")?.as_str() {
                    "legacy" => false,
                    "firmware" => true,
                    other => return Err(format!("--weights {other}: legacy or firmware")),
                }
            }
            "--level" => o.level = val("L")?.parse().map_err(|_| "bad --level")?,
            "--velocity" => {
                o.velocity = val("1..127")?.parse().map_err(|_| "bad --velocity")?;
                if !(1..=127).contains(&o.velocity) {
                    return Err("--velocity is 1..127".into());
                }
            }
            "--vibrato" => {
                o.vibrato = val("0..3")?.parse().map_err(|_| "bad --vibrato")?;
                if o.vibrato > 3 {
                    return Err("--vibrato is 0..3".into());
                }
            }
            "--keyoff-at" => o.keyoff_at = Some(val("N")?.parse().map_err(|_| "bad --keyoff-at")?),
            _ => return Err(format!("unknown option {a}")),
        }
    }
    Ok(o)
}

/// OrganLayer with everything but the tonewheel source, the drawbar sum and the key-click off.
fn render_layer(o: &Opts) -> Vec<[f32; 2]> {
    let mut organ = OrganLayer::new(o.rate as f32);
    organ.tonewheel_source = ToneWheelSource::Firmware;
    organ.detuned_copies = o.detuned;
    organ.percussion_enabled = false;
    organ.crosstalk_level = 0.0;
    organ.rotary_enabled = false;
    organ.rotary_speed = RotarySpeed::Slow;
    organ.scanner_mode = ScannerMode::Off;
    if o.mode == "vox" {
        organ.model = OrganModel::Vox;
        organ.tonewheel_source = ToneWheelSource::Vox;
        organ.key_click_enabled = false;
        organ.scanner_mode = [
            ScannerMode::Off,
            ScannerMode::V1,
            ScannerMode::V2,
            ScannerMode::V3,
        ][usize::from(o.vibrato)];
    }
    if o.mode == "b3" {
        organ.model = OrganModel::B3;
        organ.tonewheel_source = ToneWheelSource::B3Wheels;
        organ.key_click_enabled = false;
    }
    organ.drawbars = o.drawbars;
    organ.drawbar_law = if o.linear {
        DrawbarLaw::Linear
    } else {
        DrawbarLaw::Firmware
    };
    if o.firmware_weights {
        organ.weight_mode = WeightMode::Firmware;
        organ.weight_level = o.level;
        organ.key_click_enabled = false;
    }
    for &n in &o.notes {
        organ.note_on_with_gain(n, f32::from(o.velocity) / 127.0);
    }
    (0..o.frames)
        .map(|i| {
            if o.keyoff_at == Some(i) {
                for &n in &o.notes {
                    organ.note_off(n);
                }
            }
            organ.render_frame()
        })
        .collect()
}

fn write_outputs(o: &Opts, frames: &[[f32; 2]]) -> Result<(), String> {
    let mut csv = format!("# mode: {}\n# rate: {}\nindex,L,R\n", o.mode, o.rate);
    let mut pcm = Vec::with_capacity(frames.len() * 8);
    for (i, f) in frames.iter().enumerate() {
        writeln!(csv, "{i},{},{}", f[0], f[1]).map_err(|e| e.to_string())?;
        pcm.extend_from_slice(&f[0].to_le_bytes());
        pcm.extend_from_slice(&f[1].to_le_bytes());
    }
    std::fs::write(format!("{}.csv", o.out), csv).map_err(|e| e.to_string())?;
    let rate = o.rate.round() as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    for v in [16u32, 0x0002_0003, rate, rate * 8, 0x0020_0008] {
        wav.extend_from_slice(&v.to_le_bytes());
    }
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    std::fs::write(format!("{}.wav", o.out), wav).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = match parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let frames = render_layer(&o);
    if let Err(e) = write_outputs(&o, &frames) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    let peak = frames.iter().flatten().fold(0.0f32, |m, v| m.max(v.abs()));
    println!(
        "{} frames at {:.2} Hz, peak {peak:.4}, firmware gain of drawbar 8 = {}",
        frames.len(),
        o.rate,
        drawbar_gain_law(8, DrawbarLaw::Firmware)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn options_parse_and_reject_garbage() {
        let o = parse(&strings(&[
            "--notes",
            "60,64",
            "--drawbars",
            "808000000",
            "--detuned",
        ]))
        .unwrap();
        assert_eq!(
            (o.notes.clone(), o.drawbars[1], o.detuned),
            (vec![60, 64], 0, true)
        );
        assert!(parse(&strings(&["--bogus"])).is_err());
        assert!(parse(&strings(&["--drawbars", "88"])).is_err());
    }

    #[test]
    fn firmware_weight_options_parse_and_render() {
        let mut o = parse(&strings(&[
            "--weights",
            "firmware",
            "--velocity",
            "64",
            "--keyoff-at",
            "100",
        ]))
        .unwrap();
        assert!(o.firmware_weights && o.velocity == 64 && o.keyoff_at == Some(100));
        assert!(parse(&strings(&["--weights", "x"])).is_err());
        assert!(parse(&strings(&["--velocity", "0"])).is_err());
        o.frames = 400;
        let f = render_layer(&o);
        assert!(f.iter().flatten().all(|v| v.is_finite()));
        let rms = |a: usize, b: usize| {
            (f[a..b].iter().map(|v| v[0] * v[0]).sum::<f32>() / (b - a) as f32).sqrt()
        };
        assert!(rms(360, 400) < 0.5 * rms(0, 100), "release tail decays");
        assert!(rms(360, 400) > 0.0);
    }

    #[test]
    fn vox_mode_renders_the_note_bank_with_vibrato_options() {
        let mut o = parse(&strings(&["--mode", "vox", "--vibrato", "3"])).unwrap();
        assert_eq!((o.mode.as_str(), o.vibrato), ("vox", 3));
        assert!(parse(&strings(&["--vibrato", "4"])).is_err());
        o.frames = 4000;
        let f = render_layer(&o);
        assert!(f.iter().flatten().all(|v| v.is_finite()));
        let peak = f.iter().flatten().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.05 && peak < 0.3, "{peak}");
    }

    #[test]
    fn default_rate_puts_wheel_zero_on_the_firmware_increment() {
        let o = parse(&[]).unwrap();
        assert!((o.rate - 43_155.0).abs() < 5.0, "{}", o.rate);
    }

    #[test]
    fn layer_render_is_finite_and_not_silent() {
        let mut o = parse(&[]).unwrap();
        o.frames = 512;
        let f = render_layer(&o);
        assert!(f.iter().flatten().all(|v| v.is_finite()));
        assert!(f.iter().flatten().any(|v| v.abs() > 1e-3));
    }
}

//! Runs a mono `f32` file through the Mod 2 chorus of one layer FX chain and writes the stereo
//! result (interleaved little-endian `f32`). Driver of `tools/fx_chorus_clip.py`.
//!
//! `fx_chorus_probe <in.f32> <out.f32> <fw4|fw5|engine|engine5> <rate 0..127> <amount 0..127> <fs> [engine_rate_hz]`
//!
//! `fw4` / `fw5`: firmware `kFx2Type` 4 / 5 (`FirmwareFx::mod2`); `engine` / `engine5`: the
//! engine chorus and its variation (`engine_rate_hz`, amount `amount / 127`). Every other stage
//! is off.

use nord_core::fx::{FirmwareFx, LayerFxChain, Mod2Type};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 7 {
        eprintln!("usage: fx_chorus_probe <in.f32> <out.f32> <fw4|fw5|engine|engine5> <rate> <amount> <fs> [hz]");
        std::process::exit(2);
    }
    let raw = std::fs::read(&a[1]).expect("read input");
    let x: Vec<f32> = raw
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let (rate, amount): (u8, u32) = (a[4].parse().unwrap(), a[5].parse().unwrap());
    let fs: f32 = a[6].parse().unwrap();
    let mut fx = LayerFxChain::new(fs);
    fx.reverb_mix = 0.0;
    fx.mod2_type = Mod2Type::Chorus;
    fx.mod2_amount = amount as f32 / 127.0;
    match a[3].as_str() {
        "fw4" | "fw5" => {
            fx.firmware_fx = FirmwareFx {
                mod2: true,
                ..FirmwareFx::OFF
            };
            fx.mod2_type_raw = Some(if a[3] == "fw4" { 4 } else { 5 });
            fx.mod2_rate_raw = Some(rate);
        }
        _ => {
            fx.firmware_fx = FirmwareFx::OFF;
            fx.mod2_rate_hz = a.get(7).map_or(0.8, |s| s.parse().unwrap());
            fx.mod2_variation = a[3] == "engine5";
        }
    }
    let mut out = Vec::with_capacity(x.len() * 8);
    for &s in &x {
        let [l, r] = fx.process_frame([s, s]);
        out.extend_from_slice(&l.to_le_bytes());
        out.extend_from_slice(&r.to_le_bytes());
    }
    std::fs::write(&a[2], out).expect("write output");
}

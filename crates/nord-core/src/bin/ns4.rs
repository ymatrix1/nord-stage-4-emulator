//! `ns4` CLI binary for inspecting Clavia Binary (`CBIN`) files and running offline 7-layer DSP renders.

use nord_core::cbin::CbinFile;
use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;
use std::env;
use std::fs;

fn run_cli(args: &[String]) -> Result<String, String> {
    let cmd = args.get(1).map(String::as_str).unwrap_or("info");
    match cmd {
        "info" => Ok(
            "Nord Stage 4 Emulator (nord-core v0.1.0)\n\
             Targets: Standalone + CLAP/VST3 DAW Plugin\n\
             Engines: Organ (2x B3/Vox/Farf/Pipe + Rotary 122), Piano (2x + Modal String Resonance), Synth (3x Wave 2 PolyBLEP + ZDF Ladder)\n\
             Routing: 7 Independent Per-Layer FX Racks + 4 Split Zones + 3-Source Morph Matrix"
                .to_string(),
        ),
        "inspect" => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: ns4 inspect <file.ns4p|ns4y|ns4o|ns4n|ns4l|ns4t>".to_string())?;
            let bytes = fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
            let cbin = CbinFile::parse(&bytes).map_err(|e| e.to_string())?;
            Ok(format!(
                "File: {path}\nKind: {:?}\nBank: {}\nLocation: {}\nCategory: 0x{:02X}\nSchema: 0x{:04X}\nChecksum: 0x{:08X}\nPayload: {} bytes",
                cbin.header.kind,
                cbin.header.bank_index,
                cbin.header.location_index,
                cbin.header.category_id,
                cbin.header.schema_id,
                cbin.header.checksum,
                cbin.payload.len()
            ))
        }
        "render-demo" => {
            let mut engine = Stage4Engine::new(48_000.0);
            engine.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            });
            engine.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 64,
                velocity: 100,
            });
            engine.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: 67,
                velocity: 100,
            });
            let mut left = vec![0.0f32; 4096];
            let mut right = vec![0.0f32; 4096];
            engine.process_block(&mut left, &mut right);
            let peak = left
                .iter()
                .chain(right.iter())
                .fold(0.0f32, |acc, &x| acc.max(x.abs()));
            Ok(format!("Rendered 4096 stereo frames @ 48 kHz — peak amplitude: {peak:.4}"))
        }
        other => Err(format!(
            "unknown command '{other}' (expected: info | inspect <path> | render-demo)"
        )),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match run_cli(&args) {
        Ok(out) => println!("{out}"),
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_info_and_render_demo() {
        let info = run_cli(&["ns4".to_string(), "info".to_string()]).unwrap();
        assert!(info.contains("CLAP/VST3 DAW Plugin"));

        let demo = run_cli(&["ns4".to_string(), "render-demo".to_string()]).unwrap();
        assert!(demo.contains("Rendered 4096 stereo frames"));
    }
}

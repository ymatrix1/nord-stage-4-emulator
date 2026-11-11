//! `ns4` CLI binary for inspecting Clavia Binary (`CBIN`) files, converting patches to/from JSON, and running offline 7-layer DSP renders.

use nord_core::cbin::{CbinFile, CbinKind, ProgramDecoded};
use nord_core::engine::Stage4Engine;
use nord_core::firmware::FirmwareContainer;
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
                .ok_or_else(|| "usage: ns4 inspect <file.ns4p|ns4y|ns4o|ns4n|ns4l|ns4t|firmware.bin> [--json]".to_string())?;
            let bytes = fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;

            // Check if file is a Clavia firmware container (starts with 0x00000020 header size)
            if bytes.len() >= 32 && u32::from_be_bytes(bytes[0..4].try_into().unwrap_or([0; 4])) == 0x20 {
                let fw = FirmwareContainer::parse(&bytes).map_err(|e| e.to_string())?;
                let mut out = format!(
                    "Firmware Container: {path}\n\
                     Header Size: {} B\n\
                     Image 0 Size: {} B (Segments: {})\n\
                     Image 1 Size: {} B @ 0x{:X} (Segments: {})\n\
                     Discovered DSP Partitions: {}\n",
                    fw.header_size,
                    fw.image0_size,
                    fw.image0_segments.len(),
                    fw.image1_size,
                    fw.image1_offset,
                    fw.image1_segments.len(),
                    fw.dsp_partitions.len()
                );
                for p in &fw.dsp_partitions {
                    out.push_str(&format!("  [0x{:06X}] {:<12} {:?}: {}\n", p.offset, p.tag, p.engine_type, p.description));
                }
                return Ok(out);
            }

            let cbin = CbinFile::parse(&bytes).map_err(|e| e.to_string())?;

            if args.iter().any(|a| a == "--json") {
                if matches!(cbin.header.kind, CbinKind::Program | CbinKind::Live) {
                    let prog = cbin.decode_program().map_err(|e| e.to_string())?;
                    return prog.to_json().map_err(|e| e.to_string());
                } else if cbin.header.kind == CbinKind::OrganPreset {
                    let organ = cbin.decode_organ_preset().map_err(|e| e.to_string())?;
                    return serde_json::to_string_pretty(&organ).map_err(|e| e.to_string());
                } else if cbin.header.kind == CbinKind::PianoPreset {
                    let piano = cbin.decode_piano_preset().map_err(|e| e.to_string())?;
                    return serde_json::to_string_pretty(&piano).map_err(|e| e.to_string());
                } else if cbin.header.kind == CbinKind::SynthPreset {
                    let synth = cbin.decode_synth_preset().map_err(|e| e.to_string())?;
                    return serde_json::to_string_pretty(&synth).map_err(|e| e.to_string());
                }
            }

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
        "convert" => {
            let input = args
                .get(2)
                .ok_or_else(|| "usage: ns4 convert <input> [--to-json [out.json] | --to-ns4p [out.ns4p]]".to_string())?;
            let mode = args.get(3).map(String::as_str).unwrap_or("--to-json");
            match mode {
                "--to-json" => {
                    let bytes = fs::read(input).map_err(|e| format!("failed to read {input}: {e}"))?;
                    let cbin = CbinFile::parse(&bytes).map_err(|e| e.to_string())?;
                    let prog = cbin.decode_program().map_err(|e| e.to_string())?;
                    let json = prog.to_json().map_err(|e| e.to_string())?;
                    if let Some(out_path) = args.get(4) {
                        fs::write(out_path, &json)
                            .map_err(|e| format!("failed to write {out_path}: {e}"))?;
                        Ok(format!("Converted {input} -> {out_path}"))
                    } else {
                        Ok(json)
                    }
                }
                "--to-ns4p" => {
                    let json_str = fs::read_to_string(input)
                        .map_err(|e| format!("failed to read {input}: {e}"))?;
                    let prog = ProgramDecoded::from_json(&json_str).map_err(|e| e.to_string())?;
                    let cbin = prog.to_cbin_file(0, 0).map_err(|e| e.to_string())?;
                    let raw = cbin.to_bytes();
                    let out_path = args.get(4).cloned().unwrap_or_else(|| "output.ns4p".to_string());
                    fs::write(&out_path, &raw)
                        .map_err(|e| format!("failed to write {out_path}: {e}"))?;
                    Ok(format!("Converted {input} -> {out_path} ({} bytes)", raw.len()))
                }
                other => Err(format!("unknown convert mode '{other}' (expected --to-json or --to-ns4p)")),
            }
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
        "render-patch" => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: ns4 render-patch <file.ns4p> [--note N] [--frames F]".to_string())?;
            let bytes = fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
            let cbin = CbinFile::parse(&bytes).map_err(|e| e.to_string())?;
            let prog = cbin.decode_program().map_err(|e| e.to_string())?;

            let mut engine = Stage4Engine::new(48_000.0);
            engine.apply_program(&prog);

            let note = args
                .iter()
                .position(|a| a == "--note")
                .and_then(|idx| args.get(idx + 1))
                .and_then(|val| val.parse::<u8>().ok())
                .unwrap_or(60);

            let frames = args
                .iter()
                .position(|a| a == "--frames")
                .and_then(|idx| args.get(idx + 1))
                .and_then(|val| val.parse::<usize>().ok())
                .unwrap_or(4096);

            engine.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note,
                velocity: 100,
            });

            let mut left = vec![0.0f32; frames];
            let mut right = vec![0.0f32; frames];
            engine.process_block(&mut left, &mut right);

            let peak = left
                .iter()
                .chain(right.iter())
                .fold(0.0f32, |acc, &x| acc.max(x.abs()));

            let rms = (left
                .iter()
                .chain(right.iter())
                .map(|&x| x * x)
                .sum::<f32>()
                / (frames * 2) as f32)
                .sqrt();

            Ok(format!(
                "Applied program: {path}\n\
                 Rendered {frames} stereo frames @ 48 kHz (Note: {note})\n\
                 Peak amplitude: {peak:.4}\n\
                 RMS level: {rms:.4}"
            ))
        }
        other => Err(format!(
            "unknown command '{other}' (expected: info | inspect <path> [--json] | convert <path> ... | render-demo | render-patch <path>)"
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

    #[test]
    fn test_cli_convert_round_trip() {
        let prog = ProgramDecoded::default();
        let cbin = prog.to_cbin_file(1, 4).unwrap();
        let raw = cbin.to_bytes();

        let temp_dir = env::temp_dir();
        let ns4p_path = temp_dir.join("test_cli_prog.ns4p");
        let json_path = temp_dir.join("test_cli_prog.json");
        let roundtrip_ns4p = temp_dir.join("test_cli_prog_rt.ns4p");

        fs::write(&ns4p_path, &raw).unwrap();

        // Convert ns4p -> json
        let res_json = run_cli(&[
            "ns4".to_string(),
            "convert".to_string(),
            ns4p_path.to_str().unwrap().to_string(),
            "--to-json".to_string(),
            json_path.to_str().unwrap().to_string(),
        ])
        .unwrap();
        assert!(res_json.contains("Converted"));

        // Convert json -> ns4p
        let res_ns4p = run_cli(&[
            "ns4".to_string(),
            "convert".to_string(),
            json_path.to_str().unwrap().to_string(),
            "--to-ns4p".to_string(),
            roundtrip_ns4p.to_str().unwrap().to_string(),
        ])
        .unwrap();
        assert!(res_ns4p.contains("Converted"));

        let rt_bytes = fs::read(&roundtrip_ns4p).unwrap();
        assert_eq!(rt_bytes.len(), 868);

        // Clean up
        let _ = fs::remove_file(ns4p_path);
        let _ = fs::remove_file(json_path);
        let _ = fs::remove_file(roundtrip_ns4p);
    }

    #[test]
    fn test_cli_render_patch() {
        let prog = ProgramDecoded::default();
        let cbin = prog.to_cbin_file(0, 1).unwrap();
        let raw = cbin.to_bytes();

        let temp_dir = env::temp_dir();
        let ns4p_path = temp_dir.join("test_cli_render_patch.ns4p");
        fs::write(&ns4p_path, &raw).unwrap();

        let res = run_cli(&[
            "ns4".to_string(),
            "render-patch".to_string(),
            ns4p_path.to_str().unwrap().to_string(),
            "--note".to_string(),
            "64".to_string(),
            "--frames".to_string(),
            "2048".to_string(),
        ])
        .unwrap();

        assert!(res.contains("Applied program"));
        assert!(res.contains("Rendered 2048 stereo frames"));
        assert!(res.contains("Peak amplitude"));
        assert!(res.contains("RMS level"));

        let _ = fs::remove_file(ns4p_path);
    }

    #[test]
    fn test_cli_inspect_firmware_container() {
        let mut buf = vec![0u8; 512];
        // Top header (32 bytes)
        buf[0..4].copy_from_slice(&0x0000_0020u32.to_be_bytes());
        buf[4..8].copy_from_slice(&0x0000_0140u32.to_be_bytes()); // image0 = 320
        buf[8..12].copy_from_slice(&0x0000_0160u32.to_be_bytes()); // image1 = 352
        buf[12..16].copy_from_slice(&0x0000_00A0u32.to_be_bytes());

        // Image 0 subheader
        buf[0x30..0x34].copy_from_slice(&1u32.to_be_bytes());
        buf[0x34..0x38].copy_from_slice(b"SRAM");
        buf[0x38..0x3C].copy_from_slice(&0x02D4u32.to_be_bytes());
        buf[0x3C..0x40].copy_from_slice(&0x1000u32.to_be_bytes());
        buf[0x40..0x44].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        buf[0x44..0x48].copy_from_slice(&0xFFFF_0000u32.to_be_bytes());

        // DSP tags
        buf[400..408].copy_from_slice(b"DSPFxCor");
        buf[420..426].copy_from_slice(b"DSPOrg");
        buf[440..446].copy_from_slice(b"DSPSyn");

        let temp_dir = env::temp_dir();
        let fw_path = temp_dir.join("test_cli_fw.bin");
        fs::write(&fw_path, &buf).unwrap();

        let out = run_cli(&[
            "ns4".to_string(),
            "inspect".to_string(),
            fw_path.to_str().unwrap().to_string(),
        ])
        .unwrap();

        assert!(out.contains("Firmware Container"));
        assert!(out.contains("Image 0 Size: 320 B"));
        assert!(out.contains("Discovered DSP Partitions:"));
        assert!(out.contains("DSPOrg"));

        let _ = fs::remove_file(fw_path);
    }
}

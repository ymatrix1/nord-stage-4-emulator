//! `ns4` CLI binary for inspecting Clavia Binary (`CBIN`) files, converting patches to/from JSON, and running offline 7-layer DSP renders.

use nord_core::cbin::{CbinFile, CbinKind, ProgramDecoded};
use nord_core::engine::Stage4Engine;
use nord_core::firmware::FirmwareContainer;
use nord_core::midi::MidiEvent;
use nord_core::render;
use std::env;
use std::fs;

const SAMPLE_RATE: f32 = 48_000.0;
const RENDER_USAGE: &str = "usage: ns4 render-patch <file.ns4p | entry-in-backup> [--backup <file.ns4b>] [--wav <out.wav>] [--note N | --notes 60,64,67 | --events <notes.json> [--confirmed-only]] [--velocity V] [--frames F] [--hold F] [--sustain] [--cc ctrl:val@frame]...";

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
                let verify = args.iter().any(|a| a == "--verify");
                let unpack_dir = args
                    .iter()
                    .position(|a| a == "--unpack")
                    .and_then(|idx| args.get(idx + 1))
                    .cloned();
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
                if verify || unpack_dir.is_some() {
                    if let Some(ref dir) = unpack_dir {
                        fs::create_dir_all(dir)
                            .map_err(|e| format!("failed to create {dir}: {e}"))?;
                    }
                    out.push_str("Segments:\n");
                    for (bank, segs) in [(0u8, &fw.image0_segments), (1u8, &fw.image1_segments)] {
                        for seg in segs {
                            let extracted = fw.extract_segment(bank, &seg.name, &bytes)?;
                            let ok = seg.verify_checksum(&extracted);
                            out.push_str(&format!(
                                "  bank{}_{}: {} B @ 0x{:08X} checksum={}\n",
                                bank,
                                seg.name,
                                extracted.len(),
                                seg.target_address,
                                if ok { "OK" } else { "MISMATCH" }
                            ));
                            if verify && !ok {
                                return Err(format!(
                                    "checksum mismatch on bank {bank} segment {}",
                                    seg.name
                                ));
                            }
                            if let Some(ref dir) = unpack_dir {
                                let dest = std::path::Path::new(dir)
                                    .join(format!("bank{}_{}.bin", bank, seg.name));
                                fs::write(&dest, &extracted)
                                    .map_err(|e| format!("failed to write {}: {e}", dest.display()))?;
                            }
                        }
                    }
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

            if cbin.header.kind == CbinKind::PianoSample {
                let npno = nord_core::cbin::NpnoInstrument::parse(&bytes)?;
                let mut msg = format!(
                    "File: {path}\nKind: PianoSample (.npno)\nName: {}\nLong Name: {}\nChannels: {}\nStrokes: {}",
                    npno.name,
                    npno.long_name,
                    npno.channels,
                    npno.strokes.len()
                );
                if args.iter().any(|a| a == "--decode") {
                    let sound = nord_core::piano::NpnoSound::from_npno_bytes(&bytes)?;
                    let frames: usize = sound.strokes.iter().map(|s| s.pcm.len()).sum();
                    let peak = sound
                        .strokes
                        .iter()
                        .flat_map(|s| s.pcm.iter())
                        .fold(0.0f32, |m, &x| m.max(x.abs()));
                    msg.push_str(&format!(
                        "\nDecoded: {} strokes, {} frames, peak={peak:.4}, zones={}",
                        sound.strokes.len(),
                        frames,
                        sound.zones.len()
                    ));
                }
                return Ok(msg);
            }
            if cbin.header.kind == CbinKind::SampleLibrary {
                let nsmp = nord_core::cbin::Nsmp4Instrument::parse(&bytes).map_err(|e| e.to_string())?;
                let mut msg = format!(
                    "File: {path}\nKind: SampleLibrary (.nsmp4)\nName: {}\nZones: {}\nStrokes: {}",
                    nsmp.name,
                    nsmp.zones.len(),
                    nsmp.strokes.len()
                );
                if args.iter().any(|a| a == "--decode") {
                    let sound = nord_core::synth::SynthSampleSound::from_nsmp4_bytes(&bytes)
                        .map_err(|e| e.to_string())?;
                    let looped = sound
                        .zones
                        .iter()
                        .filter(|z| z.loop_start.is_some())
                        .count();
                    let stereo = sound
                        .zones
                        .iter()
                        .filter(|z| z.pcm_right.is_some())
                        .count();
                    let peak = sound
                        .zones
                        .iter()
                        .flat_map(|z| z.pcm.iter())
                        .fold(0.0f32, |m, &x| m.max(x.abs()));
                    msg.push_str(&format!(
                        "\nDecoded: {} zones ({looped} looped, {stereo} stereo), peak={peak:.4}",
                        sound.zones.len()
                    ));
                }
                return Ok(msg);
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
            let path = args.get(2).ok_or_else(|| RENDER_USAGE.to_string())?;
            let flag = |name: &str| -> Option<&String> {
                args.iter()
                    .position(|a| a == name)
                    .and_then(|idx| args.get(idx + 1))
            };
            let mut engine = Stage4Engine::new(SAMPLE_RATE);
            let mut sound_refs: Option<nord_core::cbin::ProgramSoundRefs> = None;

            if let Some(backup_path) = flag("--backup") {
                let archive = nord_core::cbin::Ns4BackupArchive::open(backup_path)
                    .map_err(|e| e.to_string())?;
                let mut file = fs::File::open(backup_path)
                    .map_err(|e| format!("failed to open {backup_path}: {e}"))?;
                let (_, refs) = engine
                    .load_program_with_sounds_from_backup(&archive, &mut file, path)
                    .map_err(|e| e.to_string())?;
                sound_refs = Some(refs);
            } else {
                let bytes = fs::read(path).map_err(|e| format!("failed to read {path}: {e}"))?;
                let cbin = CbinFile::parse(&bytes).map_err(|e| e.to_string())?;
                let prog = cbin.decode_program().map_err(|e| e.to_string())?;
                engine.apply_program(&prog);
            }

            let events_path = flag("--events");
            let notes = match flag("--notes") {
                Some(v) => render::parse_notes(v)?,
                None if events_path.is_some() => Vec::new(),
                None => vec![flag("--note")
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(60)],
            };
            let velocity = flag("--velocity")
                .map(|v| v.parse::<u8>().map_err(|_| format!("invalid --velocity '{v}'")))
                .transpose()?
                .unwrap_or(100)
                .min(127);
            let mut frames = flag("--frames")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(4096);
            let hold = flag("--hold")
                .map(|v| v.parse::<usize>().map_err(|_| format!("invalid --hold '{v}'")))
                .transpose()?;

            let mut events = Vec::new();
            for (i, a) in args.iter().enumerate() {
                if a == "--cc" {
                    let spec = args
                        .get(i + 1)
                        .ok_or_else(|| "missing value after --cc".to_string())?;
                    let (controller, value, frame) = render::parse_cc_spec(spec)?;
                    events.push(render::TimedEvent {
                        frame,
                        event: MidiEvent::ControlChange {
                            channel: 0,
                            controller,
                            value,
                        },
                    });
                }
            }
            if args.iter().any(|a| a == "--sustain") {
                events.push(render::TimedEvent {
                    frame: 0,
                    event: MidiEvent::ControlChange {
                        channel: 0,
                        controller: nord_core::midi::cc::SUSTAIN_PEDAL,
                        value: 127,
                    },
                });
            }
            if let Some(ep) = events_path {
                let json = fs::read_to_string(ep)
                    .map_err(|e| format!("failed to read {ep}: {e}"))?;
                let confirmed_only = args.iter().any(|a| a == "--confirmed-only");
                let (evs, last) = render::parse_note_events(
                    &json,
                    SAMPLE_RATE,
                    velocity,
                    confirmed_only,
                )?;
                events.extend(evs);
                if flag("--frames").is_none() {
                    frames = last + (SAMPLE_RATE as usize);
                }
            }
            for &note in &notes {
                events.push(render::TimedEvent {
                    frame: 0,
                    event: MidiEvent::NoteOn {
                        channel: 0,
                        note,
                        velocity,
                    },
                });
                if let Some(h) = hold {
                    events.push(render::TimedEvent {
                        frame: h,
                        event: MidiEvent::NoteOff {
                            channel: 0,
                            note,
                            velocity: 0,
                        },
                    });
                }
            }

            let (left, right) = render::render_timeline(&mut engine, &events, frames);
            let (peak, rms) = render::peak_rms(&left, &right);

            let mut out = format!(
                "Applied program: {path}
                 Rendered {frames} stereo frames @ 48 kHz (Notes: {notes:?}, velocity {velocity})
                 Peak amplitude: {peak:.4}
                 RMS level: {rms:.4}"
            );
            if let Some(r) = &sound_refs {
                out.push_str(&format!("
Sounds: {r:?}"));
            }
            if let Some(wav_path) = flag("--wav") {
                let wav = render::encode_wav_f32_stereo(SAMPLE_RATE as u32, &left, &right);
                fs::write(wav_path, &wav)
                    .map_err(|e| format!("failed to write {wav_path}: {e}"))?;
                out.push_str(&format!("
Wrote {wav_path} ({} bytes, 32-bit float stereo)", wav.len()));
            }
            Ok(out)
        }
        "backup" => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: ns4 backup <file.ns4b> [--list [ext]] [--extract <entry> <out>]".to_string())?;
            let archive = nord_core::cbin::Ns4BackupArchive::open(path).map_err(|e| e.to_string())?;
            let mut file = fs::File::open(path).map_err(|e| format!("failed to open {path}: {e}"))?;

            if let Some(pos) = args.iter().position(|a| a == "--extract") {
                let entry_query = args
                    .get(pos + 1)
                    .ok_or_else(|| "missing entry name after --extract".to_string())?;
                let out_path = args
                    .get(pos + 2)
                    .ok_or_else(|| "missing output path after --extract <entry>".to_string())?;
                let payload = archive
                    .read_entry(&mut file, entry_query)
                    .map_err(|e| e.to_string())?;
                fs::write(out_path, &payload)
                    .map_err(|e| format!("failed to write {out_path}: {e}"))?;
                return Ok(format!("Extracted {entry_query} ({} bytes) -> {out_path}", payload.len()));
            }

            if let Some(pos) = args.iter().position(|a| a == "--list") {
                let filter = args.get(pos + 1).map(String::as_str).unwrap_or("");
                let mut lines = Vec::new();
                for entry in archive.entries() {
                    if filter.is_empty()
                        || entry
                            .name
                            .to_ascii_lowercase()
                            .ends_with(&filter.to_ascii_lowercase())
                    {
                        lines.push(format!(
                            "{} ({} B, method={})",
                            entry.name, entry.uncompressed_size, entry.compression_method
                        ));
                    }
                }
                return Ok(lines.join("\n"));
            }

            let manifest = archive.read_manifest(&mut file).ok();
            let mut out = format!("Backup: {path}\nEntries: {}\n", archive.entries().len());
            if let Some(m) = manifest {
                out.push_str(&format!(
                    "Product ID: {} (version={}, build={}, stage4={})\n",
                    m.product_id,
                    m.product_version,
                    m.product_build,
                    m.is_nord_stage_4()
                ));
            }
            for ext in [".ns4p", ".npno", ".nsmp4", ".ns4y", ".ns4o", ".ns4n"] {
                out.push_str(&format!("  {ext}: {}\n", archive.entries_with_ext(ext).len()));
            }
            Ok(out)
        }
        other => Err(format!(
            "unknown command '{other}' (expected: info | inspect <path> [--json] [--verify] [--unpack <dir>] | backup <path.ns4b> [...] | convert <path> ... | render-demo | render-patch <path>)"
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
    fn test_cli_render_patch_wav_chord_and_pedal() {
        let prog = ProgramDecoded::default();
        let raw = prog.to_cbin_file(0, 1).unwrap().to_bytes();
        let dir = env::temp_dir();
        let ns4p = dir.join("ns4_rp_chord.ns4p");
        let wav = dir.join("ns4_rp_chord.wav");
        fs::write(&ns4p, &raw).unwrap();
        let s = |x: &str| x.to_string();

        let out = run_cli(&[
            s("ns4"),
            s("render-patch"),
            ns4p.to_string_lossy().to_string(),
            s("--notes"),
            s("60,64,67"),
            s("--velocity"),
            s("90"),
            s("--frames"),
            s("6000"),
            s("--hold"),
            s("3000"),
            s("--cc"),
            s("64:127@0"),
            s("--wav"),
            wav.to_string_lossy().to_string(),
        ])
        .unwrap();
        assert!(out.contains("[60, 64, 67]"));
        assert!(out.contains("Rendered 6000 stereo frames"));
        let bytes = fs::read(&wav).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 6000 * 8);

        assert!(run_cli(&[s("ns4"), s("render-patch")]).is_err());
        assert!(run_cli(&[
            s("ns4"),
            s("render-patch"),
            ns4p.to_string_lossy().to_string(),
            s("--cc"),
            s("bogus"),
        ])
        .is_err());

        let _ = fs::remove_file(ns4p);
        let _ = fs::remove_file(wav);
    }

    #[test]
    fn test_cli_render_patch_from_backup_entry() {
        let prog = ProgramDecoded {
            name: "FromBackup".to_string(),
            ..Default::default()
        };
        let ns4p = prog.to_cbin_file(0, 0).unwrap().to_bytes();
        let name = b"Program/From Backup.ns4p";
        let sz = ns4p.len() as u32;
        let mut zip = Vec::new();
        zip.extend_from_slice(b"PK");
        zip.extend_from_slice(&20u16.to_le_bytes());
        zip.extend_from_slice(&[0u8; 12]);
        zip.extend_from_slice(&sz.to_le_bytes());
        zip.extend_from_slice(&sz.to_le_bytes());
        zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(name);
        zip.extend_from_slice(&ns4p);
        let mut cd = Vec::new();
        cd.extend_from_slice(b"PK");
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&[0u8; 12]);
        cd.extend_from_slice(&sz.to_le_bytes());
        cd.extend_from_slice(&sz.to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&[0u8; 12]);
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(name);
        let cd_off = zip.len() as u32;
        let cd_sz = cd.len() as u32;
        zip.extend_from_slice(&cd);
        zip.extend_from_slice(b"PK    ");
        zip.extend_from_slice(&1u16.to_le_bytes());
        zip.extend_from_slice(&1u16.to_le_bytes());
        zip.extend_from_slice(&cd_sz.to_le_bytes());
        zip.extend_from_slice(&cd_off.to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());

        let path = env::temp_dir().join("ns4_rp_backup.ns4b");
        fs::write(&path, &zip).unwrap();
        let out = run_cli(&[
            "ns4".to_string(),
            "render-patch".to_string(),
            "From Backup.ns4p".to_string(),
            "--backup".to_string(),
            path.to_string_lossy().to_string(),
            "--frames".to_string(),
            "1024".to_string(),
        ])
        .unwrap();
        assert!(out.contains("Applied program: From Backup.ns4p"));
        assert!(out.contains("Rendered 1024 stereo frames"));

        let missing = run_cli(&[
            "ns4".to_string(),
            "render-patch".to_string(),
            "Nope.ns4p".to_string(),
            "--backup".to_string(),
            path.to_string_lossy().to_string(),
        ]);
        assert!(missing.is_err());
        let _ = fs::remove_file(path);
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

    #[test]
    fn test_cli_inspect_firmware_verify_and_unpack() {
        use nord_core::firmware::{calculate_checksum, compress_clavia_lz};

        let sram0_raw = vec![0xAAu8; 32];
        let code0_raw: Vec<u8> = (0..128u16).map(|i| ((i * 7) & 0xFF) as u8).collect();
        let code0_comp = compress_clavia_lz(&code0_raw);

        let code1_raw = b"Bank 1 uncompressed or LZO1X CODE segment payload!!".to_vec();
        let code1_comp = compress_clavia_lz(&code1_raw);

        // Layout:
        // Top header: 0x20 bytes
        // Image 0 subheader starts at 0x20 (header_size), segment descriptors at 0x34 and 0x60 (44 bytes each -> ends at 0x8C)
        // Image 0 bank_base = 0x20.
        // Image 1 starts at image1_offset = 0x200.
        let image0_bank_base = 0x20usize;
        let sram0_off = 0xA0usize;
        let code0_off = sram0_off + sram0_raw.len();
        let image1_offset = 0x200usize;

        let code1_rel_off = 0x50usize;
        let code1_abs_off = image1_offset + code1_rel_off;
        let total_len = code1_abs_off + code1_comp.len();

        let mut buf = vec![0u8; total_len];
        buf[0..4].copy_from_slice(&0x20u32.to_be_bytes());
        buf[4..8].copy_from_slice(&((image1_offset - image0_bank_base) as u32).to_be_bytes());
        buf[8..12].copy_from_slice(&(image1_offset as u32).to_be_bytes());
        buf[12..16].copy_from_slice(&((total_len - image1_offset) as u32).to_be_bytes());

        // Image 0 subheader at 0x30 (2 segments, 44 bytes per descriptor)
        buf[0x30..0x34].copy_from_slice(&2u32.to_be_bytes());
        // Seg 0: SRAM at 0x34
        buf[0x34..0x38].copy_from_slice(b"SRAM");
        buf[0x38..0x3C].copy_from_slice(&((sram0_off - image0_bank_base) as u32).to_be_bytes());
        buf[0x3C..0x40].copy_from_slice(&(sram0_raw.len() as u32).to_be_bytes());
        buf[0x40..0x44].copy_from_slice(&0x0000_0000u32.to_be_bytes());
        buf[0x44..0x48].copy_from_slice(&calculate_checksum(&sram0_raw).to_be_bytes());
        buf[sram0_off..sram0_off + sram0_raw.len()].copy_from_slice(&sram0_raw);

        // Seg 1: CODE at 0x60 (= 0x34 + 44)
        buf[0x60..0x64].copy_from_slice(b"CODE");
        buf[0x64..0x68].copy_from_slice(&((code0_off - image0_bank_base) as u32).to_be_bytes());
        buf[0x68..0x6C].copy_from_slice(&(code0_raw.len() as u32).to_be_bytes());
        buf[0x6C..0x70].copy_from_slice(&0x4000_0000u32.to_be_bytes());
        buf[0x70..0x74].copy_from_slice(&calculate_checksum(&code0_raw).to_be_bytes());
        buf[code0_off..code0_off + code0_comp.len()].copy_from_slice(&code0_comp);

        // Image 1 subheader at image1_offset + 0x10 (1 segment: CODE at +0x14, 44 bytes)
        buf[image1_offset + 0x10..image1_offset + 0x14].copy_from_slice(&1u32.to_be_bytes());
        buf[image1_offset + 0x14..image1_offset + 0x18].copy_from_slice(b"CODE");
        buf[image1_offset + 0x18..image1_offset + 0x1C]
            .copy_from_slice(&(code1_rel_off as u32).to_be_bytes());
        buf[image1_offset + 0x1C..image1_offset + 0x20]
            .copy_from_slice(&(code1_raw.len() as u32).to_be_bytes());
        buf[image1_offset + 0x20..image1_offset + 0x24]
            .copy_from_slice(&0x4000_0000u32.to_be_bytes());
        buf[image1_offset + 0x24..image1_offset + 0x28]
            .copy_from_slice(&calculate_checksum(&code1_raw).to_be_bytes());
        buf[code1_abs_off..code1_abs_off + code1_comp.len()].copy_from_slice(&code1_comp);

        let fw_path = std::env::temp_dir().join("ns4_test_fw_unpack.bin");
        let unpack_dir = std::env::temp_dir().join("ns4_test_fw_unpack_out");
        let _ = fs::remove_dir_all(&unpack_dir);
        fs::write(&fw_path, &buf).unwrap();

        let out = run_cli(&[
            "ns4".to_string(),
            "inspect".to_string(),
            fw_path.to_string_lossy().to_string(),
            "--verify".to_string(),
            "--unpack".to_string(),
            unpack_dir.to_string_lossy().to_string(),
        ])
        .unwrap();

        assert!(out.contains("bank0_SRAM: 32 B @ 0x00000000 checksum=OK"));
        assert!(out.contains("bank0_CODE: 128 B @ 0x40000000 checksum=OK"));
        assert!(out.contains("bank1_CODE:"));
        assert!(out.contains("checksum=OK"));
        assert_eq!(
            fs::read(unpack_dir.join("bank0_SRAM.bin")).unwrap(),
            sram0_raw
        );
        assert_eq!(
            fs::read(unpack_dir.join("bank0_CODE.bin")).unwrap(),
            code0_raw
        );
        assert_eq!(
            fs::read(unpack_dir.join("bank1_CODE.bin")).unwrap(),
            code1_raw
        );

        let _ = fs::remove_file(fw_path);
        let _ = fs::remove_dir_all(unpack_dir);
    }

    #[test]
    fn test_cli_inspect_nsmp4_and_backup_subcommand() {
        // Minimal valid .nsmp4 file in temp dir
        let dir = std::env::temp_dir().join("nord_stage4_nsmp_cli_test");
        let _ = fs::create_dir_all(&dir);
        let nsmp_path = dir.join("test_cli.nsmp4");
        let mut data = vec![0u8; 48];
        data[0..4].copy_from_slice(b"CBIN");
        data[4..8].copy_from_slice(&1u32.to_le_bytes());
        data[8..12].copy_from_slice(b"nsmp");
        data[0x2c..0x30].copy_from_slice(b"NSMP");
        data.extend_from_slice(&[0u8; 12]);
        let mut hdr = vec![0u8; 64];
        hdr[10..17].copy_from_slice(b"Strings");
        data.extend_from_slice(b"\0hdr");
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&(hdr.len() as u32).to_be_bytes());
        data.extend_from_slice(&hdr);
        fs::write(&nsmp_path, &data).unwrap();

        let out = run_cli(&[
            "ns4".to_string(),
            "inspect".to_string(),
            nsmp_path.to_string_lossy().to_string(),
        ])
        .unwrap();
        assert!(out.contains("SampleLibrary (.nsmp4)"));
        assert!(out.contains("Strings"));

        let _ = fs::remove_dir_all(dir);
    }
}

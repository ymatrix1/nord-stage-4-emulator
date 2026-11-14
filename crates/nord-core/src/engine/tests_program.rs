//! Engine tests for rendering, Program application, sound loading and FX morph.

use super::*;

#[test]
fn test_7_layer_engine_render_morph_and_scene_ii() {
    let mut engine = Stage4Engine::new(48_000.0);
    // Assign ModWheel morph to boost Synth A filter cutoff
    engine.synth_cutoff_morph[0].delta_wheel = 0.45;

    engine.handle_midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 110,
    });
    engine.handle_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: cc::MOD_WHEEL,
        value: 127,
    });

    let mut left = [0.0f32; 256];
    let mut right = [0.0f32; 256];
    engine.process_block(&mut left, &mut right);

    let energy: f32 = left
        .iter()
        .zip(right.iter())
        .map(|(l, r)| l * l + r * r)
        .sum();
    assert!(
        energy > 0.05,
        "7-layer engine must output non-zero stereo audio"
    );
    assert!(left.iter().all(|x| x.is_finite()));

    // Toggle Scene II via CC85
    engine.handle_midi(MidiEvent::ControlChange {
        channel: 0,
        controller: cc::LAYER_SCENE_II,
        value: 127,
    });
    assert!(engine.scene.scene_2_active);
}

#[test]
fn test_engine_apply_decoded_program() {
    let mut engine = Stage4Engine::new(48_000.0);
    let mut prog = crate::cbin::ProgramDecoded::default();
    prog.organ.model = crate::organ::OrganModel::Vox;
    prog.organ.drawbars_a = [8, 0, 4, 0, 2, 0, 1, 0, 0];
    prog.organ.percussion_soft = true;
    prog.global.tempo_bpm = 145.0;
    prog.global.master_compressor = 8.0;
    prog.piano.layer_a.timbre = crate::piano::PianoTimbre::Bright;
    prog.synth.layer_a.filter_cutoff_hz = 1500.0;

    engine.apply_program(&prog);

    assert_eq!(engine.organ_a.model, crate::organ::OrganModel::Vox);
    assert_eq!(engine.organ_a.drawbars, [8, 0, 4, 0, 2, 0, 1, 0, 0]);
    assert!(engine.organ_a.percussion_soft);
    assert_eq!(engine.master_clock.bpm, 145.0);
    assert_eq!(engine.piano_a.timbre, crate::piano::PianoTimbre::Bright);
    assert_eq!(engine.synth_a.cutoff_hz, 1500.0);

    // Synth category routing check (when scene_1_mask == 0 & Piano A fallback artifact occurs)
    let mut synth_prog = crate::cbin::ProgramDecoded {
        category_id: 0x2D, // Synth
        ..Default::default()
    };
    synth_prog.global.scene_1_mask = 0;
    synth_prog.organ.layer_a_enabled = false;
    synth_prog.synth.layer_a.enabled = false;
    synth_prog.synth.layer_a.waveform = crate::synth::VaWaveform::Pulse;
    engine.apply_program(&synth_prog);
    assert_ne!(engine.scene.scene_1_mask & (1 << 4), 0);
    assert_eq!(engine.synth_a.waveform, crate::synth::VaWaveform::Pulse);

    // Organ category routing check
    let mut organ_prog = crate::cbin::ProgramDecoded {
        category_id: 0x07, // Organ
        ..Default::default()
    };
    organ_prog.global.scene_1_mask = 0;
    organ_prog.organ.layer_a_enabled = false;
    organ_prog.synth.layer_a.enabled = false;
    organ_prog.organ.model = crate::organ::OrganModel::Farfisa;
    engine.apply_program(&organ_prog);
    assert_ne!(engine.scene.scene_1_mask & (1 << 0), 0);
    assert_eq!(engine.organ_a.model, crate::organ::OrganModel::Farfisa);
}

#[test]
fn test_engine_apply_program_wires_fx_morph_and_preserves_highpass() {
    let mut engine = Stage4Engine::new(48_000.0);
    let mut prog = crate::cbin::ProgramDecoded::default();
    prog.organ.vibrato_on = true;
    prog.organ.vibrato_type = 5; // C3
    prog.organ.rotary_enabled = false;
    prog.organ.fx.mod2_enabled = true;
    prog.organ.fx.mod2_type_raw = 0; // Phaser
    prog.organ.fx.mod2_amount = 0.6;
    prog.organ.fx.amp_enabled = true;
    prog.organ.fx.eq_mid_db = -4.0;

    prog.synth.layer_a.filter_model = crate::synth::FilterModel::HighPass;
    prog.synth.layer_a.filter_cutoff_hz = 6500.0;
    prog.synth.layer_a.filter_cutoff_morph.wheel = 0.25;
    prog.synth.layer_a.level = 0.0;
    prog.synth.layer_a.level_morph.wheel = 0.85;
    prog.synth.fx_a.reverb_enabled = true;
    prog.synth.fx_a.reverb_type_raw = 4; // Hall
    prog.synth.fx_a.reverb_amount = 0.45;
    prog.synth.fx_a.delay_enabled = true;
    prog.synth.fx_a.delay_mix = 0.35;

    engine.apply_program(&prog);

    assert_eq!(engine.organ_a.scanner_mode, crate::organ::ScannerMode::C3);
    assert!(!engine.organ_a.rotary_enabled);
    assert_eq!(engine.fx_chains[0].mod2_type, crate::fx::Mod2Type::Phaser);
    assert!((engine.fx_chains[0].eq_mid_db + 4.0).abs() < 1e-4);

    assert_eq!(
        engine.synth_a.filter_model,
        crate::synth::FilterModel::HighPass
    );
    assert_eq!(engine.synth_a.cutoff_hz, 6500.0);
    assert!((engine.synth_cutoff_morph[0].delta_wheel - 0.25).abs() < 1e-4);
    assert_eq!(engine.layer_levels[4].base, 0.0);
    assert!((engine.layer_levels[4].delta_wheel - 0.85).abs() < 1e-4);
    assert_eq!(
        engine.fx_chains[4].reverb_model,
        crate::fx::ReverbModel::Hall
    );
    assert!((engine.fx_chains[4].reverb_mix - 0.45).abs() < 1e-4);
    assert!(engine.fx_chains[4].delay_enabled);
    assert!((engine.fx_chains[4].delay_mix - 0.35).abs() < 1e-4);
}

#[test]
fn apply_program_routes_synth_layer_params_through_synth_params() {
    let mut engine = Stage4Engine::new(48_000.0);
    let mut prog = crate::cbin::ProgramDecoded::default();
    let l = &mut prog.synth.layer_b;
    l.unison = 2;
    l.legato = true;
    l.glide = 5.0;
    l.filter_env_dedicated = true;
    l.filter_env_decay_ms = 500.0;
    l.lfo_dest = crate::synth::LfoDestination::OscPitch;
    engine.apply_program(&prog);

    let s = &engine.synth_b;
    assert_eq!(s.unison, 2);
    assert!(s.mono && s.legato);
    assert!(s.glide_sec > 0.0);
    assert!(s.filter_env_dedicated);
    assert!((s.filter_env_decay_sec - 0.5).abs() < 1e-6);
    assert_eq!(s.lfo_dest, crate::synth::LfoDestination::OscPitch);
    // Untouched layers keep polyphonic defaults.
    assert!(!engine.synth_a.mono);
}

#[test]
fn load_synth_nsmp4_and_program_from_backup_helpers() {
    let mut engine = Stage4Engine::new(48_000.0);

    // Invalid synth layer index rejected cleanly
    assert!(engine.load_synth_nsmp4_bytes(3, &[]).is_err());

    // Valid minimal .nsmp4 loaded into Synth Layer B (index 1)
    let mut data = vec![0u8; 48];
    data[0..4].copy_from_slice(b"CBIN");
    data[4..8].copy_from_slice(&1u32.to_le_bytes());
    data[8..12].copy_from_slice(b"nsmp");
    data[0x2c..0x30].copy_from_slice(b"NSMP");
    data.extend_from_slice(&[0u8; 12]);
    let mut hdr = vec![0u8; 64];
    hdr[10..15].copy_from_slice(b"Pad1\0");
    data.extend_from_slice(b"\0hdr");
    data.extend_from_slice(&1u32.to_be_bytes());
    data.extend_from_slice(&(hdr.len() as u32).to_be_bytes());
    data.extend_from_slice(&hdr);

    let snd = engine
        .load_synth_nsmp4_bytes(1, &data)
        .expect("empty-stroke nsmp4 parses");
    assert_eq!(snd.name, "Pad1");
    assert_eq!(engine.synth_b.loaded_sample_name(), Some("Pad1"));

    // Auto-resolve .npno and .nsmp4 when loading .ns4p from .ns4b
    let piano_stem = "Astoria Grand Lrg 6.1";
    let sample_stem = "Ac Guitar Finger_ST Mart 4.1";
    let mut npno = vec![0u8; 48];
    npno[0..4].copy_from_slice(b"CBIN");
    npno[4..8].copy_from_slice(&1u32.to_le_bytes());
    npno[8..12].copy_from_slice(b"npno");
    let mut pno_hdr = vec![0u8; 0x90];
    pno_hdr[0x48..0x48 + 7].copy_from_slice(b"Astoria");
    pno_hdr[0x68..0x68 + 7].copy_from_slice(b"Astoria");
    npno.extend_from_slice(b"\0hdr");
    npno.extend_from_slice(&1u32.to_be_bytes());
    npno.extend_from_slice(&(pno_hdr.len() as u32).to_be_bytes());
    npno.extend_from_slice(&pno_hdr);

    let mut prog = crate::cbin::ProgramDecoded {
        name: "Split Resolve".to_string(),
        ..Default::default()
    };
    prog.global.scene_1_mask = (1 << 2) | (1 << 5); // Piano A + Synth B
    prog.piano.layer_a.enabled = true;
    prog.piano.layer_a.name_hash = crate::cbin::nord_name_hash(piano_stem);
    prog.synth.layer_b.enabled = true;
    prog.synth.layer_b.osc_mode = crate::synth::SynthOscMode::Sample;
    prog.synth.layer_b.sample_id = crate::cbin::nord_name_hash(sample_stem);
    let ns4p_bytes = prog.to_cbin_file(0, 0).expect("pack ns4p").to_bytes();

    let build_test_zip = |entries: &[(&str, &[u8])]| -> Vec<u8> {
        let mut archive = Vec::new();
        let mut cd = Vec::new();
        for &(name, payload) in entries {
            let lfh_off = archive.len() as u32;
            let n = name.as_bytes();
            let sz = payload.len() as u32;
            archive.extend_from_slice(b"PK\x03\x04");
            archive.extend_from_slice(&20u16.to_le_bytes());
            archive.extend_from_slice(&[0u8; 12]);
            archive.extend_from_slice(&sz.to_le_bytes());
            archive.extend_from_slice(&sz.to_le_bytes());
            archive.extend_from_slice(&(n.len() as u16).to_le_bytes());
            archive.extend_from_slice(&0u16.to_le_bytes());
            archive.extend_from_slice(n);
            archive.extend_from_slice(payload);

            cd.extend_from_slice(b"PK\x01\x02");
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&[0u8; 12]);
            cd.extend_from_slice(&sz.to_le_bytes());
            cd.extend_from_slice(&sz.to_le_bytes());
            cd.extend_from_slice(&(n.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0u8; 12]);
            cd.extend_from_slice(&lfh_off.to_le_bytes());
            cd.extend_from_slice(n);
        }
        let cd_off = archive.len() as u32;
        let cd_sz = cd.len() as u32;
        let cnt = entries.len() as u16;
        archive.extend_from_slice(&cd);
        archive.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        archive.extend_from_slice(&cnt.to_le_bytes());
        archive.extend_from_slice(&cnt.to_le_bytes());
        archive.extend_from_slice(&cd_sz.to_le_bytes());
        archive.extend_from_slice(&cd_off.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive
    };
    let zip_bytes = build_test_zip(&[
        ("Program/Split Resolve.ns4p", &ns4p_bytes),
        ("Piano/Grand/Astoria Grand Lrg 6.1.npno", &npno),
        ("Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4", &data),
    ]);
    let archive = crate::cbin::Ns4BackupArchive::from_bytes(&zip_bytes).expect("parse backup");
    let mut cursor = std::io::Cursor::new(&zip_bytes);
    let mut eng2 = Stage4Engine::new(48_000.0);
    let (_dec, refs) = eng2
        .load_program_with_sounds_from_backup(&archive, &mut cursor, "Program/Split Resolve.ns4p")
        .expect("load program with resolved sounds");
    assert_eq!(
        refs.piano_a.as_deref(),
        Some("Piano/Grand/Astoria Grand Lrg 6.1.npno")
    );
    assert_eq!(
        refs.synth_b.as_deref(),
        Some("Samp Lib/Guitar/Ac Guitar Finger_ST Mart 4.1.nsmp4")
    );
    assert_eq!(eng2.synth_b.loaded_sample_name(), Some("Pad1"));
}

#[test]
fn test_fx_morph_loaded_from_program_and_applied_per_frame() {
    let mut prog = crate::cbin::ProgramDecoded::default();
    prog.organ.fx.reverb_enabled = true;
    prog.organ.fx.reverb_amount = 0.2;
    prog.organ.fx.reverb_amount_morph = crate::cbin::MorphDeltas {
        wheel: 0.5,
        aftertouch: 0.0,
        ctrl_pedal: 0.0,
    };
    prog.organ.fx.delay_enabled = false;
    prog.organ.fx.delay_mix_morph.wheel = 1.0;
    let mut engine = Stage4Engine::new(48_000.0);
    engine.apply_program(&prog);
    engine.morph_controllers.mod_wheel = 1.0;
    engine.render_frame();
    assert!((engine.fx_chains[0].reverb_mix - 0.7).abs() < 1e-4);
    assert!(
        engine.fx_chains[0].delay_mix.abs() < 1e-6,
        "disabled slot must not be morphed"
    );
}

#[test]
fn test_amp_mode_mapping_from_program() {
    use crate::fx::AmpCabinet;
    let cases: [(u16, AmpCabinet, bool); 10] = [
        (0, AmpCabinet::Off, false),
        (33, AmpCabinet::Twin, false),      // 0b00100001: mode 1
        (50, AmpCabinet::Twin, true),       // mode 2
        (67, AmpCabinet::Jc, false),        // mode 3
        (340, AmpCabinet::Jc, true),        // mode 4
        (101, AmpCabinet::Small, false),    // mode 5
        (118, AmpCabinet::Small, true),     // mode 6
        (135, AmpCabinet::ToRotary, false), // mode 7
        (168, AmpCabinet::LpFilter, false), // mode 8
        (201, AmpCabinet::HpFilter, false), // mode 9
    ];
    for (raw, cab, var) in cases {
        let mut prog = crate::cbin::ProgramDecoded::default();
        prog.organ.fx.amp_enabled = true;
        prog.organ.fx.amp_type_new = raw;
        let mut engine = Stage4Engine::new(48_000.0);
        engine.apply_program(&prog);
        assert_eq!(engine.fx_chains[0].amp_cabinet, cab, "raw {raw}");
        assert_eq!(engine.fx_chains[0].amp_variation, var, "raw {raw}");
    }
}

#[test]
fn b3_bass_program_sets_the_bass_mode_and_exports_it_back() {
    let mut prog = crate::cbin::ProgramDecoded::default();
    prog.organ.b3_bass_a = true;
    let mut engine = Stage4Engine::new(48_000.0);
    engine.apply_program(&prog);
    assert!(engine.organ_a.b3_bass_mode && !engine.organ_b.b3_bass_mode);
    assert_eq!(engine.organ_a.model, crate::organ::OrganModel::B3);
    let out = engine.export_program(&prog);
    assert!(out.organ.b3_bass_a && !out.organ.b3_bass_b);
}

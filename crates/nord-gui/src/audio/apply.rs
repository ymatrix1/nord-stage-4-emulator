//! Applies `AudioCommand`s to the `Stage4Engine` on the audio thread.

use super::AudioCommand;
use nord_core::engine::Stage4Engine;
use nord_core::organ::ScannerMode;

/// Applies a single `AudioCommand` to the `Stage4Engine` and `master_vol`.
pub fn apply_audio_command(cmd: AudioCommand, engine: &mut Stage4Engine, master_vol: &mut f32) {
    match cmd {
        AudioCommand::Midi(ev) => engine.handle_midi(ev),
        AudioCommand::SetMasterVolume(v) => *master_vol = v.clamp(0.0, 1.0),
        AudioCommand::SetDrawbars(d) => engine.organ_a.drawbars = d,
        AudioCommand::SetOrganModel(m) => {
            engine.organ_a.model = m;
            engine.organ_b.model = m;
        }
        AudioCommand::SetScannerMode(sm) => {
            engine.organ_a.scanner_mode = sm;
        }
        AudioCommand::SetScannerOn(on) => {
            if !on {
                engine.organ_a.scanner_mode = ScannerMode::Off;
            } else if engine.organ_a.scanner_mode == ScannerMode::Off {
                engine.organ_a.scanner_mode = ScannerMode::C3;
            }
        }
        AudioCommand::SetPercussion {
            enabled,
            soft,
            fast,
            third,
        } => {
            engine.organ_a.percussion_enabled = enabled;
            engine.organ_a.percussion_soft = soft;
            engine.organ_a.percussion_fast_decay = fast;
            engine.organ_a.percussion_third_harmonic = third;
        }
        AudioCommand::SetRotarySpeed(sp) => {
            engine.organ_a.rotary_speed = sp;
            engine.organ_b.rotary_speed = sp;
        }
        AudioCommand::SetRotaryDrive(drv) => {
            engine.organ_a.rotary_drive = drv;
            engine.organ_b.rotary_drive = drv;
        }
        AudioCommand::SetRotaryEnabled(en) => {
            engine.organ_a.rotary_enabled = en;
            engine.organ_b.rotary_enabled = en;
        }
        AudioCommand::SetOrganLevel(lvl) => engine.layer_levels[0].base = lvl,
        AudioCommand::SetOrganBLevel(lvl) => engine.layer_levels[1].base = lvl,
        AudioCommand::SetPianoLevel(lvl) => engine.layer_levels[2].base = lvl,
        AudioCommand::SetPianoBLevel(lvl) => engine.layer_levels[3].base = lvl,
        AudioCommand::SetPianoCategory(cat) => {
            engine.piano_a.category = cat;
            engine.piano_b.category = cat;
        }
        AudioCommand::SetPianoTimbre(t) => {
            engine.piano_a.timbre = t;
            engine.piano_b.timbre = t;
        }
        AudioCommand::SetPianoTouch(tc) => {
            engine.piano_a.touch = tc;
            engine.piano_b.touch = tc;
        }
        AudioCommand::SetPianoAcoustics {
            string_res,
            pedal_noise,
        } => {
            engine.piano_a.string_resonance_enabled = string_res;
            engine.piano_a.pedal_noise_enabled = pedal_noise;
        }
        AudioCommand::SetSynthLevel(lvl) => engine.layer_levels[4].base = lvl,
        AudioCommand::SetSynthBLevel(lvl) => engine.layer_levels[5].base = lvl,
        AudioCommand::SetSynthCLevel(lvl) => engine.layer_levels[6].base = lvl,
        AudioCommand::SetSynthOscMode(m) => engine.synth_a.osc_mode = m,
        AudioCommand::SetSynthWaveform(w) => engine.synth_a.waveform = w,
        AudioCommand::SetSynthDetune(d) => engine.synth_a.supersaw_detune = d,
        AudioCommand::SetSynthFilterModel(fm) => engine.synth_a.filter_model = fm,
        AudioCommand::SetSynthCutoff(co) => engine.synth_a.cutoff_hz = co,
        AudioCommand::SetSynthResonance(res) => engine.synth_a.resonance = res,
        AudioCommand::SetSynthDrive(drv) => engine.synth_a.filter_drive = drv,
        AudioCommand::SetSynthAttack(atk) => engine.synth_a.attack_sec = atk,
        AudioCommand::SetSynthDecay(dec) => engine.synth_a.decay_sec = dec,
        AudioCommand::SetSynthRelease(rel) => engine.synth_a.release_sec = rel,
        AudioCommand::SetFxMod1 { kind, rate, amount } => {
            for fx in &mut engine.fx_chains {
                fx.mod1_type = kind;
                fx.mod1_rate_hz = rate;
                fx.mod1_amount = amount;
            }
        }
        AudioCommand::SetFxMod2 { kind, rate, amount } => {
            for fx in &mut engine.fx_chains {
                fx.mod2_type = kind;
                fx.mod2_rate_hz = rate;
                fx.mod2_amount = amount;
            }
        }
        AudioCommand::SetFxEq {
            drive,
            bass,
            mid,
            mid_freq,
            treble,
        } => {
            for fx in &mut engine.fx_chains {
                fx.drive_amount = drive;
                fx.eq_bass_db = bass;
                fx.eq_mid_db = mid;
                fx.eq_mid_freq_hz = mid_freq;
                fx.eq_treble_db = treble;
            }
        }
        AudioCommand::SetFxDelay {
            enabled,
            feedback,
            mix,
        } => {
            for fx in &mut engine.fx_chains {
                fx.delay_enabled = enabled;
                fx.delay_feedback = feedback;
                fx.delay_mix = mix;
            }
        }
        AudioCommand::SetFxReverb { model, mix } => {
            for fx in &mut engine.fx_chains {
                fx.reverb_model = model;
                fx.reverb_mix = mix;
            }
        }
        AudioCommand::SetMasterComp(amt) => {
            engine.master_bus.compressor_amount = amt;
        }
        AudioCommand::SetLayerActive(idx, on) => {
            if idx < 7 {
                let bit = 1u8 << idx;
                if on {
                    engine.scene.scene_1_mask |= bit;
                    engine.scene.scene_2_mask |= bit;
                } else {
                    engine.scene.scene_1_mask &= !bit;
                    engine.scene.scene_2_mask &= !bit;
                    // Release anything still sounding on that layer.
                    for n in 0..128u8 {
                        match idx {
                            0 => engine.organ_a.note_off(n),
                            1 => engine.organ_b.note_off(n),
                            2 => engine.piano_a.note_off(n),
                            3 => engine.piano_b.note_off(n),
                            4 => engine.synth_a.note_off(n),
                            5 => engine.synth_b.note_off(n),
                            _ => engine.synth_c.note_off(n),
                        }
                    }
                }
            }
        }
        AudioCommand::SetFirmwareOptions(opts) => opts.apply_exact(engine),
        AudioCommand::LoadProgram(prog) => {
            engine.apply_program(&prog);
        }
        AudioCommand::LoadPianoSound { slot_b, sound } => {
            if slot_b {
                engine.piano_b.set_sound(sound);
            } else {
                engine.piano_a.set_sound(sound);
            }
        }
        AudioCommand::LoadSynthSample { layer_idx, sound } => {
            let layer = match layer_idx {
                0 => &mut engine.synth_a,
                1 => &mut engine.synth_b,
                2 => &mut engine.synth_c,
                _ => return,
            };
            layer.load_sample(sound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::organ::OrganModel;
    use std::sync::Arc;

    #[test]
    fn test_apply_audio_command_updates_engine() {
        let mut engine = Stage4Engine::new(48_000.0);
        let mut master = 0.85;
        apply_audio_command(
            AudioCommand::SetOrganModel(OrganModel::Farfisa),
            &mut engine,
            &mut master,
        );
        assert_eq!(engine.organ_a.model, OrganModel::Farfisa);

        apply_audio_command(
            AudioCommand::SetSynthCutoff(1234.0),
            &mut engine,
            &mut master,
        );
        assert_eq!(engine.synth_a.cutoff_hz, 1234.0);
    }

    #[test]
    fn test_layer_active_toggles_scene_masks() {
        let mut engine = Stage4Engine::new(48_000.0);
        let mut master = 0.85;
        apply_audio_command(
            AudioCommand::SetLayerActive(1, true),
            &mut engine,
            &mut master,
        );
        assert_ne!(engine.scene.scene_1_mask & 0b10, 0);
        apply_audio_command(
            AudioCommand::SetLayerActive(1, false),
            &mut engine,
            &mut master,
        );
        assert_eq!(engine.scene.scene_1_mask & 0b10, 0);
    }

    #[test]
    fn test_apply_audio_command_load_program() {
        let mut engine = Stage4Engine::new(48000.0);
        let mut vol = 0.8;
        let mut prog = nord_core::cbin::ProgramDecoded::default();
        prog.organ.layer_a_level = 0.42;
        prog.organ.drawbars_a = [8, 8, 8, 0, 0, 0, 0, 0, 0];

        apply_audio_command(
            AudioCommand::LoadProgram(Box::new(prog)),
            &mut engine,
            &mut vol,
        );

        assert_eq!(engine.organ_a.drawbars, [8, 8, 8, 0, 0, 0, 0, 0, 0]);
        assert!((engine.layer_levels[0].base - 0.42).abs() < 1e-5);
    }

    #[test]
    fn test_apply_audio_command_load_piano_and_synth_samples() {
        let mut engine = Stage4Engine::new(48000.0);
        let mut vol = 0.8;

        let piano_sound = Arc::new(nord_core::piano::NpnoSound::new_synthetic(
            "Test Grand",
            60,
            &[0.1, 0.2, 0.1, 0.0],
            48000.0,
        ));
        apply_audio_command(
            AudioCommand::LoadPianoSound {
                slot_b: false,
                sound: Some(Arc::clone(&piano_sound)),
            },
            &mut engine,
            &mut vol,
        );
        engine.piano_a.note_on(60, 100);
        let [l, r] = engine.piano_a.render_frame();
        assert!(l.is_finite() && r.is_finite());

        let synth_sound = Arc::new(nord_core::synth::SynthSampleSound {
            name: "Test Sample".to_string(),
            zones: vec![],
        });
        apply_audio_command(
            AudioCommand::LoadSynthSample {
                layer_idx: 0,
                sound: Some(Arc::clone(&synth_sound)),
            },
            &mut engine,
            &mut vol,
        );
        assert_eq!(engine.synth_a.loaded_sample_name(), Some("Test Sample"));

        apply_audio_command(
            AudioCommand::LoadSynthSample {
                layer_idx: 0,
                sound: None,
            },
            &mut engine,
            &mut vol,
        );
        assert_eq!(engine.synth_a.loaded_sample_name(), None);
    }
}

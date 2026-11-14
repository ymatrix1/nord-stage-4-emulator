//! `Stage4Engine` sound loading and decoded Program application.

use super::*;

impl Stage4Engine {
    /// Loads an `.npno` multi-velocity fixed-LPC piano bank into Piano Layer A (`slot_b = false`) or B (`slot_b = true`).
    pub fn load_piano_npno_bytes(
        &mut self,
        slot_b: bool,
        bytes: &[u8],
    ) -> Result<std::sync::Arc<crate::piano::NpnoSound>, String> {
        let sound = std::sync::Arc::new(crate::piano::NpnoSound::from_npno_bytes(bytes)?);
        if slot_b {
            self.piano_b.set_sound(Some(std::sync::Arc::clone(&sound)));
        } else {
            self.piano_a.set_sound(Some(std::sync::Arc::clone(&sound)));
        }
        Ok(sound)
    }

    /// Loads an `.nsmp4` multisample instrument into Synth Layer A (`0`), B (`1`), or C (`2`).
    pub fn load_synth_nsmp4_bytes(
        &mut self,
        layer_idx: usize,
        bytes: &[u8],
    ) -> Result<std::sync::Arc<crate::synth::SynthSampleSound>, crate::cbin::CbinError> {
        let sound = std::sync::Arc::new(crate::synth::SynthSampleSound::from_nsmp4_bytes(bytes)?);
        let layer = match layer_idx {
            0 => &mut self.synth_a,
            1 => &mut self.synth_b,
            2 => &mut self.synth_c,
            _ => {
                return Err(crate::cbin::CbinError::BitstreamError(format!(
                    "invalid synth layer index {layer_idx} (expected 0..=2)"
                )));
            }
        };
        layer.load_sample(Some(std::sync::Arc::clone(&sound)));
        Ok(sound)
    }

    /// Reads an `.ns4p` program entry from an `.ns4b` backup archive, applies its parameters,
    /// and auto-resolves/loads matching active `.npno` & `.nsmp4` banks via `ARM0` MurmurHash2 hashes.
    pub fn load_program_from_backup<R: std::io::Read + std::io::Seek>(
        &mut self,
        archive: &crate::cbin::Ns4BackupArchive,
        reader: &mut R,
        entry_query: &str,
    ) -> Result<crate::cbin::ProgramDecoded, crate::cbin::CbinError> {
        let (prog, _refs) =
            self.load_program_with_sounds_from_backup(archive, reader, entry_query)?;
        Ok(prog)
    }

    /// Reads an `.ns4p` program entry and auto-loads resolved Piano/Synth sample entries from `archive`.
    pub fn load_program_with_sounds_from_backup<R: std::io::Read + std::io::Seek>(
        &mut self,
        archive: &crate::cbin::Ns4BackupArchive,
        reader: &mut R,
        entry_query: &str,
    ) -> Result<(crate::cbin::ProgramDecoded, crate::cbin::ProgramSoundRefs), crate::cbin::CbinError>
    {
        let bytes = archive.read_entry(reader, entry_query)?;
        self.load_program_bytes_with_sounds_from_backup(archive, reader, &bytes)
    }

    /// Like [`Self::load_program_with_sounds_from_backup`] but takes the `.ns4p` bytes directly
    /// (e.g. an edited copy of a factory program) while still resolving sounds from the backup.
    pub fn load_program_bytes_with_sounds_from_backup<R: std::io::Read + std::io::Seek>(
        &mut self,
        archive: &crate::cbin::Ns4BackupArchive,
        reader: &mut R,
        bytes: &[u8],
    ) -> Result<(crate::cbin::ProgramDecoded, crate::cbin::ProgramSoundRefs), crate::cbin::CbinError>
    {
        let cbin = crate::cbin::CbinFile::parse(bytes)?;
        let prog = cbin.decode_program()?;
        self.apply_program(&prog);

        let refs = archive.resolve_program_sounds(&prog);
        let active_mask = prog.global.scene_1_mask | prog.global.scene_2_mask;
        let piano_slots = [
            (false, prog.piano.layer_a.enabled, &refs.piano_a, 2u8),
            (true, prog.piano.layer_b.enabled, &refs.piano_b, 3u8),
        ];
        for (slot_b, enabled, entry_opt, bit) in piano_slots {
            let needed = enabled || active_mask == 0 || (active_mask & (1 << bit)) != 0;
            if needed {
                if let Some(entry_name) = entry_opt {
                    if let Ok(npno_bytes) = archive.read_entry(reader, entry_name) {
                        let _ = self.load_piano_npno_bytes(slot_b, &npno_bytes);
                    }
                }
            }
        }
        let synth_slots = [
            (0usize, prog.synth.layer_a.enabled, &refs.synth_a, 4u8),
            (1usize, prog.synth.layer_b.enabled, &refs.synth_b, 5u8),
            (2usize, prog.synth.layer_c.enabled, &refs.synth_c, 6u8),
        ];
        for (layer_idx, enabled, entry_opt, bit) in synth_slots {
            let needed = enabled || active_mask == 0 || (active_mask & (1 << bit)) != 0;
            if needed {
                if let Some(entry_name) = entry_opt {
                    if let Ok(nsmp_bytes) = archive.read_entry(reader, entry_name) {
                        let _ = self.load_synth_nsmp4_bytes(layer_idx, &nsmp_bytes);
                    }
                }
            }
        }

        Ok((prog, refs))
    }

    /// Applies a decoded Nord Stage 4 Program to the DSP engine.
    /// Applies all decoded CBIN program settings (layers, scenes, splits, morphs, and 7 FX chains).
    pub fn apply_program(&mut self, prog: &crate::cbin::ProgramDecoded) {
        // Organ (Layers A & B + shared Organ FX)
        self.organ_a.model = prog.organ.model;
        self.organ_b.model = prog.organ.model_b;
        self.organ_a.drawbars = prog.organ.drawbars_a;
        self.organ_b.drawbars = prog.organ.drawbars_b;
        self.organ_a.octave_shift = prog.organ.layer_a_octave.clamp(-2, 2);
        self.organ_b.octave_shift = prog.organ.layer_b_octave.clamp(-2, 2);
        for (organ, vib_on) in [
            (&mut self.organ_a, prog.organ.vibrato_on),
            (&mut self.organ_b, prog.organ.vibrato_on_b),
        ] {
            organ.percussion_enabled = prog.organ.percussion_on;
            organ.percussion_soft = prog.organ.percussion_soft;
            organ.percussion_fast_decay = prog.organ.percussion_fast;
            organ.percussion_third_harmonic = prog.organ.percussion_third;
            organ.scanner_mode = prog.organ.scanner_mode(vib_on);
            organ.rotary_enabled = prog.organ.rotary_enabled;
            organ.rotary_speed = prog.organ.rotary_speed;
            // preset unit 0..10 (`(v / 127) * 10`) to the engine unit 0..1 = the firmware `v / 127`
            // (VERIFIED, docs/research/fx-type-map.md section 9); `rotary.rs` takes the fraction.
            organ.rotary_drive = (prog.organ.rotary_drive / 10.0).clamp(0.0, 1.0);
        }

        // Piano (Layers A & B)
        for (piano, dec) in [
            (&mut self.piano_a, &prog.piano.layer_a),
            (&mut self.piano_b, &prog.piano.layer_b),
        ] {
            piano.category = dec.category;
            piano.timbre = dec.timbre;
            piano.touch = dec.touch;
            piano.string_resonance_enabled = dec.string_res;
            piano.pedal_noise_enabled = dec.pedal_noise;
            piano.soft_release = dec.soft_release;
            piano.unison = dec.unison.min(3);
            piano.dyn_comp = dec.dyn_comp.min(3);
            piano.octave_shift = dec.octave.clamp(-2, 2);
        }

        // Synth (Layers A, B & C + cutoff morph matrix)
        crate::synth_params::apply_decoded_layer(
            &mut self.synth_a,
            &mut self.synth_cutoff_morph[0],
            &prog.synth.layer_a,
        );
        crate::synth_params::apply_decoded_layer(
            &mut self.synth_b,
            &mut self.synth_cutoff_morph[1],
            &prog.synth.layer_b,
        );
        crate::synth_params::apply_decoded_layer(
            &mut self.synth_c,
            &mut self.synth_cutoff_morph[2],
            &prog.synth.layer_c,
        );

        // Per-layer 0x36 FX chains
        Self::apply_fx_chain(&mut self.fx_chains[0], &prog.organ.fx);
        Self::apply_fx_chain(&mut self.fx_chains[1], &prog.organ.fx);
        Self::apply_fx_chain(&mut self.fx_chains[2], &prog.piano.fx_a);
        Self::apply_fx_chain(&mut self.fx_chains[3], &prog.piano.fx_b);
        Self::apply_fx_chain(&mut self.fx_chains[4], &prog.synth.fx_a);
        Self::apply_fx_chain(&mut self.fx_chains[5], &prog.synth.fx_b);
        Self::apply_fx_chain(&mut self.fx_chains[6], &prog.synth.fx_c);

        // Global
        self.master_clock.bpm = prog.global.tempo_bpm;
        self.master_bus.compressor_amount = (prog.global.master_compressor / 10.0).clamp(0.0, 1.0);
        for fx in &mut self.fx_chains {
            fx.snap_delay_time(self.master_clock.bpm);
        }

        // Scene / section masks (with fallback when synthetic tests leave scene_1_mask == 0)
        let category_mask = match prog.category_id {
            0x07 | 0x12 => 1 << 0,
            0x09 | 0x16 | 0x17 | 0x18 | 0x19 => 1 << 2,
            0x1E => (1 << 2) | (1 << 4),
            0x2A..=0x2F | 0x06 | 0x0B | 0x0D | 0x34 => 1 << 4,
            _ => (1 << 0) | (1 << 2) | (1 << 4),
        };
        let default_layer_mask = (prog.organ.layer_a_enabled as u8)
            | ((prog.organ.layer_b_enabled as u8) << 1)
            | ((prog.piano.layer_a.enabled as u8) << 2)
            | ((prog.piano.layer_b.enabled as u8) << 3)
            | ((prog.synth.layer_a.enabled as u8) << 4)
            | ((prog.synth.layer_b.enabled as u8) << 5)
            | ((prog.synth.layer_c.enabled as u8) << 6);

        self.scene.scene_1_mask = if prog.global.scene_1_mask != 0 {
            prog.global.scene_1_mask
        } else if default_layer_mask != 0 && default_layer_mask != (1 << 2) {
            default_layer_mask
        } else {
            category_mask
        };
        self.scene.scene_2_mask = if prog.global.scene_2_mask != 0 {
            prog.global.scene_2_mask
        } else {
            self.scene.scene_1_mask
        };

        let any_zone_flag = prog.global.split_zone_enabled.iter().any(|&e| e);
        for (i, &pt) in prog.global.split_points.iter().enumerate() {
            self.split_router.splits[i].midi_note = pt;
            self.split_router.splits[i].crossfade = prog.global.split_crossfades[i];
            self.split_router.splits[i].enabled =
                prog.global.split_enabled && (!any_zone_flag || prog.global.split_zone_enabled[i]);
        }

        // Layer levels + morph deltas
        let level_specs = [
            (prog.organ.layer_a_level, prog.organ.layer_a_level_morph),
            (prog.organ.layer_b_level, prog.organ.layer_b_level_morph),
            (prog.piano.layer_a.level, prog.piano.layer_a.level_morph),
            (prog.piano.layer_b.level, prog.piano.layer_b.level_morph),
            (prog.synth.layer_a.level, prog.synth.layer_a.level_morph),
            (prog.synth.layer_b.level, prog.synth.layer_b.level_morph),
            (prog.synth.layer_c.level, prog.synth.layer_c.level_morph),
        ];
        for (slot, (lvl, morph)) in self.layer_levels.iter_mut().zip(level_specs) {
            slot.base = lvl.clamp(0.0, 1.0);
            slot.delta_wheel = morph.wheel;
            slot.delta_aftertouch = morph.aftertouch;
            slot.delta_pedal = morph.ctrl_pedal;
        }

        self.record_program_routing(prog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_garbage_piano_bank_is_rejected() {
        let mut e = Stage4Engine::new(48_000.0);
        assert!(e.load_piano_npno_bytes(false, b"not an npno").is_err());
    }

    #[test]
    fn rotary_drive_enters_the_engine_as_the_firmware_fraction() {
        // the preset stores (v / 127) * 10; the firmware drive is v / 127 and `rotary.rs` takes it
        let mut prog = crate::cbin::ProgramDecoded::default();
        prog.organ.rotary_drive = 2.44;
        let mut e = Stage4Engine::new(48_000.0);
        e.apply_program(&prog);
        assert!((e.organ_a.rotary_drive - 0.244).abs() < 1e-6);
        assert!((e.organ_b.rotary_drive - 0.244).abs() < 1e-6);
        prog.organ.rotary_drive = 12.0;
        e.apply_program(&prog);
        assert_eq!(e.organ_a.rotary_drive, 1.0, "clamped to the engine range");
        prog.organ.rotary_drive = 7.5;
        e.apply_program(&prog);
        let out = e.export_program(&crate::cbin::ProgramDecoded::default());
        assert!(
            (out.organ.rotary_drive - 7.5).abs() < 1e-5,
            "{}",
            out.organ.rotary_drive
        );
    }
}

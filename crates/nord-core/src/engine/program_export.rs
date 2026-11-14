//! Engine -> `.ns4p` / `.ns4l` export (Live slot and program store persistence).
//!
//! Spec: `docs/specs/cbin-and-presets.md` "Engine export". The export is a patch onto a base
//! [`ProgramDecoded`]: a field is written only when the engine value differs from what
//! `apply_program` would have produced from the base value. Unedited fields (and every field the
//! engine does not map: deep Synth parameters, FX chains, scene enables, bytes the parser keeps
//! raw) therefore stay bit-identical, and `apply_program(base)` then `export_program(base)`
//! returns `base`. Control thread only: it allocates.

use super::*;
use crate::cbin::{CbinError, CbinFile, CbinKind, MorphDeltas, ProgramDecoded};

/// Overwrites `field` with the engine value unless `project(field)` (what applying `field`
/// yields) already equals it.
fn keep<T: PartialEq + Copy>(field: &mut T, project: impl Fn(T) -> T, engine: T) {
    if project(*field) != engine {
        *field = engine;
    }
}

fn deltas(p: &MorphParameter) -> MorphDeltas {
    MorphDeltas {
        wheel: p.delta_wheel,
        aftertouch: p.delta_aftertouch,
        ctrl_pedal: p.delta_pedal,
    }
}

/// Level `0..1` and its morph deltas.
fn level(dec: &mut f32, morph: &mut MorphDeltas, engine: &MorphParameter) {
    keep(dec, |v| v.clamp(0.0, 1.0), engine.base);
    keep_deltas(morph, deltas(engine));
}

fn keep_deltas(morph: &mut MorphDeltas, engine: MorphDeltas) {
    if *morph != engine {
        *morph = engine;
    }
}

impl ProgramState {
    /// Patches `prog` with the panel state held in this snapshot (see the module docs).
    pub fn export_into(&self, prog: &mut ProgramDecoded) {
        self.export_global(prog);
        self.export_organ(prog);
        self.export_piano(prog);
        self.export_synth(prog);
    }

    fn export_global(&self, prog: &mut ProgramDecoded) {
        let g = &mut prog.global;
        keep(&mut g.tempo_bpm, |v| v, self.tempo_bpm);
        let any_zone = g.split_zone_enabled.iter().any(|&e| e);
        let mut changed = false;
        for (i, s) in self.split_router.splits.iter().enumerate() {
            keep(&mut g.split_points[i], |v| v, s.midi_note);
            keep(&mut g.split_crossfades[i], |v| v, s.crossfade);
            changed |= (g.split_enabled && (!any_zone || g.split_zone_enabled[i])) != s.enabled;
        }
        if changed {
            for (zone, s) in g
                .split_zone_enabled
                .iter_mut()
                .zip(&self.split_router.splits)
            {
                *zone = s.enabled;
            }
            g.split_enabled = self.split_router.splits.iter().any(|s| s.enabled);
        }
    }

    fn export_organ(&self, prog: &mut ProgramDecoded) {
        let o = &mut prog.organ;
        let (a, b) = (&self.organ_a, &self.organ_b);
        o.model = a.model;
        o.model_b = b.model;
        o.drawbars_a = a.drawbars;
        o.drawbars_b = b.drawbars;
        keep(&mut o.layer_a_octave, |v| v.clamp(-2, 2), a.octave_shift);
        keep(&mut o.layer_b_octave, |v| v.clamp(-2, 2), b.octave_shift);
        o.percussion_on = a.percussion_enabled;
        o.percussion_soft = a.percussion_soft;
        o.percussion_fast = a.percussion_fast_decay;
        o.percussion_third = a.percussion_third_harmonic;
        o.rotary_enabled = a.rotary_enabled;
        o.rotary_speed = a.rotary_speed;
        o.rotary_drive = a.rotary_drive * 10.0;
        level(
            &mut o.layer_a_level,
            &mut o.layer_a_level_morph,
            &self.layer_levels[0],
        );
        level(
            &mut o.layer_b_level,
            &mut o.layer_b_level_morph,
            &self.layer_levels[1],
        );
    }

    fn export_piano(&self, prog: &mut ProgramDecoded) {
        let p = &mut prog.piano;
        let layers = [
            (&mut p.layer_a, &self.piano_a, 2),
            (&mut p.layer_b, &self.piano_b, 3),
        ];
        for (dec, piano, idx) in layers {
            dec.category = piano.category;
            dec.timbre = piano.timbre;
            dec.touch = piano.touch;
            dec.string_res = piano.string_resonance_enabled;
            dec.pedal_noise = piano.pedal_noise_enabled;
            dec.soft_release = piano.soft_release;
            keep(&mut dec.unison, |v| v.min(3), piano.unison);
            keep(&mut dec.dyn_comp, |v| v.min(3), piano.dyn_comp);
            keep(&mut dec.octave, |v| v.clamp(-2, 2), piano.octave_shift);
            level(
                &mut dec.level,
                &mut dec.level_morph,
                &self.layer_levels[idx],
            );
        }
    }

    fn export_synth(&self, prog: &mut ProgramDecoded) {
        let s = &mut prog.synth;
        let layers = [
            (&mut s.layer_a, &self.synth_a, 4),
            (&mut s.layer_b, &self.synth_b, 5),
            (&mut s.layer_c, &self.synth_c, 6),
        ];
        for (dec, synth, idx) in layers {
            keep(&mut dec.octave, |v| v.clamp(-3, 3), synth.octave_offset);
            level(
                &mut dec.level,
                &mut dec.level_morph,
                &self.layer_levels[idx],
            );
        }
    }
}

impl Stage4Engine {
    /// The current program as a [`ProgramDecoded`]: `base` patched with the engine state.
    /// `base` supplies everything the engine does not map (name, category, FX, deep Synth data).
    pub fn export_program(&self, base: &ProgramDecoded) -> ProgramDecoded {
        let mut prog = base.clone();
        self.capture_state().export_into(&mut prog);
        prog
    }
}

impl LiveSlots {
    /// Slot `slot` as a [`ProgramDecoded`] patched onto `base`; `None` when the slot is empty.
    pub fn export_slot(&self, slot: usize, base: &ProgramDecoded) -> Option<ProgramDecoded> {
        let mut prog = base.clone();
        self.get(slot)?.export_into(&mut prog);
        Some(prog)
    }

    /// Stores a decoded program into `slot` (loads it into a scratch engine at `sample_rate`).
    pub fn import_program(&mut self, slot: usize, prog: &ProgramDecoded, sample_rate: f32) {
        let mut scratch = Stage4Engine::new(sample_rate);
        scratch.apply_program(prog);
        self.set(slot, scratch.capture_state());
    }

    /// Loads a Live slot file (`.ns4l`) back into the slot its header names (`location_index`,
    /// VERIFIED `0..7` in the Factory Restore archive). Returns the slot.
    pub fn import_live_file(
        &mut self,
        file: &CbinFile,
        sample_rate: f32,
    ) -> Result<usize, CbinError> {
        if file.header.kind != CbinKind::Live {
            return Err(CbinError::UnknownTag(file.header.kind.tag()));
        }
        let slot = usize::from(file.header.location_index);
        if slot >= LIVE_SLOT_COUNT {
            return Err(CbinError::BitstreamError(format!(
                "Live slot {slot} out of range (0..{LIVE_SLOT_COUNT})"
            )));
        }
        let prog = ProgramDecoded::from_cbin_file(file)?;
        self.import_program(slot, &prog, sample_rate);
        Ok(slot)
    }

    /// [`Self::import_live_file`] from the bytes of an `.ns4l` file.
    pub fn import_live_bytes(
        &mut self,
        bytes: &[u8],
        sample_rate: f32,
    ) -> Result<usize, CbinError> {
        self.import_live_file(&CbinFile::parse(bytes)?, sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::CbinFile;
    use crate::organ::OrganModel;

    fn base() -> ProgramDecoded {
        let mut p = ProgramDecoded::default();
        p.global.tempo_bpm = 97.0;
        p.organ.drawbars_a = [8, 7, 6, 5, 4, 3, 2, 1, 0];
        p.piano.layer_a.unison = 2;
        p.synth.layer_a.level = 0.6;
        p
    }

    #[test]
    fn unedited_export_equals_the_base_and_is_byte_identical() {
        let template = base().to_cbin_file(2, 9).unwrap();
        let bytes = template.to_bytes();
        let parsed = CbinFile::parse(&bytes).unwrap();
        let prog = ProgramDecoded::from_cbin_file(&parsed).unwrap();
        let mut e = Stage4Engine::new(48_000.0);
        e.apply_program(&prog);
        let out = e.export_program(&prog);
        assert_eq!(out, prog);
        assert_eq!(out.to_cbin_file_like(&parsed).unwrap().to_bytes(), bytes);
    }

    #[test]
    fn edits_survive_encode_parse_apply() {
        let template = base().to_cbin_file(0, 1).unwrap();
        let prog = ProgramDecoded::from_cbin_file(&CbinFile::parse(&template.to_bytes()).unwrap())
            .unwrap();
        let mut e = Stage4Engine::new(48_000.0);
        e.apply_program(&prog);
        e.master_clock.bpm = 140.0;
        e.organ_a.drawbars[0] = 2;
        e.organ_b.model = OrganModel::Vox;
        e.piano_b.unison = 3;
        e.layer_levels[4].base = 0.25;
        e.layer_levels[0].delta_wheel = 0.5;
        e.split_router.splits[1].midi_note = 60;
        e.split_router.splits[1].enabled = true;
        let out = e.export_program(&prog);
        let file = out.to_cbin_file_like(&template).unwrap();
        let back =
            ProgramDecoded::from_cbin_file(&CbinFile::parse(&file.to_bytes()).unwrap()).unwrap();
        assert_eq!(back.global.tempo_bpm, 140.0);
        assert_eq!(back.organ.drawbars_a[0], 2);
        assert_eq!(back.organ.model_b, OrganModel::Vox);
        assert_eq!(back.piano.layer_b.unison, 3);
        assert!((back.synth.layer_a.level - 0.25).abs() < 0.02);
        assert!((back.organ.layer_a_level_morph.wheel - 0.5).abs() < 0.02);
        assert_eq!(back.global.split_points[1], 60);
        assert!(back.global.split_enabled && back.global.split_zone_enabled[1]);
        // Untouched fields stay as they were.
        assert_eq!(back.organ.drawbars_a[1], 7);
        assert_eq!(back.piano.layer_a.unison, 2);
        // And a fresh engine loading the stored file reproduces the edits.
        let mut again = Stage4Engine::new(48_000.0);
        again.apply_program(&back);
        assert_eq!(again.master_clock.bpm, 140.0);
        assert_eq!(again.organ_a.drawbars[0], 2);
    }

    #[test]
    fn live_slots_export_and_import_round_trip() {
        let prog = base();
        let mut live = LiveSlots::default();
        assert!(live.export_slot(3, &prog).is_none());
        live.import_program(3, &prog, 48_000.0);
        let out = live.export_slot(3, &prog).unwrap();
        assert_eq!(out, prog);
        let file = out.to_live_cbin_file(3).unwrap();
        assert_eq!(file.header.location_index, 3);
    }

    #[test]
    fn import_live_files_fill_the_slot_named_by_the_header() {
        let mut prog = base();
        prog.global.tempo_bpm = 133.0_f32;
        let bytes = prog.to_live_cbin_file(5).unwrap().to_bytes();
        let mut live = LiveSlots::default();
        assert_eq!(live.import_live_bytes(&bytes, 48_000.0_f32).unwrap(), 5);
        assert!(live.get(4).is_none());
        let back = live.export_slot(5, &prog).unwrap();
        assert_eq!(back.global.tempo_bpm, 133.0_f32);
        // The imported slot survives export -> import again, bit for bit.
        assert_eq!(back.to_live_cbin_file(5).unwrap().to_bytes(), bytes);
        // A program file, an out-of-range slot and garbage are refused.
        let prog_file = prog.to_cbin_file(0, 0).unwrap().to_bytes();
        assert!(live.import_live_bytes(&prog_file, 48_000.0_f32).is_err());
        assert!(live
            .import_live_bytes(&prog.to_live_cbin_file(8).unwrap().to_bytes(), 48_000.0_f32)
            .is_err());
        assert!(live.import_live_bytes(b"nope", 48_000.0_f32).is_err());
    }
}

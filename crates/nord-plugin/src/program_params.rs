//! Program-area performance parameters (Phase 6b B9d, `ns4-ymci`; manual p.40, p.46).
//!
//! IDs are additive: `prog_transpose` (-6..6 semitones) and `prog_level_db` (Program Levels
//! offset). Defaults equal the engine defaults, so an untouched host leaves sounds unchanged.

use nih_plug::prelude::*;
use nord_core::engine::{Stage4Engine, PROGRAM_LEVEL_DB_RANGE, TRANSPOSE_RANGE};

/// Per-program Transpose and Program Levels.
#[derive(Params)]
pub struct ProgramParams {
    /// Transpose of all layers in semitones (manual p.40: +/-6).
    #[id = "transpose"]
    pub transpose: IntParam,
    /// Program Levels volume offset in dB for all layers (manual p.46; range `ASSUMED`).
    #[id = "level_db"]
    pub level_db: FloatParam,
}

impl Default for ProgramParams {
    fn default() -> Self {
        let t = i32::from(TRANSPOSE_RANGE);
        Self {
            transpose: IntParam::new("Transpose", 0, IntRange::Linear { min: -t, max: t })
                .with_unit(" st"),
            level_db: FloatParam::new(
                "Program Level",
                0.0,
                FloatRange::Linear {
                    min: -PROGRAM_LEVEL_DB_RANGE,
                    max: PROGRAM_LEVEL_DB_RANGE,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1),
        }
    }
}

impl ProgramParams {
    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        engine.set_transpose(self.transpose.value() as i8);
        engine.set_program_level_db(self.level_db.value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::midi::MidiEvent;

    fn render(p: &ProgramParams) -> Vec<[f32; 2]> {
        let mut e = Stage4Engine::new(44100.0);
        p.sync_to_engine(&mut e);
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        });
        (0..4096).map(|_| e.render_frame()).collect()
    }

    #[test]
    fn defaults_match_engine_defaults() {
        let reference = Stage4Engine::new(44100.0);
        let mut e = Stage4Engine::new(44100.0);
        ProgramParams::default().sync_to_engine(&mut e);
        assert_eq!(e.transpose(), reference.transpose());
        assert_eq!(e.program_level_db(), reference.program_level_db());
    }

    #[test]
    fn values_reach_the_engine_and_change_the_render() {
        let base = render(&ProgramParams::default());
        let p = ProgramParams {
            transpose: IntParam::new("t", 5, IntRange::Linear { min: -6, max: 6 }),
            level_db: FloatParam::new(
                "l",
                -6.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            ),
        };
        let mut e = Stage4Engine::new(44100.0);
        p.sync_to_engine(&mut e);
        assert_eq!((e.transpose(), e.program_level_db()), (5, -6.0));
        assert!(
            base != render(&p),
            "transpose/level leave the render unchanged"
        );
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = ProgramParams::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(ids, vec!["transpose".to_string(), "level_db".to_string()]);
    }
}

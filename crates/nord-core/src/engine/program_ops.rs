//! Program-area performance functions: per-program Transpose, Panic and Program Levels
//! (manual OS v1.6x p.40, p.46).
//!
//! All setters are idempotent and allocation-free, so a host may call them every block.

use super::*;

/// Transpose range in semitones (manual p.40: +/-6).
pub const TRANSPOSE_RANGE: i8 = 6;
/// Program Levels range in dB. `ASSUMED`: the manual (p.46) shows `-3.5 dB` and gives no limits;
/// +/-12 dB covers any sensible trim. Confirm on hardware.
pub const PROGRAM_LEVEL_DB_RANGE: f32 = 12.0;

impl Stage4Engine {
    /// Sets the per-program transpose (clamped to +/-6 semitones). Affects every layer, including
    /// layers on a direct MIDI channel; held notes release at the pitch they started with.
    pub fn set_transpose(&mut self, semitones: i8) {
        self.transpose = semitones.clamp(-TRANSPOSE_RANGE, TRANSPOSE_RANGE);
    }

    /// Current transpose in semitones.
    pub fn transpose(&self) -> i8 {
        self.transpose
    }

    /// Sets the Program Levels volume offset in dB applied to all layers (0.0 = unchanged).
    pub fn set_program_level_db(&mut self, db: f32) {
        let db = if db.is_finite() { db } else { 0.0 };
        let db = db.clamp(-PROGRAM_LEVEL_DB_RANGE, PROGRAM_LEVEL_DB_RANGE);
        if db != self.program_level_db {
            self.program_level_db = db;
            self.program_gain = if db == 0.0 {
                1.0
            } else {
                10f32.powf(db / 20.0)
            };
        }
    }

    /// Current Program Levels offset in dB.
    pub fn program_level_db(&self) -> f32 {
        self.program_level_db
    }

    /// Note after transpose, or `None` when it leaves the MIDI range.
    pub(super) fn transposed_note(&self, note: u8) -> Option<u8> {
        let n = i16::from(note) + i16::from(self.effective_transpose());
        (0..=127).contains(&n).then_some(n as u8)
    }

    /// Note a held `(channel, note)` key sounds at (the transpose at its note-on).
    pub(super) fn sounding_note(&self, channel: usize, note: u8) -> u8 {
        let n = i16::from(note) + i16::from(self.held_transpose[channel][note as usize]);
        n.clamp(0, 127) as u8
    }

    /// PANIC (manual p.40): releases every held note on every layer, lifts the Piano pedals and
    /// resets the Morph controllers. Release tails still decay naturally.
    pub fn panic(&mut self) {
        for channel in 0..16 {
            for note in 0..128u8 {
                let sounding = self.sounding_note(channel, note);
                for layer in LayerId::ALL {
                    let count = &mut self.note_hold_counts[channel][note as usize][layer as usize];
                    if *count > 0 {
                        *count = 0;
                        self.layer_note_off(layer, sounding);
                    }
                }
                self.held_transpose[channel][note as usize] = 0;
            }
        }
        for piano in [&mut self.piano_a, &mut self.piano_b] {
            piano.set_damper_pedal(false, 0);
            piano.set_sostenuto_pedal(false, 0);
            piano.set_soft_pedal(false, 0);
        }
        self.morph_controllers = MorphControllers::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_on(e: &mut Stage4Engine, note: u8) {
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        });
    }

    fn note_off(e: &mut Stage4Engine, note: u8) {
        e.handle_midi(MidiEvent::NoteOff {
            channel: 0,
            note,
            velocity: 0,
        });
    }

    fn render(e: &mut Stage4Engine, n: usize) -> Vec<[f32; 2]> {
        (0..n).map(|_| e.render_frame()).collect()
    }

    #[test]
    fn transpose_clamps_and_defaults_to_zero() {
        let mut e = Stage4Engine::new(48_000.0);
        assert_eq!(e.transpose(), 0);
        e.set_transpose(9);
        assert_eq!(e.transpose(), 6);
        e.set_transpose(-9);
        assert_eq!(e.transpose(), -6);
    }

    #[test]
    fn transposed_note_matches_pre_shifted_note_and_off_matches_on() {
        let mut a = Stage4Engine::new(48_000.0);
        a.set_transpose(5);
        note_on(&mut a, 60);
        let mut b = Stage4Engine::new(48_000.0);
        note_on(&mut b, 65);
        assert_eq!(render(&mut a, 2048), render(&mut b, 2048));
        // Changing the transpose while held must not strand the note.
        a.set_transpose(-3);
        note_off(&mut a, 60);
        note_off(&mut b, 65);
        assert_eq!(render(&mut a, 4096), render(&mut b, 4096));
    }

    #[test]
    fn transpose_out_of_range_drops_note() {
        let mut e = Stage4Engine::new(48_000.0);
        e.set_transpose(6);
        note_on(&mut e, 126);
        assert_eq!(e.note_recipients_for_test(0, 126), 0);
    }

    #[test]
    fn program_level_scales_output_by_db() {
        let mut a = Stage4Engine::new(48_000.0);
        let mut b = Stage4Engine::new(48_000.0);
        b.set_program_level_db(-6.0);
        note_on(&mut a, 60);
        note_on(&mut b, 60);
        let (ra, rb) = (render(&mut a, 4096), render(&mut b, 4096));
        let ea: f32 = ra.iter().map(|f| f[0] * f[0]).sum();
        let eb: f32 = rb.iter().map(|f| f[0] * f[0]).sum();
        assert!(ea > 1e-6);
        let ratio = (eb / ea).sqrt();
        assert!(
            (ratio - 10f32.powf(-6.0 / 20.0)).abs() < 0.05,
            "ratio {ratio}"
        );
        b.set_program_level_db(f32::NAN);
        assert_eq!(b.program_level_db(), 0.0);
    }

    #[test]
    fn panic_releases_held_notes_and_resets_controllers() {
        let mut e = Stage4Engine::new(48_000.0);
        e.morph_controllers.mod_wheel = 0.7;
        note_on(&mut e, 60);
        note_on(&mut e, 64);
        assert_ne!(e.note_recipients_for_test(0, 60), 0);
        e.panic();
        assert_eq!(e.note_recipients_for_test(0, 60), 0);
        assert_eq!(e.note_recipients_for_test(0, 64), 0);
        assert_eq!(e.morph_controllers, MorphControllers::default());
    }
}

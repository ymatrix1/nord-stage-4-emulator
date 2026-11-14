//! Keyboard Split screen state (manual p.15, p.39): Note and xFade rows, Low/Mid/High points.

use crate::audio::SplitCfg;
use nord_core::morph::CrossfadeWidth;

/// The 11 selectable split positions `C2..C7` in MIDI notes (Nord naming: C4 = 60).
///
/// ASSUMED: the manual gives "11 possible keyboard split positions, ranging from C2 to C7"
/// (p.39); 11 positions over 60 semitones means a step of 6 (C, F#, C, ...). The engine defaults
/// 48 / 60 / 72 are all members.
pub const POSITIONS: [u8; 11] = [36, 42, 48, 54, 60, 66, 72, 78, 84, 90, 96];

/// Focused row of the Keyboard Split screen (soft button 1 toggles it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitRow {
    #[default]
    Note,
    Xfade,
}

/// Note name in the display, Nord octave numbering (60 = C4).
pub fn note_name(note: u8) -> String {
    const N: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", N[usize::from(note % 12)], i32::from(note / 12) - 1)
}

/// Label of a crossfade width as the engine defines it (`half_semitones`).
pub fn xfade_label(x: CrossfadeWidth) -> &'static str {
    match x {
        CrossfadeWidth::Off => "-",
        CrossfadeWidth::Small => "3",
        CrossfadeWidth::Large => "6",
    }
}

/// UI state of the split function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitUi {
    /// SPLIT ON/SET state.
    pub on: bool,
    pub enabled: [bool; 3],
    pub notes: [u8; 3],
    pub xfade: [CrossfadeWidth; 3],
    pub row: SplitRow,
    /// Selected point `0..3` (Low, Mid, High).
    pub point: usize,
    /// SET KEY pressed: the next key played or clicked sets the selected split point.
    pub set_key_armed: bool,
}

impl Default for SplitUi {
    fn default() -> Self {
        Self {
            on: false,
            // ASSUMED: a fresh program starts with one active split point (Mid, C4).
            enabled: [false, true, false],
            notes: [48, 60, 72],
            xfade: [
                CrossfadeWidth::Off,
                CrossfadeWidth::Off,
                CrossfadeWidth::Off,
            ],
            row: SplitRow::Note,
            point: 1,
            set_key_armed: false,
        }
    }
}

impl SplitUi {
    /// Edit to send to the engine.
    pub fn cfg(&self) -> SplitCfg {
        SplitCfg {
            on: self.on,
            enabled: self.enabled,
            notes: self.notes,
            xfade: self.xfade,
        }
    }

    /// Which of the three split LEDs are lit (split on and the point active).
    pub fn leds(&self) -> [bool; 3] {
        std::array::from_fn(|i| self.on && self.enabled[i])
    }

    /// Soft button `n` (1..=4) of the Keyboard Split screen.
    pub fn soft(&mut self, n: u8) {
        match n {
            1 => {
                self.row = match self.row {
                    SplitRow::Note => SplitRow::Xfade,
                    SplitRow::Xfade => SplitRow::Note,
                }
            }
            2..=4 => {
                let p = usize::from(n - 2);
                if self.row == SplitRow::Note && self.enabled[p] && self.point == p {
                    // ASSUMED: pressing the selected point again deactivates it (p.39).
                    self.enabled[p] = false;
                } else {
                    self.enabled[p] = true;
                    self.point = p;
                }
            }
            _ => {}
        }
    }

    /// Dial turned by `delta` on the focused row for the selected point.
    pub fn dial(&mut self, delta: i32) {
        let p = self.point.min(2);
        match self.row {
            SplitRow::Note => {
                let cur = POSITIONS
                    .iter()
                    .position(|&n| n >= self.notes[p])
                    .unwrap_or(POSITIONS.len() - 1) as i32;
                let i = (cur + delta).clamp(0, POSITIONS.len() as i32 - 1) as usize;
                self.notes[p] = POSITIONS[i];
            }
            SplitRow::Xfade => {
                const ALL: [CrossfadeWidth; 3] = [
                    CrossfadeWidth::Off,
                    CrossfadeWidth::Small,
                    CrossfadeWidth::Large,
                ];
                let cur = ALL.iter().position(|&x| x == self.xfade[p]).unwrap_or(0) as i32;
                self.xfade[p] = ALL[(cur + delta).clamp(0, 2) as usize];
            }
        }
    }

    /// SET KEY: arm (or, with `Some(note)`, apply) the selected split point.
    pub fn set_key(&mut self, note: Option<u8>) {
        match note {
            None => self.set_key_armed = true,
            Some(n) => {
                self.notes[self.point.min(2)] = n.clamp(0, 127);
                self.enabled[self.point.min(2)] = true;
                self.set_key_armed = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_cover_c2_to_c7_in_eleven_steps() {
        assert_eq!(POSITIONS.len(), 11);
        assert_eq!(note_name(POSITIONS[0]), "C2");
        assert_eq!(note_name(POSITIONS[10]), "C7");
        assert_eq!(note_name(60), "C4");
        assert!(POSITIONS.windows(2).all(|w| w[1] - w[0] == 6));
    }

    #[test]
    fn dial_moves_the_selected_note_and_clamps() {
        let mut s = SplitUi::default();
        assert_eq!(s.notes[1], 60);
        s.dial(1);
        assert_eq!(s.notes[1], 66);
        s.dial(-100);
        assert_eq!(s.notes[1], 36);
        s.dial(100);
        assert_eq!(s.notes[1], 96);
    }

    #[test]
    fn soft_buttons_select_and_deactivate_points() {
        let mut s = SplitUi::default();
        s.soft(2); // Low: activates and selects
        assert!(s.enabled[0] && s.point == 0);
        s.soft(2); // selected again: deactivates
        assert!(!s.enabled[0]);
        s.soft(4); // High
        assert!(s.enabled[2] && s.point == 2);
        s.soft(3); // Mid is active but not selected: only selects
        assert!(s.enabled[1] && s.point == 1);
    }

    #[test]
    fn soft_one_switches_to_the_xfade_row_which_cycles_widths() {
        let mut s = SplitUi::default();
        s.soft(1);
        assert_eq!(s.row, SplitRow::Xfade);
        s.dial(1);
        assert_eq!(s.xfade[1], CrossfadeWidth::Small);
        s.dial(5);
        assert_eq!(s.xfade[1], CrossfadeWidth::Large);
        assert_eq!(xfade_label(s.xfade[1]), "6");
        s.soft(1);
        assert_eq!(s.row, SplitRow::Note);
    }

    #[test]
    fn set_key_arms_then_sets_the_selected_point_exactly() {
        let mut s = SplitUi::default();
        s.set_key(None);
        assert!(s.set_key_armed);
        s.set_key(Some(55));
        assert!(!s.set_key_armed);
        assert_eq!(s.notes[1], 55);
    }

    #[test]
    fn leds_and_cfg_follow_on_and_enabled() {
        let mut s = SplitUi::default();
        assert_eq!(s.leds(), [false; 3]);
        s.on = true;
        assert_eq!(s.leds(), [false, true, false]);
        let c = s.cfg();
        assert!(c.on && c.enabled[1] && c.notes[2] == 72);
    }
}

//! Per-frame housekeeping of the Program area: time, external program changes, snapshot replies,
//! MIDI activity LEDs, list debounce, SET KEY from the keybed.

use super::catalog::LoadRequest;
use super::model::Screen;
use super::nav::cmd;
use crate::audio::{AudioCommand, ProgramEdit};
use crate::panel::ui_state::ProgramUi;

/// MIDI LED blink length for a played key.
///
/// ASSUMED: 120 ms; the manual only says "short" and "longer" blinks (p.41).
pub const MIDI_BLINK_MS: u32 = 120;

impl ProgramUi {
    /// Start of a frame. `keys` are the keys currently sounding from MIDI input; `prog_id` and
    /// `prog_title` are the panel's current values (another part of the app may have changed them,
    /// for example by loading a program from the library drawer).
    pub fn begin_frame(
        &mut self,
        now_ms: u32,
        keys: &[bool; 128],
        prog_id: &str,
        prog_title: &str,
    ) {
        self.now_ms = now_ms;
        self.painted.clear();

        if prog_id != self.last_prog_id {
            if let Some(loc) = super::store::ProgLoc::parse(prog_id) {
                self.loc = loc;
                self.browse = loc;
            }
            self.title = prog_title.to_string();
            self.last_prog_id = prog_id.to_string();
            self.preset_name = false;
            self.mark_clean();
            self.extern_led_until = now_ms.saturating_add(700); // Send On Load: a very long blink
        }

        while let Ok((ticket, state)) = self.reply_rx.try_recv() {
            self.accept_snapshot(ticket, state);
        }

        if keys
            .iter()
            .zip(self.prev_keys.iter())
            .any(|(now, was)| *now && !*was)
        {
            self.midi_led_until = now_ms.saturating_add(MIDI_BLINK_MS);
        }
        self.prev_keys = *keys;

        if self
            .message
            .as_ref()
            .is_some_and(|(_, until)| now_ms >= *until)
        {
            self.message = None;
        }
        if let Some((due, idx)) = self.list_load_due {
            if now_ms >= due {
                self.list_load_due = None;
                if let Some(entry) = self.catalog.programs.get(idx) {
                    self.requests.push(LoadRequest::Program(entry.clone()));
                }
            }
        }
    }

    /// End of a frame: the values the panel state should now carry.
    pub fn write_back(
        &mut self,
        prog_id: &mut String,
        prog_title: &mut String,
        button_idx: &mut u8,
    ) {
        *prog_id = self.loc.label();
        *prog_title = self.title.clone();
        *button_idx = self.loc.slot + 1;
        self.last_prog_id = prog_id.clone();
    }

    /// MIDI LED is lit.
    pub fn midi_led_on(&self) -> bool {
        self.now_ms < self.midi_led_until
    }

    /// EXTERN LED is lit.
    pub fn extern_led_on(&self) -> bool {
        self.now_ms < self.extern_led_until
    }

    /// A time-driven element is active, so the frame must repaint soon.
    pub fn needs_repaint(&self) -> bool {
        self.midi_led_on()
            || self.extern_led_on()
            || self.message.is_some()
            || self.list_load_due.is_some()
            || self.trackers.values().any(|t| t.is_down())
    }

    /// A keybed key was pressed: with SET KEY armed it sets the selected split point.
    pub fn key_pressed(&mut self, note: u8) -> Vec<AudioCommand> {
        if !self.split.set_key_armed {
            return Vec::new();
        }
        self.split.set_key(Some(note));
        self.screen = Screen::Split;
        vec![cmd(ProgramEdit::Split(self.split.cfg()))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_KEYS: [bool; 128] = [false; 128];

    #[test]
    fn external_program_change_is_adopted_once() {
        let mut u = ProgramUi::default();
        u.preset_name = true;
        u.begin_frame(10, &NO_KEYS, "C:87", "Rusty PWM");
        assert_eq!(u.loc.label(), "C:87");
        assert_eq!(u.title, "Rusty PWM");
        assert!(!u.preset_name, "reset on program load");
        assert!(u.extern_led_on());
        u.begin_frame(2000, &NO_KEYS, "C:87", "Other");
        assert_eq!(u.title, "Rusty PWM", "unchanged id is not re-adopted");
        assert!(!u.extern_led_on());
    }

    #[test]
    fn write_back_publishes_the_location_and_button() {
        let mut u = ProgramUi::default();
        u.select_loc(super::super::store::ProgLoc::parse("D:35").unwrap());
        let (mut id, mut title, mut idx) = ("A:11".to_string(), String::new(), 1u8);
        u.write_back(&mut id, &mut title, &mut idx);
        assert_eq!((id.as_str(), idx), ("D:35", 5));
        u.begin_frame(1, &NO_KEYS, &id, &title);
        assert_eq!(u.loc.label(), "D:35", "own write is not an external change");
    }

    #[test]
    fn played_key_blinks_the_midi_led_and_message_expires() {
        let mut u = ProgramUi::default();
        let mut keys = NO_KEYS;
        keys[60] = true;
        u.begin_frame(100, &keys, "A:11", "t");
        assert!(u.midi_led_on());
        u.begin_frame(100 + MIDI_BLINK_MS, &keys, "A:11", "t");
        assert!(!u.midi_led_on(), "held key does not retrigger");
        u.set_message("x");
        u.begin_frame(5000, &NO_KEYS, "A:11", "t");
        assert!(u.message.is_none());
    }

    #[test]
    fn list_load_fires_after_the_dial_rests() {
        let mut u = ProgramUi::default();
        u.sync_catalog(&["a.ns4p".into(), "b.ns4p".into()], &[], &[]);
        u.list_load_due = Some((500, 1));
        u.begin_frame(499, &NO_KEYS, "A:11", "t");
        assert!(u.requests.is_empty() && u.needs_repaint());
        u.begin_frame(500, &NO_KEYS, "A:11", "t");
        assert_eq!(
            u.take_requests(),
            vec![LoadRequest::Program("b.ns4p".into())]
        );
    }

    #[test]
    fn set_key_applies_only_when_armed() {
        let mut u = ProgramUi::default();
        assert!(u.key_pressed(55).is_empty());
        u.split.set_key(None);
        let out = u.key_pressed(55);
        assert_eq!(out.len(), 1);
        assert_eq!(u.split.notes[1], 55);
        assert_eq!(u.screen, Screen::Split);
    }
}

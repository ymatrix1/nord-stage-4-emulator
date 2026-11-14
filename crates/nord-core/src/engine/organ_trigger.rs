//! Sound menu page 4 "Organ Keyboard Trigger Point" (manual OS v1.6x p.58).
//!
//! `High` (the hardware default) triggers the organ before the key is fully depressed, like a real
//! B3; `Low` triggers it later in the key travel. MIDI note-ons carry no key depth, so the emulator
//! models the difference as a fixed trigger delay on Organ note-ons: none for `High`,
//! [`LOW_TRIGGER_DELAY_SEC`] for `Low`. A key released before the delay ran out never sounds. The
//! organ ignores velocity (manual p.56), so only the timing matters. Default `High` delays nothing,
//! so the render is bit-identical to the engine before this setting existed.
//!
//! Provenance: the two choices and the `High` default are `VERIFIED` against the manual (p.58). The
//! delay length is `ASSUMED`: the manual gives no figure; 8 ms is a plausible early-to-late contact
//! spread of a keybed. A recording of the same key press at both settings would confirm it.

use super::*;

/// Trigger delay of the `Low` setting in seconds. `ASSUMED`, see the module docs.
pub const LOW_TRIGGER_DELAY_SEC: f32 = 0.008;
/// Organ note-ons that can wait for their trigger at once; further ones sound immediately.
const PENDING: usize = 16;

/// Trigger point choice (menu order: High, Low).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerPoint {
    #[default]
    High,
    Low,
}

#[derive(Debug, Clone, Copy, Default)]
struct Pending {
    layer: u8,
    note: u8,
    gain: f32,
    /// Samples until the note sounds; `0` = slot free.
    left: u32,
}

/// Trigger point setting plus the note-ons waiting for it.
#[derive(Debug, Clone)]
pub struct OrganTrigger {
    pub point: TriggerPoint,
    pending: [Pending; PENDING],
    active: u8,
}

impl Default for OrganTrigger {
    fn default() -> Self {
        Self {
            point: TriggerPoint::High,
            pending: [Pending::default(); PENDING],
            active: 0,
        }
    }
}

impl OrganTrigger {
    /// Drops every waiting note-on.
    pub(super) fn clear(&mut self) {
        self.pending = [Pending::default(); PENDING];
        self.active = 0;
    }
}

impl Stage4Engine {
    /// Queues an Organ note-on behind the trigger delay. `false` means "play it now" (setting is
    /// `High`, or all slots are busy).
    pub(super) fn defer_organ_note(&mut self, layer: LayerId, note: u8, gain: f32) -> bool {
        if self.organ_trigger.point == TriggerPoint::High {
            return false;
        }
        let delay = (LOW_TRIGGER_DELAY_SEC * self.sample_rate).round().max(1.0) as u32;
        let t = &mut self.organ_trigger;
        let same = |p: &Pending| p.left > 0 && p.layer == layer as u8 && p.note == note;
        let Some(slot) = t
            .pending
            .iter()
            .position(same)
            .or_else(|| t.pending.iter().position(|p| p.left == 0))
        else {
            return false;
        };
        if t.pending[slot].left == 0 {
            t.active += 1;
        }
        t.pending[slot] = Pending {
            layer: layer as u8,
            note,
            gain,
            left: delay,
        };
        true
    }

    /// A release before the trigger fired: drops the waiting note-on. `true` if there was one.
    pub(super) fn cancel_pending_organ(&mut self, layer: LayerId, note: u8) -> bool {
        let t = &mut self.organ_trigger;
        match t
            .pending
            .iter_mut()
            .find(|p| p.left > 0 && p.layer == layer as u8 && p.note == note)
        {
            Some(p) => {
                p.left = 0;
                t.active -= 1;
                true
            }
            None => false,
        }
    }

    /// Advances the trigger delays by one sample and starts the notes that reached their trigger.
    pub(super) fn tick_organ_trigger(&mut self) {
        if self.organ_trigger.active == 0 {
            return;
        }
        for i in 0..PENDING {
            let p = &mut self.organ_trigger.pending[i];
            if p.left == 0 {
                continue;
            }
            p.left -= 1;
            if p.left == 0 {
                let fire = *p;
                self.organ_trigger.active -= 1;
                if fire.layer == LayerId::OrganA as u8 {
                    self.organ_a.note_on_with_gain(fire.note, fire.gain);
                } else {
                    self.organ_b.note_on_with_gain(fire.note, fire.gain);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(point: TriggerPoint) -> Stage4Engine {
        let mut e = Stage4Engine::new(48_000.0);
        e.scene.scene_1_mask = 1 << LayerId::OrganA as u8;
        e.organ_trigger.point = point;
        e
    }

    fn press(e: &mut Stage4Engine, note: u8) {
        e.handle_midi(MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        });
    }

    /// Frames until the first audible sample (`usize::MAX` if none within 100 ms).
    fn onset(e: &mut Stage4Engine) -> usize {
        for n in 0..4800 {
            let [l, r] = e.render_frame();
            if l.abs() + r.abs() > 1e-7 {
                return n;
            }
        }
        usize::MAX
    }

    #[test]
    fn high_is_the_default_and_renders_bit_identically() {
        assert_eq!(OrganTrigger::default().point, TriggerPoint::High);
        let run = |touch: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            if touch {
                e.organ_trigger = OrganTrigger::default();
            }
            press(&mut e, 60);
            (0..2048).map(|_| e.render_frame()).collect::<Vec<_>>()
        };
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn low_delays_the_organ_onset_by_the_trigger_time() {
        let mut high = engine(TriggerPoint::High);
        press(&mut high, 60);
        let mut low = engine(TriggerPoint::Low);
        press(&mut low, 60);
        let (h, l) = (onset(&mut high), onset(&mut low));
        let delay = (LOW_TRIGGER_DELAY_SEC * 48_000.0).round() as usize;
        assert!(l >= h + delay - 1, "high {h} low {l} delay {delay}");
        assert!(l <= h + delay + 8, "high {h} low {l} delay {delay}");
    }

    #[test]
    fn a_key_released_before_the_trigger_never_sounds() {
        let mut e = engine(TriggerPoint::Low);
        press(&mut e, 60);
        e.handle_midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60,
            velocity: 0,
        });
        assert_eq!(e.organ_trigger.active, 0);
        assert_eq!(onset(&mut e), usize::MAX);
    }

    #[test]
    fn a_full_queue_falls_back_to_immediate_sound_and_panic_clears_it() {
        let mut e = engine(TriggerPoint::Low);
        for n in 40..40 + PENDING as u8 {
            press(&mut e, n);
        }
        assert_eq!(usize::from(e.organ_trigger.active), PENDING);
        assert!(!e.defer_organ_note(LayerId::OrganA, 99, 1.0), "queue full");
        e.panic();
        assert_eq!(e.organ_trigger.active, 0);
        assert_eq!(onset(&mut e), usize::MAX);
    }
}

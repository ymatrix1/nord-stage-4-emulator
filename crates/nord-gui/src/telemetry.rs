//! GUI side of the engine telemetry (bean `ns4-gjy2`, spec `docs/specs/telemetry.md`).
//!
//! [`TelemetryHandle`] is created next to the `AudioHandle`; the audio thread publishes into its
//! cell (`nord_core::engine::TelemetryCell`, a lock-free seqlock) and the panel reads one
//! [`PanelTelemetry`] per frame. Every accessor takes the value the UI would have shown without
//! the engine and returns it unchanged when no snapshot exists (no audio device, headless tests),
//! so a bay needs one call per LED and no `Option` handling.

use nord_core::engine::{Telemetry, TelemetryCell};
use nord_core::organ::RotarySpeed;
use std::sync::Arc;

/// MIDI / EXTERN LED blink length after a message.
///
/// ASSUMED: 120 ms like the Program area's key blink (`bay_program::frame::MIDI_BLINK_MS`); the
/// manual only says "short" and "longer" blinks (p.41).
pub const ACTIVITY_BLINK_MS: u32 = 120;

/// Shared reader handle onto the engine's telemetry cell. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct TelemetryHandle {
    cell: Arc<TelemetryCell>,
}

impl TelemetryHandle {
    /// A handle with an empty cell (nothing published yet).
    pub fn new() -> Self {
        Self::default()
    }

    /// The cell the audio thread publishes into (`Stage4Engine::attach_telemetry`).
    pub fn cell(&self) -> Arc<TelemetryCell> {
        Arc::clone(&self.cell)
    }

    /// Newest snapshot, wrapped for the panel. Lock-free, allocation-free.
    pub fn latest(&self) -> PanelTelemetry {
        PanelTelemetry(self.cell.read())
    }
}

/// `true` when the engine plays layer `layer` through the Aux keyboard.
///
/// The Organ, Piano and Synth bays call this for their Aux KB LEDs; `layer` is the engine layer
/// index (Organ A/B `0..2`, Piano A/B `2..4`, Synth A/B/C `4..7`).
pub fn aux_flag_from_telemetry(t: &Telemetry, layer: usize) -> bool {
    t.layer_aux(layer)
}

/// One frame's snapshot of the engine, or nothing when the engine is not running.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PanelTelemetry(pub Option<Telemetry>);

impl PanelTelemetry {
    /// The raw snapshot.
    pub fn get(&self) -> Option<&Telemetry> {
        self.0.as_ref()
    }

    /// Level the 11-segment ladder of `layer` shows: the engine's level after fader and Morph (`0`
    /// when the layer is off), else the UI's own `ui_level` / `ui_active` reading.
    pub fn ladder_level(&self, layer: usize, ui_level: f32, ui_active: bool) -> f32 {
        match &self.0 {
            Some(t) if layer < 7 => {
                if t.layer_active(layer) {
                    t.layer_level[layer]
                } else {
                    0.0
                }
            }
            _ => {
                if ui_active {
                    ui_level
                } else {
                    0.0
                }
            }
        }
    }

    /// Aux KB LED of `layer`; `false` without a snapshot (the UI has no Aux state of its own).
    pub fn aux(&self, layer: usize) -> bool {
        self.0
            .as_ref()
            .is_some_and(|t| aux_flag_from_telemetry(t, layer))
    }

    /// Drawbar `index` (`0..9`) of Organ A (`organ_b = false`) or B as the engine plays it.
    pub fn drawbar(&self, organ_b: bool, index: usize, ui_value: u8) -> u8 {
        self.0
            .as_ref()
            .and_then(|t| t.organ_drawbars[usize::from(organ_b)].get(index).copied())
            .unwrap_or(ui_value)
    }

    /// Rotary FAST LED: the real rotor state (pedals can change it), else the UI toggle.
    pub fn rotary_fast(&self, ui_fast: bool) -> bool {
        self.0.as_ref().map_or(ui_fast, |t| {
            t.rotary_speed == RotarySpeed::Fast && !t.rotary_stop
        })
    }

    /// Rotary SLOW/STOP LED, the complement of [`Self::rotary_fast`].
    pub fn rotary_slow(&self, ui_fast: bool) -> bool {
        !self.rotary_fast(ui_fast)
    }

    /// The L / M / H split LEDs from the engine split state, else the UI's.
    pub fn split_leds(&self, ui: [bool; 3]) -> [bool; 3] {
        self.0.as_ref().map_or(ui, Telemetry::split_leds)
    }

    /// Sixteenth `0..=3` inside the current Master Clock beat; `None` without an engine.
    pub fn arp_step(&self) -> Option<u8> {
        self.0.as_ref().map(|t| t.arp_step)
    }

    /// `true` only when the engine is running and reports nothing to undo.
    pub fn undo_blocked(&self) -> bool {
        self.0.as_ref().is_some_and(|t| t.undo.is_none())
    }

    /// `true` only when the engine reports an undone operation to redo.
    pub fn redo_available(&self) -> bool {
        self.0.as_ref().is_some_and(|t| t.redo.is_some())
    }
}

/// Blink timer of one activity LED: lights for [`ACTIVITY_BLINK_MS`] whenever its counter moves.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActivityLed {
    seen: Option<u32>,
    until_ms: u32,
}

impl ActivityLed {
    /// Feeds the engine's activity counter. The first value only establishes the baseline.
    pub fn update(&mut self, count: u32, now_ms: u32) {
        if self.seen.is_some_and(|prev| prev != count) {
            self.until_ms = now_ms.saturating_add(ACTIVITY_BLINK_MS);
        }
        self.seen = Some(count);
    }

    /// LED is lit at `now_ms`.
    pub fn is_on(&self, now_ms: u32) -> bool {
        now_ms < self.until_ms
    }
}

/// The MIDI and EXTERN LEDs of the Program area.
#[derive(Debug, Clone, Copy, Default)]
pub struct PanelLeds {
    pub midi: ActivityLed,
    pub extern_out: ActivityLed,
    now_ms: u32,
}

impl PanelLeds {
    /// Once per frame: advance the blink timers from the snapshot.
    pub fn update(&mut self, t: &PanelTelemetry, now_ms: u32) {
        self.now_ms = now_ms;
        if let Some(t) = t.get() {
            self.midi.update(t.midi_in_count, now_ms);
            self.extern_out.update(t.midi_out_count, now_ms);
        }
    }

    /// MIDI LED lit by real MIDI input.
    pub fn midi_on(&self) -> bool {
        self.midi.is_on(self.now_ms)
    }

    /// EXTERN LED lit by real outgoing Extern messages.
    pub fn extern_on(&self) -> bool {
        self.extern_out.is_on(self.now_ms)
    }

    /// A blink is running, so the frame must repaint to turn the LED off again.
    pub fn needs_repaint(&self) -> bool {
        self.midi_on() || self.extern_on()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_core::engine::Stage4Engine;

    fn live(e: &Stage4Engine) -> PanelTelemetry {
        PanelTelemetry(Some(e.telemetry()))
    }

    #[test]
    fn the_clock_step_is_none_without_an_engine_and_follows_the_engine() {
        assert_eq!(PanelTelemetry(None).arp_step(), None);
        let t = nord_core::engine::Telemetry {
            arp_step: 2,
            ..Default::default()
        };
        assert_eq!(PanelTelemetry(Some(t)).arp_step(), Some(2));
    }

    #[test]
    fn handle_round_trips_a_published_snapshot() {
        let h = TelemetryHandle::new();
        assert!(h.latest().get().is_none(), "nothing published yet");
        let mut e = Stage4Engine::new(48_000.0);
        e.attach_telemetry(h.cell());
        e.master_clock.bpm = 101.0;
        e.publish_telemetry();
        assert_eq!(h.clone().latest().get().map(|t| t.bpm), Some(101.0));
    }

    #[test]
    fn ladder_shows_the_morphed_level_and_falls_back_to_the_ui() {
        let none = PanelTelemetry::default();
        assert_eq!(none.ladder_level(2, 0.5, true), 0.5);
        assert_eq!(none.ladder_level(2, 0.5, false), 0.0);

        let mut e = Stage4Engine::new(48_000.0);
        e.layer_levels[2].base = 0.4;
        e.layer_levels[2].delta_wheel = 0.5;
        e.morph_controllers.mod_wheel = 1.0;
        let t = live(&e);
        assert!(
            (t.ladder_level(2, 0.4, true) - 0.9).abs() < 1e-6,
            "morphed, not the fader"
        );
        assert_eq!(
            t.ladder_level(3, 0.7, true),
            0.0,
            "engine says Piano B is off"
        );
        assert_eq!(
            t.ladder_level(9, 0.7, true),
            0.7,
            "unknown layer keeps the UI"
        );
    }

    #[test]
    fn aux_drawbar_rotary_split_and_undo_follow_the_engine() {
        let mut e = Stage4Engine::new(48_000.0);
        e.aux_kb.enabled = true;
        e.aux_kb.set_section_masks(0, 0b10, 0);
        e.organ_a.drawbars = [1, 2, 3, 4, 5, 6, 7, 8, 0];
        e.organ_a.rotary_speed = RotarySpeed::Fast;
        e.split_router.splits[2].enabled = true;
        let t = live(&e);
        assert!(t.aux(3) && !t.aux(2));
        assert!(aux_flag_from_telemetry(t.get().unwrap(), 3));
        assert!(!PanelTelemetry::default().aux(3));
        assert_eq!(t.drawbar(false, 3, 0), 4);
        assert_eq!(PanelTelemetry::default().drawbar(false, 3, 6), 6);
        assert!(t.rotary_fast(false) && !t.rotary_slow(false));
        assert!(PanelTelemetry::default().rotary_fast(true));
        e.organ_a.rotary_stop_mode = true;
        assert!(live(&e).rotary_slow(true), "brake wins over the speed");
        assert_eq!(t.split_leds([true; 3]), [false, false, true]);
        assert_eq!(PanelTelemetry::default().split_leds([true; 3]), [true; 3]);
        assert!(t.undo_blocked() && !t.redo_available());
        assert!(!PanelTelemetry::default().undo_blocked());
        e.all_fx_off();
        assert!(!live(&e).undo_blocked());
        e.undo();
        assert!(live(&e).redo_available());
    }

    #[test]
    fn activity_led_blinks_on_change_only() {
        let mut led = ActivityLed::default();
        led.update(5, 1000);
        assert!(!led.is_on(1000), "first value is a baseline, not activity");
        led.update(5, 1010);
        assert!(!led.is_on(1010));
        led.update(6, 1020);
        assert!(led.is_on(1020) && led.is_on(1020 + ACTIVITY_BLINK_MS - 1));
        assert!(!led.is_on(1020 + ACTIVITY_BLINK_MS));
        led.update(0, 2000);
        assert!(led.is_on(2001), "a wrapped counter is still a change");
    }

    #[test]
    fn panel_leds_follow_midi_and_extern_counters() {
        let mut e = Stage4Engine::new(48_000.0);
        let mut leds = PanelLeds::default();
        leds.update(&live(&e), 100);
        assert!(!leds.midi_on() && !leds.extern_on() && !leds.needs_repaint());
        e.handle_midi(nord_core::midi::MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 90,
        });
        e.extern_out
            .queue
            .push(nord_core::engine::MidiOut::default());
        leds.update(&live(&e), 200);
        assert!(leds.midi_on() && leds.extern_on() && leds.needs_repaint());
        leds.update(&live(&e), 200 + ACTIVITY_BLINK_MS);
        assert!(!leds.midi_on() && !leds.extern_on());
        leds.update(&PanelTelemetry::default(), 5000);
        assert!(!leds.midi_on(), "no snapshot, no activity");
    }
}

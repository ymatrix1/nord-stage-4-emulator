//! Lock-free keybed event queue from the editor (GUI thread) into `process()` (audio thread).
//!
//! A bounded `crossbeam_channel` is an array queue: `try_send` / `try_recv` neither lock nor
//! allocate after construction, so `process()` stays allocation-free
//! (`.agents/rules/dsp-and-realtime.md`). One producer (the editor) and one consumer (the
//! plugin) is the only configuration used.

use crate::midi::route_plugin_event;
use crate::params::{MidiRoutingMode, Stage4Params};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use nih_plug::prelude::NoteEvent;
use nord_core::engine::Stage4Engine;
use nord_core::midi::MidiEvent;

/// Pending events the queue holds; a full queue drops the newest event.
///
/// ASSUMED: 256 events is far above what a mouse-driven keybed produces per audio block (a few
/// events per 10 ms); confirm by profiling with a glissando in a host.
pub const NOTE_QUEUE_CAPACITY: usize = 256;

/// Producer end, owned by the editor.
#[derive(Clone)]
pub struct NoteSender(Sender<MidiEvent>);

/// Consumer end, owned by the plugin and drained in `process()`.
#[derive(Clone)]
pub struct NoteReceiver(Receiver<MidiEvent>);

/// New queue with `capacity` slots.
pub fn note_queue(capacity: usize) -> (NoteSender, NoteReceiver) {
    let (tx, rx) = bounded(capacity);
    (NoteSender(tx), NoteReceiver(rx))
}

impl NoteSender {
    /// Queue one event; `false` when the queue is full (the event is dropped).
    pub fn push(&self, ev: MidiEvent) -> bool {
        !matches!(self.0.try_send(ev), Err(TrySendError::Full(_)))
    }
}

impl NoteReceiver {
    /// Next pending event, if any. Never blocks, never allocates.
    pub fn pop(&self) -> Option<MidiEvent> {
        self.0.try_recv().ok()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Feed one queued keybed event to the engine, honouring the plugin's MIDI routing mode.
///
/// Notes go through `route_plugin_event` so Multitimbral-per-layer routing applies to the
/// on-screen keys exactly as to host MIDI; everything else goes to `Stage4Engine::handle_midi`.
pub fn apply_to_engine(
    engine: &mut Stage4Engine,
    params: &Stage4Params,
    mode: MidiRoutingMode,
    ev: MidiEvent,
) {
    // Local Control Off (MIDI menu page 1): the on-screen keys only transmit.
    if !engine.midi_menu.local_control {
        engine.handle_keyboard_midi(ev);
        return;
    }
    let note_event = match ev {
        MidiEvent::NoteOn {
            channel,
            note,
            velocity,
        } => Some(NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel,
            note,
            velocity: f32::from(velocity) / 127.0,
        }),
        MidiEvent::NoteOff {
            channel,
            note,
            velocity,
        } => Some(NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel,
            note,
            velocity: f32::from(velocity) / 127.0,
        }),
        _ => None,
    };
    match note_event {
        Some(e) => route_plugin_event(engine, params, mode, e),
        None => engine.handle_midi(ev),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(note: u8) -> MidiEvent {
        MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        }
    }

    #[test]
    fn queue_is_fifo_and_drops_when_full() {
        let (tx, rx) = note_queue(2);
        assert!(rx.is_empty());
        assert!(tx.push(on(60)));
        assert!(tx.push(on(61)));
        assert!(!tx.push(on(62)), "third event must be dropped");
        assert_eq!(rx.len(), 2);
        assert_eq!(rx.pop(), Some(on(60)));
        assert_eq!(rx.pop(), Some(on(61)));
        assert_eq!(rx.pop(), None);
    }

    #[test]
    fn queue_crosses_threads() {
        let (tx, rx) = note_queue(NOTE_QUEUE_CAPACITY);
        let h = std::thread::spawn(move || {
            for n in 0..100u8 {
                assert!(tx.push(on(n)));
            }
        });
        h.join().unwrap();
        let got: Vec<u8> = std::iter::from_fn(|| rx.pop())
            .map(|e| match e {
                MidiEvent::NoteOn { note, .. } => note,
                _ => 255,
            })
            .collect();
        assert_eq!(got, (0..100u8).collect::<Vec<_>>());
    }

    #[test]
    fn note_events_reach_the_engine_in_both_routing_modes() {
        let params = Stage4Params::default();
        for mode in [
            MidiRoutingMode::OmniSplit,
            MidiRoutingMode::MultitimbralPerLayer,
        ] {
            let mut engine = Stage4Engine::new(48_000.0);
            params.sync_to_engine(&mut engine);
            apply_to_engine(&mut engine, &params, mode, on(60));
            let mut peak = 0.0f32;
            for _ in 0..4_000 {
                let [l, r] = engine.render_frame();
                peak = peak.max(l.abs()).max(r.abs());
            }
            assert!(peak > 0.0, "{mode:?}: key press produced no audio");
        }
    }

    #[test]
    fn local_control_off_keeps_the_on_screen_keys_silent_but_transmits_them() {
        let params = Stage4Params::default();
        let mut engine = Stage4Engine::new(48_000.0);
        engine.midi_menu.local_control = false;
        apply_to_engine(&mut engine, &params, MidiRoutingMode::OmniSplit, on(60));
        let peak = (0..4_000).fold(0.0f32, |m, _| m.max(engine.render_frame()[0].abs()));
        assert_eq!(peak, 0.0);
        assert_eq!(
            engine.pop_midi_out().map(|m| m.bytes()),
            Some([0x90, 60, 100])
        );
    }

    #[test]
    fn non_note_events_use_handle_midi() {
        let params = Stage4Params::default();
        let mut engine = Stage4Engine::new(48_000.0);
        apply_to_engine(
            &mut engine,
            &params,
            MidiRoutingMode::OmniSplit,
            MidiEvent::ControlChange {
                channel: 0,
                controller: nord_core::midi::cc::MOD_WHEEL,
                value: 127,
            },
        );
        assert!(engine.morph_controllers.mod_wheel > 0.9);
    }
}

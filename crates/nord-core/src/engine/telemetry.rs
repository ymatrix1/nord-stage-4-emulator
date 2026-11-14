//! Engine to GUI telemetry: a fixed-size, plain-data snapshot of what the panel LEDs show.
//!
//! Spec: `docs/specs/telemetry.md`. [`Stage4Engine::telemetry`] builds a [`Telemetry`] from the
//! engine state without allocating; [`Stage4Engine::publish_telemetry`] stores it in a
//! [`TelemetryCell`] (a seqlock, `std` atomics only) that GUI threads read lock-free. The audio
//! callback publishes once per block, never per sample.

use super::*;
use std::sync::Arc;

pub use super::telemetry_cell::TelemetryCell;

/// Number of `u32` words of the wire format (see [`Telemetry::to_words`]).
pub const WORDS: usize = 34;

/// Everything the front panel reads back from the running engine.
///
/// `Copy`, no heap data: the audio thread builds it on the stack and copies it into the cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    /// Level of each layer after fader and Morph (`MorphParameter::evaluate`, `0.0..=1.0`); what
    /// the 11-segment LED ladders show. Excludes the program level and the master volume.
    pub layer_level: [f32; 7],
    /// Bit `i` set: layer `i` plays in the current scene.
    pub active_mask: u8,
    /// Bit `i` set: layer `i` is played by the Aux keyboard (its Aux KB LED).
    pub aux_mask: u8,
    /// Number of keys held per layer (all channels, saturating at 255).
    pub layer_held: [u8; 7],
    /// Morph source values: mod wheel, control pedal, aftertouch (`0.0..=1.0`).
    pub morph: [f32; 3],
    /// Bit `n % 32` of word `n / 32`: key `n` is held on some layer.
    pub held_notes: [u32; 4],
    /// Bit `p` set: split point `p` (Low, Mid, High) is enabled (the L / M / H LEDs).
    pub split_enabled: u8,
    /// Best gain any active layer gives the four keybed zones (`KeybedSplitRouter::note_gain`).
    pub split_zone_gain: [f32; 4],
    /// MIDI events received since start (wraps); the MIDI LED blinks when it changes.
    pub midi_in_count: u32,
    /// MIDI messages queued for the Extern output since start (wraps); the EXTERN LED blinks.
    pub midi_out_count: u32,
    /// Outgoing messages dropped because the queue was full (overflow counter).
    pub midi_out_dropped: u32,
    /// Outgoing messages waiting in the queue right now (memory pressure).
    pub midi_out_pending: u32,
    /// Bit `i` set: Synth layer `i` is in Extern mode.
    pub extern_mask: u8,
    /// Effective drawbars of Organ A and B (`OrganLayer::active_drawbars`, `0..=8`).
    ///
    /// ASSUMED: the engine has no drawbar morph, so "morphed" equals the effective value; a future
    /// morph route changes this field only.
    pub organ_drawbars: [[u8; 9]; 2],
    /// Rotary speed state of the organ rotor (`Stop`, `Slow`, `Fast`) including pedal changes.
    pub rotary_speed: RotarySpeed,
    /// Rotor brake engaged (overrides `rotary_speed`).
    pub rotary_stop: bool,
    /// Scene II is the active scene.
    pub scene_2: bool,
    /// Program transpose in semitones.
    pub transpose: i8,
    /// MST CLK tempo in BPM.
    pub bpm: f32,
    /// Sixteenth-note step `0..=3` inside the current MST CLK beat.
    ///
    /// ASSUMED: the engine has no arpeggiator; this is the clock step an arpeggiator would use.
    pub arp_step: u8,
    /// Operation an UNDO would revert.
    pub undo: Option<UndoOp>,
    /// Operation a REDO would restore.
    pub redo: Option<UndoOp>,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            layer_level: [0.0; 7],
            active_mask: 0,
            aux_mask: 0,
            layer_held: [0; 7],
            morph: [0.0; 3],
            held_notes: [0; 4],
            split_enabled: 0,
            split_zone_gain: [0.0; 4],
            midi_in_count: 0,
            midi_out_count: 0,
            midi_out_dropped: 0,
            midi_out_pending: 0,
            extern_mask: 0,
            organ_drawbars: [[0; 9]; 2],
            rotary_speed: RotarySpeed::Slow,
            rotary_stop: false,
            scene_2: false,
            transpose: 0,
            bpm: 120.0,
            arp_step: 0,
            undo: None,
            redo: None,
        }
    }
}

/// Sequential word writer of the wire format.
struct Out([u32; WORDS], usize);

impl Out {
    fn word(&mut self, v: u32) {
        self.0[self.1] = v;
        self.1 += 1;
    }
    fn float(&mut self, v: f32) {
        self.word(v.to_bits());
    }
    fn bytes(&mut self, b: &[u8]) {
        for chunk in b.chunks(4) {
            let mut w = [0u8; 4];
            w[..chunk.len()].copy_from_slice(chunk);
            self.word(u32::from_le_bytes(w));
        }
    }
}

/// Sequential word reader, the inverse of [`Out`].
struct In<'a>(&'a [u32; WORDS], usize);

impl In<'_> {
    fn word(&mut self) -> u32 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn float(&mut self) -> f32 {
        f32::from_bits(self.word())
    }
    fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        for chunk in out.chunks_mut(4) {
            let w = self.word().to_le_bytes();
            chunk.copy_from_slice(&w[..chunk.len()]);
        }
        out
    }
}

fn undo_code(op: Option<UndoOp>) -> u8 {
    op.map_or(0, |o| o.index() + 1)
}

fn undo_from_code(code: u8) -> Option<UndoOp> {
    usize::from(code)
        .checked_sub(1)
        .and_then(|i| UndoOp::ALL.get(i))
        .copied()
}

impl Telemetry {
    /// Serialises into the fixed word layout the seqlock stores.
    pub fn to_words(&self) -> [u32; WORDS] {
        let mut o = Out([0; WORDS], 0);
        self.layer_level.iter().for_each(|&l| o.float(l));
        o.bytes(&[
            self.active_mask,
            self.aux_mask,
            self.split_enabled,
            self.extern_mask,
        ]);
        let speed = match self.rotary_speed {
            RotarySpeed::Stop => 0,
            RotarySpeed::Slow => 1,
            RotarySpeed::Fast => 2,
        };
        o.bytes(&[
            u8::from(self.scene_2) | u8::from(self.rotary_stop) << 1 | speed << 2,
            self.transpose as u8,
            self.arp_step,
            undo_code(self.undo),
        ]);
        o.bytes(&self.layer_held);
        self.morph.iter().for_each(|&m| o.float(m));
        self.held_notes.iter().for_each(|&w| o.word(w));
        self.split_zone_gain.iter().for_each(|&g| o.float(g));
        o.word(self.midi_in_count);
        o.word(self.midi_out_count);
        o.word(self.midi_out_dropped);
        o.word(self.midi_out_pending);
        o.bytes(&self.organ_drawbars[0]);
        o.bytes(&self.organ_drawbars[1]);
        o.float(self.bpm);
        o.bytes(&[undo_code(self.redo)]);
        debug_assert_eq!(o.1, WORDS, "wire layout and WORDS disagree");
        o.0
    }

    /// Inverse of [`Self::to_words`].
    pub fn from_words(w: &[u32; WORDS]) -> Self {
        let mut i = In(w, 0);
        let layer_level = std::array::from_fn(|_| i.float());
        let [active_mask, aux_mask, split_enabled, extern_mask] = i.bytes::<4>();
        let [flags, transpose, arp_step, undo] = i.bytes::<4>();
        let layer_held = i.bytes::<7>();
        let morph = std::array::from_fn(|_| i.float());
        let held_notes = std::array::from_fn(|_| i.word());
        let split_zone_gain = std::array::from_fn(|_| i.float());
        let midi_in_count = i.word();
        let midi_out_count = i.word();
        let midi_out_dropped = i.word();
        let midi_out_pending = i.word();
        let organ_drawbars = [i.bytes::<9>(), i.bytes::<9>()];
        let bpm = i.float();
        let [redo] = i.bytes::<1>();
        Self {
            layer_level,
            active_mask,
            aux_mask,
            layer_held,
            morph,
            held_notes,
            split_enabled,
            split_zone_gain,
            midi_in_count,
            midi_out_count,
            midi_out_dropped,
            midi_out_pending,
            extern_mask,
            organ_drawbars,
            rotary_speed: match flags >> 2 & 3 {
                0 => RotarySpeed::Stop,
                1 => RotarySpeed::Slow,
                _ => RotarySpeed::Fast,
            },
            rotary_stop: flags & 2 != 0,
            scene_2: flags & 1 != 0,
            transpose: transpose as i8,
            bpm,
            arp_step,
            undo: undo_from_code(undo),
            redo: undo_from_code(redo),
        }
    }

    /// Layer `i` (`0..7`) plays in the current scene.
    pub fn layer_active(&self, i: usize) -> bool {
        i < 7 && self.active_mask >> i & 1 != 0
    }

    /// Layer `i` is played by the Aux keyboard.
    pub fn layer_aux(&self, i: usize) -> bool {
        i < 7 && self.aux_mask >> i & 1 != 0
    }

    /// Key `note` is held on some layer.
    pub fn note_held(&self, note: u8) -> bool {
        self.held_notes[usize::from(note >> 5) & 3] >> (note & 31) & 1 != 0
    }

    /// The L / M / H split LEDs.
    pub fn split_leds(&self) -> [bool; 3] {
        std::array::from_fn(|p| self.split_enabled >> p & 1 != 0)
    }
}

/// The cell an engine publishes to. Cloning an engine does not clone the sink (a cell has exactly
/// one writer), so a cloned engine publishes nowhere until [`Stage4Engine::attach_telemetry`].
#[derive(Debug, Default)]
pub struct TelemetrySink(Option<Arc<TelemetryCell>>);

impl Clone for TelemetrySink {
    fn clone(&self) -> Self {
        Self(None)
    }
}

impl Stage4Engine {
    /// Builds the current [`Telemetry`]. Pure read, no allocation.
    pub fn telemetry(&self) -> Telemetry {
        let ctrl = &self.morph_controllers;
        let mut t = Telemetry {
            morph: [ctrl.mod_wheel, ctrl.control_pedal, ctrl.aftertouch],
            midi_in_count: self.midi_in_count,
            midi_out_count: self.extern_out.queue.pushed(),
            midi_out_dropped: self.extern_out.queue.dropped(),
            midi_out_pending: self.extern_out.queue.len() as u32,
            organ_drawbars: [
                *self.organ_a.active_drawbars(),
                *self.organ_b.active_drawbars(),
            ],
            rotary_speed: self.organ_a.rotary_speed,
            rotary_stop: self.organ_a.rotary_stop_mode,
            scene_2: self.scene.scene_2_active,
            transpose: self.transpose,
            bpm: self.master_clock.bpm,
            arp_step: ((self.master_clock.beat_phase.clamp(0.0, 1.0) * 4.0) as u8).min(3),
            ..Telemetry::default()
        };
        for layer in LayerId::ALL {
            let i = layer as usize;
            t.layer_level[i] = self.layer_levels[i].evaluate(ctrl);
            t.active_mask |= u8::from(self.scene.is_layer_active(layer)) << i;
            t.aux_mask |= u8::from(self.aux_kb.is_aux_layer(layer)) << i;
        }
        for (p, s) in self.split_router.splits.iter().enumerate() {
            t.split_enabled |= u8::from(s.enabled) << p;
        }
        for (i, layer) in self.extern_out.layers.iter().enumerate() {
            t.extern_mask |= u8::from(layer.enabled) << i;
        }
        for notes in &self.note_hold_counts {
            for (n, layers) in notes.iter().enumerate() {
                for (l, &c) in layers.iter().enumerate() {
                    if c > 0 {
                        t.held_notes[n >> 5] |= 1 << (n & 31);
                        t.layer_held[l] = t.layer_held[l].saturating_add(c.min(1));
                    }
                }
            }
        }
        t.split_zone_gain = self.zone_gains();
        (t.undo, t.redo) = self.undo_status();
        t
    }

    /// Gain of the four keybed zones: the best `note_gain` of any active layer at a note inside
    /// each zone (zone edges are the Low / Mid / High split notes).
    fn zone_gains(&self) -> [f32; 4] {
        let s = &self.split_router.splits;
        let (lo, mid, hi) = (s[0].midi_note, s[1].midi_note, s[2].midi_note);
        let probe = [
            lo.saturating_sub(12),
            ((u16::from(lo) + u16::from(mid)) / 2) as u8,
            ((u16::from(mid) + u16::from(hi)) / 2) as u8,
            hi.saturating_add(12).min(127),
        ];
        std::array::from_fn(|z| {
            LayerId::ALL
                .into_iter()
                .filter(|&l| self.scene.is_layer_active(l))
                .map(|l| self.split_router.note_gain(l, probe[z]))
                .fold(0.0, f32::max)
        })
    }

    /// Operations an UNDO / REDO would act on right now.
    pub fn undo_status(&self) -> (Option<UndoOp>, Option<UndoOp>) {
        (self.undo_available(), self.redo_available())
    }

    /// Routes [`Self::publish_telemetry`] into `cell`.
    pub fn attach_telemetry(&mut self, cell: Arc<TelemetryCell>) {
        self.telemetry_out = TelemetrySink(Some(cell));
    }

    /// Publishes the current snapshot to the attached cell (no-op without one). Audio-thread safe:
    /// no allocation, lock or I/O. Call once per block.
    pub fn publish_telemetry(&self) {
        if let Some(cell) = &self.telemetry_out.0 {
            cell.publish(&self.telemetry());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// More snapshot tests live in `tests_telemetry.rs`; this one pins the wire helpers.
    #[test]
    fn undo_codes_round_trip_and_reject_out_of_range() {
        assert_eq!(undo_from_code(undo_code(None)), None);
        for op in UndoOp::ALL {
            assert_eq!(undo_from_code(undo_code(Some(op))), Some(op));
        }
        assert_eq!(undo_from_code(200), None);
    }

    #[test]
    fn word_budget_matches_the_layout() {
        let words = Telemetry::default().to_words();
        assert_eq!(words.len(), WORDS);
    }
}

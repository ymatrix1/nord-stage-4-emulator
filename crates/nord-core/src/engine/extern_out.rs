//! Extern layer mode: a Synth layer that emits MIDI out instead of rendering audio.
//!
//! Output goes through a fixed-capacity ring buffer (`MidiOutQueue`) that backends drain with
//! `Stage4Engine::pop_midi_out`; nothing here allocates. Spec: `docs/specs/midi-and-control.md`
//! (Extern, manual OS v1.6x p.46-47, p.55, p.62-64).

use super::*;

/// Capacity of the outgoing MIDI ring buffer. ASSUMED: 256 events per drain interval is ample for
/// one audio block; overflow drops the newest event and counts it in `MidiOutQueue::dropped`.
pub const MIDI_OUT_CAPACITY: usize = 256;
const NO_NOTE: u8 = 0xFF;
const VOLUME_UNSET: u8 = 0xFF;

/// One outgoing channel-voice message (`len` is 2 for Program Change, otherwise 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MidiOut {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub len: u8,
}

impl MidiOut {
    pub fn note_on(channel: u8, note: u8, velocity: u8) -> Self {
        Self::three(0x90, channel, note, velocity)
    }
    pub fn note_off(channel: u8, note: u8) -> Self {
        Self::three(0x80, channel, note, 0)
    }
    pub fn control_change(channel: u8, controller: u8, value: u8) -> Self {
        Self::three(0xB0, channel, controller, value)
    }
    pub fn program_change(channel: u8, program: u8) -> Self {
        Self {
            status: 0xC0 | (channel & 0x0F),
            data1: program & 0x7F,
            data2: 0,
            len: 2,
        }
    }
    /// `value` is the 14-bit bend, `0..=16383`.
    pub fn pitch_bend(channel: u8, value: u16) -> Self {
        Self::three(
            0xE0,
            channel,
            (value & 0x7F) as u8,
            ((value >> 7) & 0x7F) as u8,
        )
    }
    fn three(kind: u8, channel: u8, d1: u8, d2: u8) -> Self {
        Self {
            status: kind | (channel & 0x0F),
            data1: d1 & 0x7F,
            data2: d2 & 0x7F,
            len: 3,
        }
    }
    /// The wire bytes (`&bytes[..len]`).
    pub fn bytes(&self) -> [u8; 3] {
        [self.status, self.data1, self.data2]
    }
}

/// Fixed-capacity FIFO of outgoing MIDI; never allocates.
#[derive(Debug, Clone)]
pub struct MidiOutQueue {
    buf: [MidiOut; MIDI_OUT_CAPACITY],
    head: usize,
    len: usize,
    dropped: u32,
    pushed: u32,
}

impl Default for MidiOutQueue {
    fn default() -> Self {
        Self {
            buf: [MidiOut::default(); MIDI_OUT_CAPACITY],
            head: 0,
            len: 0,
            dropped: 0,
            pushed: 0,
        }
    }
}

impl MidiOutQueue {
    /// Appends `event`; returns `false` (and counts a drop) when the queue is full.
    pub fn push(&mut self, event: MidiOut) -> bool {
        if self.len == MIDI_OUT_CAPACITY {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        self.buf[(self.head + self.len) % MIDI_OUT_CAPACITY] = event;
        self.len += 1;
        self.pushed = self.pushed.wrapping_add(1);
        true
    }
    pub fn pop(&mut self) -> Option<MidiOut> {
        if self.len == 0 {
            return None;
        }
        let event = self.buf[self.head];
        self.head = (self.head + 1) % MIDI_OUT_CAPACITY;
        self.len -= 1;
        Some(event)
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Events discarded because the queue was full.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }
    /// Events accepted since creation (wraps); the GUI blinks the EXTERN / MIDI-out LED on change.
    pub fn pushed(&self) -> u32 {
        self.pushed
    }
}

/// Settings of one Extern layer (per program, except `channel`; manual p.46-47, p.63-64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternLayer {
    /// Synth layer is in Extern mode.
    pub enabled: bool,
    /// Transmit channel `0..=15` (MIDI menu "Extern Channel", defaults 13/14/15; `None` = Off).
    pub channel: Option<u8>,
    /// Outgoing note transposition in octaves.
    pub octave_shift: i8,
    /// The two user controller numbers (Extern menu "CC Number 1/2", `0..=119`, default 2).
    pub cc_numbers: [u8; 2],
    /// Their values (Synth display CC VAL dials).
    pub cc_values: [u8; 2],
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
    /// "Send On Load" (default Off).
    pub send_on_load: bool,
}

impl ExternLayer {
    fn new(channel: u8) -> Self {
        Self {
            enabled: false,
            channel: Some(channel),
            octave_shift: 0,
            cc_numbers: [2, 2],
            cc_values: [0, 0],
            bank_msb: 0,
            bank_lsb: 0,
            program: 0,
            send_on_load: false,
        }
    }
}

/// Extern state of the three Synth layers plus the output queue.
#[derive(Debug, Clone)]
pub struct ExternState {
    pub layers: [ExternLayer; 3],
    pub queue: MidiOutQueue,
    sent_note: [[u8; 128]; 3],
    last_volume: [u8; 3],
    last_sustain: [u8; 3],
    last_bend: [u16; 3],
}

impl Default for ExternState {
    fn default() -> Self {
        Self {
            layers: [
                ExternLayer::new(12),
                ExternLayer::new(13),
                ExternLayer::new(14),
            ],
            queue: MidiOutQueue::default(),
            sent_note: [[NO_NOTE; 128]; 3],
            last_volume: [VOLUME_UNSET; 3],
            last_sustain: [0; 3],
            last_bend: [8192; 3],
        }
    }
}

impl Stage4Engine {
    /// Pops the next outgoing MIDI message, if any. Allocation-free; call from the backend.
    pub fn pop_midi_out(&mut self) -> Option<MidiOut> {
        self.extern_out.queue.pop()
    }

    /// Extern slot `0..3` for `layer` when it is a Synth layer in Extern mode.
    pub(super) fn extern_index(&self, layer: LayerId) -> Option<usize> {
        let i = (layer as usize).checked_sub(LayerId::SynthA as usize)?;
        self.extern_out
            .layers
            .get(i)
            .filter(|l| l.enabled)
            .map(|_| i)
    }

    pub(super) fn extern_emit(&mut self, i: usize, make: impl FnOnce(u8) -> MidiOut) {
        if let Some(ch) = self.extern_out.layers[i].channel {
            self.extern_out.queue.push(make(ch));
        }
    }

    /// Sends a transposed Note On; the sent note is remembered so Note Off always matches it.
    pub(super) fn extern_note_on(&mut self, i: usize, note: u8, velocity: u8) {
        let velocity = self.extern_velocity(i, velocity);
        let shifted = i16::from(note) + 12 * i16::from(self.extern_out.layers[i].octave_shift);
        if !(0..=127).contains(&shifted) {
            return;
        }
        let out = shifted as u8;
        self.extern_out.sent_note[i][note as usize] = out;
        self.extern_emit(i, |ch| MidiOut::note_on(ch, out, velocity));
    }

    pub(super) fn extern_note_off(&mut self, i: usize, note: u8) {
        let out = std::mem::replace(&mut self.extern_out.sent_note[i][note as usize], NO_NOTE);
        if out != NO_NOTE {
            self.extern_emit(i, |ch| MidiOut::note_off(ch, out));
        }
    }

    /// Releases every note still sounding on Extern layer `i` (use before disabling the mode).
    pub fn extern_all_notes_off(&mut self, i: usize) {
        for note in 0..128u8 {
            self.extern_note_off(i, note);
        }
    }

    /// Turns a CC VAL dial: stores `value` for `slot` (0 or 1) and transmits it.
    pub fn extern_set_cc_value(&mut self, i: usize, slot: usize, value: u8) {
        let layer = &mut self.extern_out.layers[i];
        layer.cc_values[slot] = value.min(127);
        let (cc, v) = (layer.cc_numbers[slot], layer.cc_values[slot]);
        self.extern_emit(i, |ch| MidiOut::control_change(ch, cc, v));
    }

    /// Turns the PROGRAM dial: Bank Select MSB/LSB then Program Change.
    pub fn extern_set_program(&mut self, i: usize, program: u8) {
        self.extern_out.layers[i].program = program.min(127);
        self.extern_send_program(i);
    }

    fn extern_send_program(&mut self, i: usize) {
        let l = self.extern_out.layers[i];
        self.extern_emit(i, |ch| {
            MidiOut::control_change(ch, cc::BANK_SELECT_MSB, l.bank_msb)
        });
        self.extern_emit(i, |ch| {
            MidiOut::control_change(ch, cc::BANK_SELECT_LSB, l.bank_lsb)
        });
        self.extern_emit(i, |ch| MidiOut::program_change(ch, l.program));
    }

    /// "Send On Load": transmits stored bank/program, CC values and volume for layers with it on.
    pub fn extern_send_on_load(&mut self) {
        for i in 0..3 {
            let l = self.extern_out.layers[i];
            if !l.enabled || !l.send_on_load {
                continue;
            }
            self.extern_send_program(i);
            for slot in 0..2 {
                let (cc, v) = (l.cc_numbers[slot], l.cc_values[slot]);
                self.extern_emit(i, |ch| MidiOut::control_change(ch, cc, v));
            }
            let vol = self.extern_volume(i);
            self.extern_out.last_volume[i] = vol;
            self.extern_emit(i, |ch| MidiOut::control_change(ch, cc::MASTER_LEVEL, vol));
        }
    }

    fn extern_volume(&self, i: usize) -> u8 {
        (self.layer_levels[LayerId::SynthA as usize + i]
            .base
            .clamp(0.0, 1.0)
            * 127.0)
            .round() as u8
    }

    /// Transmits CC7 when an Extern layer's fader moved (the first poll only records the value).
    pub fn extern_sync_volume(&mut self) {
        for i in 0..3 {
            if !self.extern_out.layers[i].enabled {
                self.extern_out.last_volume[i] = VOLUME_UNSET;
                continue;
            }
            let vol = self.extern_volume(i);
            let last = std::mem::replace(&mut self.extern_out.last_volume[i], vol);
            if last != VOLUME_UNSET && last != vol {
                self.extern_emit(i, |ch| MidiOut::control_change(ch, cc::MASTER_LEVEL, vol));
            }
        }
    }

    /// Forwards the sustain pedal value (`0..=127`) to Extern layer `i` when it changed.
    pub(super) fn extern_forward_sustain(&mut self, i: usize, value: u8) {
        if std::mem::replace(&mut self.extern_out.last_sustain[i], value) != value {
            self.extern_emit(i, |ch| {
                MidiOut::control_change(ch, cc::SUSTAIN_PEDAL, value)
            });
        }
    }

    /// Forwards the pitch stick (`-1.0..=1.0`) to Extern layer `i` when it changed.
    pub(super) fn extern_forward_bend(&mut self, i: usize, norm: f32) {
        let value = (8192.0 + norm.clamp(-1.0, 1.0) * 8192.0).clamp(0.0, 16383.0) as u16;
        if std::mem::replace(&mut self.extern_out.last_bend[i], value) != value {
            self.extern_emit(i, |ch| MidiOut::pitch_bend(ch, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_is_fifo_wraps_and_counts_drops() {
        let mut q = MidiOutQueue::default();
        for i in 0..MIDI_OUT_CAPACITY {
            assert!(q.push(MidiOut::note_on(0, (i % 128) as u8, 1)));
        }
        assert!(!q.push(MidiOut::note_on(0, 1, 1)));
        assert_eq!(q.dropped(), 1);
        assert_eq!(q.pop().unwrap().data1, 0);
        assert!(q.push(MidiOut::note_on(0, 99, 1)));
        assert_eq!(q.len(), MIDI_OUT_CAPACITY);
        let last = std::iter::from_fn(|| q.pop()).last().unwrap();
        assert_eq!(last.data1, 99);
        assert!(q.is_empty());
    }

    #[test]
    fn messages_encode_wire_bytes() {
        assert_eq!(MidiOut::note_on(12, 60, 100).bytes(), [0x9C, 60, 100]);
        assert_eq!(MidiOut::program_change(2, 5).len, 2);
        assert_eq!(MidiOut::pitch_bend(0, 8193).bytes(), [0xE0, 0x01, 0x40]);
    }
}

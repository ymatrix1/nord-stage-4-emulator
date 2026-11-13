//! Hardware piano level of a note computed from a parsed `.npno` header and stroke records.
//!
//! Opt-in model, not used by the default render. Evidence: `docs/research/piano-sound-load.md`
//! section 9 (runs of the real firmware with `tools/arm_piano_load.py`; every field below was
//! matched against the in-memory structures the firmware builds from the same file):
//!
//! * `keymap[note]` at `0xB8` (128 bytes, `0xFF` = unmapped) gives the root key of a note.
//! * The halfword block at `0x648` (big endian): `hw[2]` is the number of stroke records and
//!   `hw[3 + root]` the number of records of that root; records follow in ascending root order.
//! * Stroke record (118 bytes from `0x75E`): `layer = byte[4] & 3`, level offset `rec[0x5E]` =
//!   big-endian `i16` at `0x34`, list threshold = byte `0x74`.
//! * Per-key signed bytes: `b1[note]` at `0x138 + note`, `b2[note]` at `0x3B8 + note`, and one
//!   signed `b3` at `0x438`.
//! * Level: `n = rec[0x5E] + 2 X + 60 - (b1 + b2 + b3)`, amplitude [`super::level_q23`], with
//!   `X` from [`super::velocity_curve`]. The stroke is the one of the root's layer list with the
//!   largest threshold not above the velocity position `T(velocity) + bias`.
//!
//! A note-on starts one or two firmware voices ([`NpnoLevel::voices`]). The *main* voice (layer 0,
//! position `T`) always exists. Sounds of the classes in [`TWO_VOICE_CLASSES`] (file byte `0x44`:
//! 1 grand, 2 and 3 electric piano / Wurlitzer, 5 grand, 6 upright; VERIFIED on grand, upright,
//! Wurlitzer, EP; single voice on 4, 7, 14, 15, 16) add a first voice: on layer 1 at the position
//! `F` (record byte `0x33`) of the main voice's record when the root has layer-1 strokes,
//! otherwise on layer 0 at `T + 20`. Across 96 random notes on four sounds the list of voice
//! levels equals the firmware's in 96 of 96 cases (the two-stage layer-1 position was found by
//! comparing three candidates; the plain `T` position matches only 22 of 24 upright notes).
//! All factory files carry the version word `0x464`; the firmware branches for the words 2 and 5
//! are never used by them and are not modelled.

use super::velocity_curve::{velocity_position, x_from_curve};
use super::velocity_level::amplitude_q23;

const KEYMAP: usize = 0xB8;
const B1: usize = 0x138;
const B2: usize = 0x3B8;
const B3: usize = 0x438;
const COUNTS: usize = 0x648;
const RECORDS: usize = 0x75E;
const RECORD_LEN: usize = 0x76;
const LEVEL_FIELD: usize = 0x34;
const THRESHOLD_FIELD: usize = 0x74;
const POSITION_MAX: u16 = 249;
const CLASS_BYTE: usize = 0x44;
const FLOOR_FIELD: usize = 0x33;
/// Class bytes whose notes start the extra first voice (VERIFIED by run, see the module note).
pub const TWO_VOICE_CLASSES: [u8; 5] = [1, 2, 3, 5, 6];
/// Position bias of the extra voice when the root has no layer-1 strokes.
pub const EXTRA_VOICE_BIAS: u16 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stroke {
    index: u16,
    offset: i16,
    threshold: u8,
    layer: u8,
    /// Record byte `0x33`: the position the layer-1 pre-selection uses.
    floor: u8,
}

/// The (at most two) voices of a note-on, stored inline (no heap allocation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoiceList {
    items: [VoiceLevel; 2],
    len: usize,
}

impl VoiceList {
    fn push(&mut self, voice: VoiceLevel) {
        if self.len < self.items.len() {
            self.items[self.len] = voice;
            self.len += 1;
        }
    }
}

impl std::ops::Deref for VoiceList {
    type Target = [VoiceLevel];
    fn deref(&self) -> &[VoiceLevel] {
        &self.items[..self.len]
    }
}

/// One firmware voice of a note-on: the selected stroke record and its Q23 level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoiceLevel {
    /// Index of the stroke record in the file.
    pub record: u16,
    pub amplitude_q23: u32,
}

/// Level inputs of one `.npno` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpnoLevel {
    keymap: [u8; 128],
    b1: [i8; 128],
    b2: [i8; 128],
    b3: i8,
    class: u8,
    /// `(root, strokes of that root in file order)`.
    roots: Vec<(u8, Vec<Stroke>)>,
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

impl NpnoLevel {
    /// Parse the level data of an `.npno` file (at least the bytes up to the last stroke record).
    /// `None` when the file is too short or a record count does not fit.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut keymap = [0xFF; 128];
        keymap.copy_from_slice(bytes.get(KEYMAP..KEYMAP + 128)?);
        let mut b1 = [0i8; 128];
        let mut b2 = [0i8; 128];
        for note in 0..128 {
            b1[note] = *bytes.get(B1 + note)? as i8;
            b2[note] = *bytes.get(B2 + note)? as i8;
        }
        let b3 = *bytes.get(B3)? as i8;
        let class = *bytes.get(CLASS_BYTE)?;
        let total = usize::from(be16(bytes, COUNTS + 4)?);
        let mut roots: Vec<u8> = keymap.iter().copied().filter(|&r| r != 0xFF).collect();
        roots.sort_unstable();
        roots.dedup();
        let mut next = 0usize;
        let mut list = Vec::new();
        for root in roots {
            let count = usize::from(be16(bytes, COUNTS + 2 * (3 + usize::from(root)))?);
            let mut strokes = Vec::with_capacity(count);
            for index in next..next + count {
                if index >= total {
                    return None;
                }
                let at = RECORDS + RECORD_LEN * index;
                let record = bytes.get(at..at + RECORD_LEN)?;
                strokes.push(Stroke {
                    index: u16::try_from(index).ok()?,
                    floor: record[FLOOR_FIELD],
                    offset: i16::from_be_bytes([record[LEVEL_FIELD], record[LEVEL_FIELD + 1]]),
                    threshold: record[THRESHOLD_FIELD],
                    layer: record[4] & 3,
                });
            }
            next += count;
            list.push((root, strokes));
        }
        Some(Self {
            keymap,
            b1,
            b2,
            b3,
            class,
            roots: list,
        })
    }

    /// Root key of `note` (`None` when unmapped).
    pub fn root_of(&self, note: u8) -> Option<u8> {
        self.keymap
            .get(usize::from(note))
            .copied()
            .filter(|&r| r != 0xFF)
    }

    /// Root key the stroke record `record` belongs to (`None` for an unknown record).
    pub fn record_root(&self, record: u16) -> Option<u8> {
        self.roots
            .iter()
            .find(|(_, strokes)| strokes.iter().any(|s| s.index == record))
            .map(|(root, _)| *root)
    }

    /// `b1 + b2 + b3` of `note`.
    pub fn key_sum(&self, note: u8) -> i16 {
        let n = usize::from(note.min(127));
        i16::from(self.b1[n]) + i16::from(self.b2[n]) + i16::from(self.b3)
    }

    fn pick(&self, note: u8, layer: u8, position: u16) -> Option<&Stroke> {
        let root = self.root_of(note)?;
        let strokes = &self.roots.iter().find(|(r, _)| *r == root)?.1;
        let mut chosen: Option<&Stroke> = None;
        for stroke in strokes.iter().filter(|s| s.layer == layer) {
            if chosen.is_none() || u16::from(stroke.threshold) <= position {
                chosen = Some(stroke);
            }
        }
        chosen
    }

    fn has_layer(&self, note: u8, layer: u8) -> bool {
        self.root_of(note)
            .and_then(|root| self.roots.iter().find(|(r, _)| *r == root))
            .is_some_and(|(_, strokes)| strokes.iter().any(|s| s.layer == layer))
    }

    fn level(&self, note: u8, velocity: u8, stroke: &Stroke) -> VoiceLevel {
        VoiceLevel {
            record: stroke.index,
            amplitude_q23: amplitude_q23(stroke.offset, x_from_curve(velocity), self.key_sum(note)),
        }
    }

    /// `rec[0x5E]` of the stroke of `note` on `layer` selected at velocity position `position`.
    pub fn stroke_offset(&self, note: u8, layer: u8, position: u16) -> Option<i16> {
        self.pick(note, layer, position).map(|s| s.offset)
    }

    /// Q23 amplitude of one voice: `layer` list, velocity position `T(velocity) + bias`.
    pub fn amplitude_q23(&self, note: u8, velocity: u8, layer: u8, bias: u16) -> Option<u32> {
        let position = (u16::from(velocity_position(velocity)) + bias).min(POSITION_MAX);
        let stroke = self.pick(note, layer, position)?;
        Some(self.level(note, velocity, stroke).amplitude_q23)
    }

    /// The voices a note-on starts, extra first voice (if any) then the main voice; empty when
    /// the note is unmapped or the root has no layer-0 stroke.
    pub fn voices(&self, note: u8, velocity: u8) -> VoiceList {
        let velocity = velocity.clamp(1, 127);
        let t = u16::from(velocity_position(velocity));
        let Some(main) = self.pick(note, 0, t) else {
            return VoiceList::default();
        };
        let mut out = VoiceList::default();
        if TWO_VOICE_CLASSES.contains(&self.class) {
            let extra = if self.has_layer(note, 1) {
                self.pick(note, 1, u16::from(main.floor))
            } else {
                self.pick(note, 0, (t + EXTRA_VOICE_BIAS).min(POSITION_MAX))
            };
            if let Some(extra) = extra {
                out.push(self.level(note, velocity, extra));
            }
        }
        out.push(self.level(note, velocity, main));
        out
    }

    /// Q23 amplitude of the main voice (layer 0, no bias).
    pub fn main_voice_q23(&self, note: u8, velocity: u8) -> Option<u32> {
        self.amplitude_q23(note, velocity, 0, 0)
    }
}

#[cfg(test)]
/// Image with one root (`root`, mapped from note 60) and the given `(layer, offset, threshold)`
/// strokes, per-key sum 0 except `b1[60] = key_sum`.
pub(crate) fn test_image(root: u8, strokes: &[(u8, i16, u8)], key_sum: i8) -> Vec<u8> {
    let mut d = vec![0u8; RECORDS + RECORD_LEN * strokes.len().max(1)];
    d[KEYMAP..KEYMAP + 128].fill(0xFF);
    d[KEYMAP + 60] = root;
    d[B1 + 60] = key_sum as u8;
    d[COUNTS + 4..COUNTS + 6].copy_from_slice(&(strokes.len() as u16).to_be_bytes());
    let at = COUNTS + 2 * (3 + usize::from(root));
    d[at..at + 2].copy_from_slice(&(strokes.len() as u16).to_be_bytes());
    for (i, &(layer, offset, threshold)) in strokes.iter().enumerate() {
        let r = RECORDS + RECORD_LEN * i;
        d[r + 4] = layer;
        d[r + LEVEL_FIELD..r + LEVEL_FIELD + 2].copy_from_slice(&offset.to_be_bytes());
        d[r + THRESHOLD_FIELD] = threshold;
    }
    d
}

#[cfg(test)]
#[path = "npno_level_tests.rs"]
mod tests;

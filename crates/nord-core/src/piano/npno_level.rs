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

/// One firmware voice of a note-on: the selected stroke record and its Q23 level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub fn voices(&self, note: u8, velocity: u8) -> Vec<VoiceLevel> {
        let velocity = velocity.clamp(1, 127);
        let t = u16::from(velocity_position(velocity));
        let Some(main) = self.pick(note, 0, t) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(2);
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
mod tests {
    use super::super::velocity_level::{
        note_amplitude_q23, stroke_at, AMBER_UPRIGHT_STROKES, BRIGHT_GRAND_STROKES,
        CLAVINET_D6_STROKES, X_SWEEP,
    };
    use super::*;

    // Root-60 lists of the swept sounds: `(rec[0x5E], threshold)` of the layer in use.
    const CLAVINET_LIST: [(i16, u8); 3] = [(66, 0), (56, 16), (36, 46)];
    const GRAND_LIST: [(i16, u8); 8] = [
        (35, 0),
        (34, 10),
        (26, 29),
        (19, 47),
        (8, 62),
        (-2, 78),
        (-1, 92),
        (-10, 107),
    ];
    const UPRIGHT_LIST: [(i16, u8); 9] = [
        (43, 0),
        (44, 6),
        (36, 19),
        (36, 32),
        (28, 45),
        (18, 61),
        (8, 76),
        (8, 90),
        (-1, 106),
    ];

    /// Velocities swept on the grand and the upright: 1, 5, 9, ... and 127.
    fn swept_every_fourth() -> impl Iterator<Item = u8> {
        (1..=127u8).step_by(4).chain([127])
    }

    fn level(layer: u8, list: &[(i16, u8)], key_sum: i8) -> NpnoLevel {
        let strokes: Vec<(u8, i16, u8)> = list.iter().map(|&(o, t)| (layer, o, t)).collect();
        NpnoLevel::parse(&test_image(60, &strokes, key_sum)).expect("synthetic image parses")
    }

    #[test]
    fn clavinet_main_voice_matches_every_swept_amplitude() {
        let model = level(0, &CLAVINET_LIST, 40);
        for &(velocity, _) in &X_SWEEP {
            let want = note_amplitude_q23(&CLAVINET_D6_STROKES, &[(60, 40)], velocity, 60);
            assert_eq!(
                model.main_voice_q23(60, velocity),
                Some(want),
                "velocity {velocity}"
            );
        }
    }

    #[test]
    fn grand_first_voice_with_bias_20_matches_every_swept_amplitude() {
        let model = level(0, &GRAND_LIST, 49);
        for velocity in swept_every_fourth() {
            let want = note_amplitude_q23(&BRIGHT_GRAND_STROKES, &[(60, 49)], velocity, 60);
            assert_eq!(
                model.amplitude_q23(60, velocity, 0, 20),
                Some(want),
                "velocity {velocity}"
            );
        }
    }

    #[test]
    fn upright_first_voice_on_layer_1_matches_every_swept_amplitude() {
        let model = level(1, &UPRIGHT_LIST, 54);
        for velocity in swept_every_fourth() {
            let want = note_amplitude_q23(&AMBER_UPRIGHT_STROKES, &[(60, 54)], velocity, 60);
            assert_eq!(
                model.amplitude_q23(60, velocity, 1, 0),
                Some(want),
                "velocity {velocity}"
            );
        }
    }

    #[test]
    fn stroke_choice_agrees_with_the_swept_step_tables() {
        let model = level(0, &CLAVINET_LIST, 40);
        for &(velocity, _) in &X_SWEEP {
            let position = u16::from(velocity_position(velocity));
            assert_eq!(
                model.stroke_offset(60, 0, position),
                Some(stroke_at(&CLAVINET_D6_STROKES, velocity))
            );
        }
    }

    #[test]
    fn unmapped_note_and_missing_layer_give_none() {
        let model = level(0, &CLAVINET_LIST, 40);
        assert_eq!(model.main_voice_q23(61, 64), None, "note 61 is unmapped");
        assert_eq!(
            model.amplitude_q23(60, 64, 2, 0),
            None,
            "no layer 2 strokes"
        );
        assert_eq!(model.root_of(60), Some(60));
        assert_eq!(model.key_sum(60), 40);
    }

    #[test]
    fn parse_rejects_short_and_inconsistent_files() {
        assert!(NpnoLevel::parse(&[0u8; 100]).is_none());
        let mut d = test_image(60, &[(0, 1, 0)], 0);
        // Claim two strokes for the root while the header total says one.
        let at = COUNTS + 2 * (3 + 60);
        d[at..at + 2].copy_from_slice(&2u16.to_be_bytes());
        assert!(NpnoLevel::parse(&d).is_none());
    }

    #[test]
    fn two_voice_class_adds_the_biased_first_voice_on_layer_0() {
        let strokes: Vec<(u8, i16, u8)> = GRAND_LIST.iter().map(|&(o, t)| (0, o, t)).collect();
        let mut image = test_image(60, &strokes, 49);
        image[CLASS_BYTE] = 5;
        let model = NpnoLevel::parse(&image).unwrap();
        let voices = model.voices(60, 49);
        assert_eq!(voices.len(), 2);
        assert_eq!(
            Some(voices[0].amplitude_q23),
            model.amplitude_q23(60, 49, 0, 20)
        );
        assert_eq!(Some(voices[1].amplitude_q23), model.main_voice_q23(60, 49));
        image[CLASS_BYTE] = 4;
        let single = NpnoLevel::parse(&image).unwrap().voices(60, 49);
        assert_eq!(single.len(), 1, "class 4 has one voice");
        assert_eq!(single[0], voices[1]);
    }

    #[test]
    fn layer_1_voice_uses_the_floor_byte_of_the_main_record() {
        // Layer 0: offsets 10 / 20 / 30 at thresholds 0 / 20 / 40, floor bytes 0 / 25 / 50.
        // Layer 1: offsets 1 / 2 / 3 at thresholds 0 / 30 / 60.
        let mut image = test_image(
            60,
            &[
                (0, 10, 0),
                (0, 20, 20),
                (0, 30, 40),
                (1, 1, 0),
                (1, 2, 30),
                (1, 3, 60),
            ],
            0,
        );
        image[CLASS_BYTE] = 6;
        for (i, floor) in [0u8, 25, 50, 0, 0, 0].into_iter().enumerate() {
            image[RECORDS + RECORD_LEN * i + FLOOR_FIELD] = floor;
        }
        // The count header lists six records for root 60 (test_image sets it).
        let model = NpnoLevel::parse(&image).unwrap();
        // Velocity 64: T = 78, main = third layer-0 record (floor 50), layer 1 at position 50 -> offset 2.
        let voices = model.voices(60, 64);
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].record, 4, "layer-1 record index");
        assert_eq!(voices[1].record, 2);
    }

    #[test]
    fn key_sum_adds_the_three_signed_bytes() {
        let mut d = test_image(60, &[(0, 1, 0)], -4);
        d[B2 + 60] = 3;
        d[B3] = 49;
        let model = NpnoLevel::parse(&d).unwrap();
        assert_eq!(model.key_sum(60), 48);
    }
}

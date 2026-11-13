//! Tests of `npno_level` (goldens of the swept sounds and the SMP record gains).
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

/// `(class, key sum at note 60, root, strokes (layer, rec[0x5E], threshold, floor), firmware
/// voice gains `level_q23 / 2^23` read from the SMP voice record at velocity 30 and 90)`.
type Sound = (
    u8,
    i16,
    u8,
    &'static [(u8, i16, u8, u8)],
    [&'static [u32]; 2],
);

// Clavinet
const CLAVINET: Sound = (
    4,
    40,
    60,
    &[(0, 66, 0, 0), (0, 56, 16, 31), (0, 36, 46, 60)],
    [&[301124], &[1589344]],
);
// Bright
const BRIGHT: Sound = (
    5,
    49,
    60,
    &[
        (0, 35, 0, 0),
        (0, 34, 10, 20),
        (0, 26, 29, 40),
        (0, 19, 47, 55),
        (0, 8, 62, 69),
        (0, -2, 78, 84),
        (0, -1, 92, 100),
        (0, -10, 107, 113),
        (2, -34, 7, 65),
        (2, -44, 27, 85),
        (2, -52, 49, 107),
        (2, -63, 69, 123),
        (2, -71, 90, 151),
        (2, -81, 110, 167),
    ],
    [&[568447, 568447], &[2704005, 2493948]],
);
// Amber
const AMBER: Sound = (
    6,
    54,
    59,
    &[
        (0, 43, 0, 0),
        (0, 46, 7, 12),
        (0, 36, 20, 25),
        (0, 36, 32, 38),
        (0, 27, 45, 52),
        (0, 17, 60, 69),
        (0, 8, 76, 83),
        (0, 8, 91, 97),
        (0, -1, 106, 115),
        (1, 43, 0, 0),
        (1, 44, 6, 13),
        (1, 36, 19, 26),
        (1, 36, 32, 38),
        (1, 28, 45, 52),
        (1, 18, 61, 68),
        (1, 8, 76, 84),
        (1, 8, 90, 98),
        (1, -1, 106, 113),
        (2, -32, 9, 64),
        (2, -43, 28, 83),
        (2, -51, 51, 103),
        (2, -60, 75, 127),
        (2, -70, 98, 146),
    ],
    [&[542777, 542777], &[2353973, 2353973]],
);
// Wurlitzer
const WURLITZER: Sound = (
    3,
    30,
    59,
    &[
        (0, 14, 0, 0),
        (0, 15, 5, 10),
        (0, 21, 12, 14),
        (0, 13, 18, 21),
        (0, 15, 24, 26),
        (0, 17, 28, 30),
        (0, 16, 33, 35),
        (0, 18, 37, 39),
        (0, 9, 42, 44),
        (0, 10, 47, 50),
        (0, 11, 54, 58),
        (0, 2, 63, 68),
        (0, -6, 79, 90),
        (2, -26, 7, 57),
        (2, -33, 23, 66),
        (2, -32, 38, 75),
        (2, -32, 44, 78),
        (2, -41, 54, 94),
        (2, -60, 75, 131),
    ],
    [&[435808, 435808], &[2381325, 2196335]],
);

/// The SMP voice records of the real firmware (DM `0x9251E` extra voice, `0x9252A` main voice)
/// hold exactly the gains the model predicts: 12 voices on four sounds at two velocities.
#[test]
fn voice_gains_equal_the_smp_record_floats() {
    for (class, key_sum, root, strokes, wants) in [CLAVINET, BRIGHT, AMBER, WURLITZER] {
        let list: Vec<(u8, i16, u8)> = strokes.iter().map(|&(l, o, t, _)| (l, o, t)).collect();
        let mut image = test_image(root, &list, key_sum as i8);
        image[KEYMAP + 60] = root;
        image[CLASS_BYTE] = class;
        for (i, &(_, _, _, floor)) in strokes.iter().enumerate() {
            image[RECORDS + RECORD_LEN * i + FLOOR_FIELD] = floor;
        }
        let model = NpnoLevel::parse(&image).unwrap();
        for (velocity, want) in [30u8, 90].into_iter().zip(wants) {
            let got: Vec<u32> = model
                .voices(60, velocity)
                .iter()
                .map(|v| v.amplitude_q23)
                .collect();
            assert_eq!(got, want, "class {class} velocity {velocity}");
        }
    }
}

#[test]
fn key_sum_adds_the_three_signed_bytes() {
    let mut d = test_image(60, &[(0, 1, 0)], -4);
    d[B2 + 60] = 3;
    d[B3] = 49;
    let model = NpnoLevel::parse(&d).unwrap();
    assert_eq!(model.key_sum(60), 48);
}

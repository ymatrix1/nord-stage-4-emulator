//! The voice gains of the model equal the floats of the real firmware's SMP voice records.

use super::*;

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

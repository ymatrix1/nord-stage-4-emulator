//! B3 key-down tables of the PNO core 1 generator/processor path: the per-wheel ring amplitude
//! `A` and the per-key footage weights `W`.
//!
//! Spec: `docs/specs/organ-engine.md` section 3.1.6. Research: `docs/research/keydown-zones.md`
//! section 6.2 (captured B3 key-down messages, 24 keys 21..108), derived tables
//! `docs/research/data/keydown-b3-wheels.csv` and `b3-key-weights.csv`
//! (`tools/organ_b3_keys.py`). A partial of a held key sounds with
//! `A[wheel] * W(key, footage) * wheel table`; neither carries a velocity (VERIFIED negative,
//! `keydown-zones.md` section 4).

/// Per-note ring amplitude `A` written by the ARM key-down at `DM 0x95122 + (note - 36)`, notes
/// 36..115 (wheel `w` = note `w + 24`; the key-down reaches wheels 12..88).
///
/// VERIFIED for the 44 wheels of [`A_CAPTURED_WHEELS`]: the first write of the key-down burst, a
/// constant per wheel over 24 captured keys (`docs/research/data/keydown-b3-wheels.csv`,
/// `docs/research/keydown-zones.md` section 6.2; `tests::gain_table_matches_the_captured_first_writes`),
/// rounded to 4 digits. EMPIRICAL elsewhere (89..91 and the wheels no captured key reaches): the
/// earlier median of oracle fits with gaps interpolated. Wheels that several footages fold onto
/// get lower values on later writes (wheel 12: 0.5556, 0.5328, 0.5179); the first write is kept.
pub const WHEEL_GAIN: [f32; 80] = [
    0.5555, 0.5192, 0.4802, 0.4829, 0.4546, 0.4646, 0.4344, 0.4098, 0.4075, 0.3853, 0.3902, 0.4061,
    0.3755, 0.3758, 0.3680, 0.3569, 0.3758, 0.3632, 0.3630, 0.3406, 0.3502, 0.3451, 0.3465, 0.3439,
    0.2940, 0.2824, 0.2904, 0.3298, 0.2879, 0.4095, 0.3988, 0.3425, 0.3510, 0.3498, 0.3845, 0.3739,
    0.3704, 0.3805, 0.3901, 0.3998, 0.3998, 0.4093, 0.3842, 0.4501, 0.4534, 0.4687, 0.4879, 0.4698,
    0.5130, 0.5015, 0.5148, 0.5059, 0.5465, 0.4932, 0.5497, 0.6399, 0.5505, 0.5713, 0.5731, 0.5834,
    0.4720, 0.7005, 0.5863, 0.7624, 0.6681, 0.5948, 0.4731, 0.6896, 0.6951, 0.7180, 0.7569, 0.7214,
    0.6293, 0.6429, 0.5926, 0.6336, 0.6747, 0.6747, 0.6747, 0.6747,
];

/// Wheels (`note - 24`) whose [`WHEEL_GAIN`] entry is a captured ARM write (VERIFIED, see there).
pub const A_CAPTURED_WHEELS: [u8; 44] = [
    12, 16, 19, 21, 23, 24, 25, 28, 31, 32, 33, 35, 36, 37, 40, 43, 44, 45, 47, 48, 49, 52, 55, 56,
    57, 59, 60, 61, 64, 65, 67, 68, 69, 71, 72, 73, 76, 79, 80, 81, 83, 84, 85, 88,
];

/// Fold a wheel index (`note - 24`) into 12..88 by octaves: the ring notes the key-down reaches.
///
/// VERIFIED: the multiset of written wheels of all 24 captured keys (`keydown-zones.md` s6.2;
/// `tools/organ_b3_keys.py` checks the word index of every captured weight write).
pub(super) fn fold_wheel(wheel: i32) -> usize {
    let mut w = wheel;
    while w < 12 {
        w += 12;
    }
    while w > 88 {
        w -= 12;
    }
    w as usize
}

/// Keys of the capture (VERIFIED): the notes of the [`KEY_W`] rows.
pub(super) const KEY_NOTES: [u8; 24] = [
    21, 24, 28, 33, 36, 40, 45, 48, 52, 57, 60, 61, 64, 69, 72, 76, 81, 84, 88, 93, 96, 100, 105,
    108,
];

/// Footage weights `W` of the captured keys, columns `[16', 5 1/3', 8', 4']`: the float the ARM
/// sends to `DM 0x951F1 + (partial note - 36)` per footage item at the drawbar level of the
/// default program, as `d7 * taper` (`docs/research/b3-generator.md` section 4). Cumulative writes
/// to one word (several footages folded onto one wheel) are split into per-footage increments.
///
/// VERIFIED: every entry is a captured write (`tools/organ_b3_keys.py check` against
/// `docs/research/data/keydown-organ-keys.csv`, `tests::key_table_matches_the_derived_csv`).
pub(super) const KEY_W: [[f32; 4]; 24] = [
    [0.008963, 0.020832, 0.015987, 0.021801],
    [0.008963, 0.020832, 0.015987, 0.021801],
    [0.008963, 0.020832, 0.015987, 0.021801],
    [0.008963, 0.020832, 0.015987, 0.021801],
    [0.008963, 0.020832, 0.015987, 0.021801],
    [0.009689, 0.020348, 0.015745, 0.020105],
    [0.011385, 0.020590, 0.015745, 0.022528],
    [0.013565, 0.022285, 0.015745, 0.023497],
    [0.022770, 0.032944, 0.022285, 0.034882],
    [0.025434, 0.034882, 0.023254, 0.033670],
    [0.031733, 0.036577, 0.032944, 0.032217],
    [0.030764, 0.033913, 0.034882, 0.030764],
    [0.031248, 0.033186, 0.035124, 0.032217],
    [0.032217, 0.031975, 0.034397, 0.030764],
    [0.044086, 0.030279, 0.032459, 0.031975],
    [0.048447, 0.038757, 0.039484, 0.020832],
    [0.047235, 0.035124, 0.038273, 0.022770],
    [0.054745, 0.039484, 0.039968, 0.023981],
    [0.053049, 0.053049, 0.046993, 0.020348],
    [0.051838, 0.049658, 0.051111, 0.020590],
    [0.057167, 0.055229, 0.048689, 0.021074],
    [0.057167, 0.055229, 0.048689, 0.021074],
    [0.057167, 0.055229, 0.048689, 0.021074],
    [0.057167, 0.055229, 0.048689, 0.021074],
];

/// Column of [`KEY_W`] for footage `j` (0 = 16', 1 = 5 1/3', 2 = 8', 3 = 4', ...): the higher
/// footages (2 2/3' and up) reuse the 4' column.
///
/// ASSUMED: the taper bytes of the other five footages were not in the capture (the default
/// program has only these four items); the 4' column is the nearest captured footage. Confirm with
/// a key-down capture at drawbars `888888888`.
pub(super) const fn footage_column(j: usize) -> usize {
    match j {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => 3,
    }
}

/// Footage weights `[16', 5 1/3', 8', 4']` of any key: the captured rows, linear in the note
/// between neighbours and flat beyond the first and last captured key.
///
/// EMPIRICAL between the 24 captured keys (VERIFIED at them); the taper steps of the ARM table are
/// not resolved by 24 samples, so the interpolation error is not bounded by the capture.
pub(super) fn key_footage_weights(key: i32) -> [f32; 4] {
    let k = key.clamp(i32::from(KEY_NOTES[0]), i32::from(KEY_NOTES[23]));
    let hi = KEY_NOTES
        .iter()
        .position(|&n| i32::from(n) >= k)
        .unwrap_or(KEY_NOTES.len() - 1);
    if i32::from(KEY_NOTES[hi]) == k || hi == 0 {
        return KEY_W[hi];
    }
    let (n0, n1) = (f32::from(KEY_NOTES[hi - 1]), f32::from(KEY_NOTES[hi]));
    let t = (k as f32 - n0) / (n1 - n0);
    let mut out = [0.0f32; 4];
    for ((o, &a), &b) in out.iter_mut().zip(&KEY_W[hi - 1]).zip(&KEY_W[hi]) {
        *o = a + t * (b - a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv_rows(text: &str) -> Vec<Vec<&str>> {
        text.lines()
            .skip(1)
            .map(|l| l.split(',').collect())
            .collect()
    }

    #[test]
    fn gain_table_matches_the_captured_first_writes() {
        let rows = csv_rows(include_str!(
            "../../../../docs/research/data/keydown-b3-wheels.csv"
        ));
        for r in &rows {
            let wheel: usize = r[0].parse().unwrap();
            let want: f32 = r[3].parse().unwrap();
            assert!(A_CAPTURED_WHEELS.contains(&(wheel as u8)), "wheel {wheel}");
            assert!(
                (WHEEL_GAIN[wheel - 12] - want).abs() < 6e-5,
                "wheel {wheel}"
            );
        }
        assert_eq!(rows.len(), A_CAPTURED_WHEELS.len());
    }

    #[test]
    fn gain_table_spot_values_follow_the_arm_key_down_floats() {
        // DM 0x9513A (note 60) 0x3E9689D0 = 0.2939 -> 0.2940; 0x95152 (note 84) 0.5129
        assert!((WHEEL_GAIN[60 - 36] - 0.294).abs() < 1e-3);
        assert!((WHEEL_GAIN[84 - 36] - 0.5129).abs() < 2e-3);
        assert!((WHEEL_GAIN[67 - 36] - 0.3427).abs() < 3e-3);
    }

    #[test]
    fn key_table_matches_the_derived_csv() {
        let rows = csv_rows(include_str!(
            "../../../../docs/research/data/b3-key-weights.csv"
        ));
        assert_eq!(rows.len(), KEY_NOTES.len());
        for (r, (&note, w)) in rows.iter().zip(KEY_NOTES.iter().zip(KEY_W.iter())) {
            assert_eq!(r[0].parse::<u8>().unwrap(), note);
            for (c, &v) in w.iter().enumerate() {
                let want: f32 = r[1 + c].parse().unwrap();
                assert!((v - want).abs() < 1e-6, "note {note} column {c}");
            }
        }
    }

    #[test]
    fn interpolation_is_exact_at_captured_keys_flat_outside_and_linear_between() {
        for (&n, w) in KEY_NOTES.iter().zip(KEY_W.iter()) {
            assert_eq!(&key_footage_weights(i32::from(n)), w);
        }
        assert_eq!(key_footage_weights(0), KEY_W[0]);
        assert_eq!(key_footage_weights(127), KEY_W[23]);
        let mid = key_footage_weights(58); // 1/3 of the way from 57 to 60
        for (c, &m) in mid.iter().enumerate() {
            let (a, b) = (KEY_W[9][c], KEY_W[10][c]);
            assert!((m - (a + (b - a) / 3.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn fold_reaches_wheels_12_to_88_by_octaves() {
        assert_eq!(fold_wheel(-12), 12);
        assert_eq!(fold_wheel(0), 12);
        assert_eq!(fold_wheel(11), 23);
        assert_eq!(fold_wheel(36), 36);
        assert_eq!(fold_wheel(93), 81);
        assert_eq!(fold_wheel(100), 88);
        assert_eq!(footage_column(0), 0);
        assert_eq!(footage_column(8), 3);
    }
}

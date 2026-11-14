//! Firmware key ranges of the layer zone codes (`Edt_Org / Pno / Syn_kKbZone`).
//!
//! Spec: `docs/specs/morph-and-zones.md` section 6, evidence: `docs/research/zone-key-ranges.md`.
//!
//! The firmware turns a layer's 4-bit zone code and the split state into two bounds (lower key and
//! upper key, each with a crossfade width setting) in `0xC01B0DC4` / `0xC01B0C40`, hands them to the
//! layer through the range setters (`0xC00C843C`) and scales every key by the smaller of the two
//! bound gains (`0xC00C84B8`, `0xC00C859C`). Everything below is a port checked against the ARM
//! functions by `tools/arm_zone_map.py --check` (0 mismatches) and against the zone objects of all
//! 384 factory programs by `tools/arm_zone_programs.py` (0 mismatches). [`digest`] is the same
//! FNV-1a stream as `arm_zone_map.digest()`.

/// Split state resolved to the keys of one layer: lower bound `(key, fade setting)` and upper bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneRange {
    /// First key of the zone and its crossfade width setting (`0` off, `1` 6 keys, `2` 12 keys).
    pub lower: (u8, u8),
    /// Last key of the zone and its crossfade width setting.
    pub upper: (u8, u8),
}

/// VERIFIED: bytes of the firmware case table `0xC02CB230`, `[mask][code][sel]`; `mask` bit `i` =
/// split point `i` (Low, Mid, High) enabled and flagged. The two `sel` columns differ only for code
/// 11 (`sel == 1`: case 10, an empty range). `tools/arm_zone_map.py --check` reads it back from the
/// firmware image.
#[rustfmt::skip]
pub const CASE_TABLE: [[[u8; 2]; 12]; 8] = [
    [[9, 9], [9, 9], [9, 9], [9, 9], [9, 9], [9, 9], [9, 9], [9, 9], [9, 9], [9, 9], [9, 9], [9, 10]],
    [[0, 0], [0, 0], [0, 0], [0, 0], [0, 0], [9, 9], [8, 8], [8, 8], [8, 8], [8, 8], [8, 8], [8, 10]],
    [[4, 4], [4, 4], [4, 4], [4, 4], [4, 4], [9, 9], [6, 6], [6, 6], [6, 6], [6, 6], [6, 6], [6, 10]],
    [[0, 0], [1, 1], [4, 4], [4, 4], [4, 4], [9, 9], [8, 8], [8, 8], [8, 8], [1, 1], [6, 6], [6, 10]],
    [[7, 7], [7, 7], [7, 7], [7, 7], [7, 7], [9, 9], [3, 3], [3, 3], [3, 3], [3, 3], [3, 3], [3, 10]],
    [[0, 0], [5, 5], [7, 7], [7, 7], [7, 7], [9, 9], [8, 8], [8, 8], [8, 8], [5, 5], [3, 3], [3, 10]],
    [[4, 4], [2, 2], [7, 7], [7, 7], [7, 7], [9, 9], [6, 6], [6, 6], [6, 6], [2, 2], [3, 3], [3, 10]],
    [[0, 0], [1, 1], [4, 4], [5, 5], [7, 7], [9, 9], [8, 8], [5, 5], [6, 6], [2, 2], [3, 3], [3, 10]],
];

/// Bound kind of a case: `Edge` = lowest / highest key, `Point(k)` = split point `k`, `Empty`.
#[derive(Clone, Copy)]
enum Kind {
    Edge,
    Point(usize),
    Empty,
}

/// VERIFIED (jump tables `0xC01B0E20` lower, `0xC01B0C9C` upper): bound kind per case 0..=10.
#[rustfmt::skip]
const LOWER: [Kind; 11] = [
    Kind::Edge, Kind::Point(0), Kind::Point(1), Kind::Point(2), Kind::Edge, Kind::Point(0), Kind::Point(1), Kind::Edge, Kind::Point(0), Kind::Edge, Kind::Empty,
];
#[rustfmt::skip]
const UPPER: [Kind; 11] = [
    Kind::Point(0), Kind::Point(1), Kind::Point(2), Kind::Edge, Kind::Point(1), Kind::Point(2), Kind::Edge, Kind::Point(2), Kind::Edge, Kind::Edge, Kind::Empty,
];

/// VERIFIED (ARM data `0xC02D8370`, 11 words): split key per position setting `0..=10`.
pub const SPLIT_NOTES: [u8; 11] = [36, 41, 48, 53, 60, 65, 72, 77, 84, 89, 96];
/// VERIFIED (ARM data `0xC02D839C`): crossfade half width in keys per width setting.
pub const FADE_KEYS: [u8; 3] = [0, 6, 12];
/// VERIFIED (split state object `+0xEC` / `+0xF0`): lowest and highest key of the keybed.
pub const LOWEST_KEY: u8 = 21;
pub const HIGHEST_KEY: u8 = 108;
/// Full level of the 16.16 gains.
pub const UNITY: u32 = 0x10000;
/// VERIFIED (ARM data `0xC0569018`): crossfade curve of width setting 1 (16.16 gains).
#[rustfmt::skip]
pub const FADE_12: [u32; 12] = [
    0x72B, 0xC8B, 0x15C2, 0x24DD, 0x3BE7, 0x5B22, 0x8000, 0xB810, 0xDE35, 0xF1A9, 0xFA1C, 0xFDB2,
];
/// VERIFIED (ARM data `0xC0569048`): crossfade curve of width setting 2.
#[rustfmt::skip]
pub const FADE_24: [u32; 24] = [
    0x560, 0x76C, 0x9FB, 0xD91, 0x126E, 0x1893, 0x20C4, 0x2B02, 0x37CE, 0x46E9, 0x5851, 0x6BC6, 0x8000, 0x9FBE, 0xBBE7, 0xD22D, 0xE24D, 0xED4F, 0xF439, 0xF8D4, 0xFBA5, 0xFD70, 0xFE76, 0xFEF9,
];

/// Position setting of split point `k` limited by its active neighbours (port of `0xC01B0A44`,
/// exhaustively equal to the ARM function over 8 flag sets x 1331 positions x 3 points).
pub fn split_index(flags: [bool; 3], pos: [u8; 3], k: usize) -> u8 {
    // active points in order (no allocation: this runs on the note-on path)
    let mut active = [0usize; 3];
    let mut n = 0;
    for (i, _) in flags.iter().enumerate().filter(|(_, f)| **f) {
        active[n] = i;
        n += 1;
    }
    let p = |i: usize| i32::from(pos[i]);
    let (mut lo, mut hi) = (0i32, 10i32);
    if n >= 2 && flags[k] {
        if n == 2 {
            let (a, b) = (active[0], active[1]);
            if k == a {
                hi = (p(b) - 1).max(0);
            } else {
                lo = (p(a) + 1).min(10);
            }
        } else if k == 0 {
            hi = if p(1) - 1 < 0 { 0 } else { (p(1) - 1).min(8) };
        } else if k == 1 {
            lo = if p(0) < 0 { 1 } else { (p(0) + 1).min(9) };
            hi = if p(2) - 1 <= 0 { 1 } else { (p(2) - 1).min(9) };
        } else {
            lo = if p(1) < 1 { 2 } else { (p(1) + 1).min(10) };
        }
    }
    let ip = p(k);
    (if ip < lo { lo } else { ip.min(hi) }) as u8
}

/// Nearest position setting of a split key (inverse of [`SPLIT_NOTES`] for the table keys).
pub fn split_setting_of(note: u8) -> u8 {
    (0..SPLIT_NOTES.len())
        .min_by_key(|&i| SPLIT_NOTES[i].abs_diff(note))
        .unwrap_or(4) as u8
}

fn bound(kind: Kind, upper: bool, flags: [bool; 3], pos: [u8; 3], widths: [u8; 3]) -> (u8, u8) {
    match kind {
        Kind::Edge => (if upper { HIGHEST_KEY } else { LOWEST_KEY }, 0),
        Kind::Empty => (u8::from(upper), 0),
        Kind::Point(k) => {
            let key = SPLIT_NOTES[usize::from(split_index(flags, pos, k))] - u8::from(upper);
            (key, widths[k].min(2))
        }
    }
}

/// Key range of a layer: the setter arguments of `0xC010ABA4`. `active[i]` = split enabled and
/// point `i` flagged, `pos` the position settings `0..=10`, `widths` the width settings `0..=2`,
/// `sel` the `[store + 0x80]` selector (0 on every layer store in the harness).
pub fn zone_range(
    code: u8,
    active: [bool; 3],
    pos: [u8; 3],
    widths: [u8; 3],
    sel: u8,
) -> ZoneRange {
    let mask = usize::from(active[0]) | usize::from(active[1]) << 1 | usize::from(active[2]) << 2;
    let case = usize::from(CASE_TABLE[mask][usize::from(code.min(11))][usize::from(sel == 1)]);
    ZoneRange {
        lower: bound(LOWER[case], false, active, pos, widths),
        upper: bound(UPPER[case], true, active, pos, widths),
    }
}

fn fade(setting: u8, index: i32) -> u32 {
    match setting {
        0 => UNITY,
        1 => FADE_12[index as usize],
        _ => FADE_24[index as usize],
    }
}

/// 16.16 gain of one bound at `note` (port of `0xC00C84B8`).
fn bound_gain(b: (u8, u8), upper: bool, note: u8) -> u32 {
    let (key, setting) = (i32::from(b.0), b.1);
    let half = i32::from(FADE_KEYS[usize::from(setting)]);
    let n = i32::from(note);
    if half == 0 {
        let inside = if upper { n <= key } else { n >= key };
        return if inside { UNITY } else { 0 };
    }
    if upper {
        if n < key + 1 - half {
            UNITY
        } else if n > key + half {
            0
        } else {
            fade(setting, key + half - n)
        }
    } else if n < key - half {
        0
    } else if n > key + half - 1 {
        UNITY
    } else {
        fade(setting, n - (key - half))
    }
}

impl ZoneRange {
    /// 16.16 gain handed to the layer key object: the smaller bound gain, 0 outside the range
    /// widened by the fades (`0xC00C859C`).
    pub fn gain_16_16(&self, note: u8) -> u32 {
        let lo = i32::from(self.lower.0) - i32::from(FADE_KEYS[usize::from(self.lower.1)]);
        let hi = i32::from(self.upper.0) + i32::from(FADE_KEYS[usize::from(self.upper.1)]);
        if i32::from(note) < lo || i32::from(note) > hi {
            return 0;
        }
        bound_gain(self.lower, false, note).min(bound_gain(self.upper, true, note))
    }

    /// [`gain_16_16`](Self::gain_16_16) as `0.0..=1.0`.
    pub fn gain(&self, note: u8) -> f32 {
        self.gain_16_16(note) as f32 / UNITY as f32
    }
}

/// FNV-1a 64 stream pinned against `tools/arm_zone_map.py` `digest()`.
pub fn digest() -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    let mut eat = |v: u32| h = (h ^ u64::from(v)).wrapping_mul(0x0100_0000_01B3);
    for row in CASE_TABLE {
        for pair in row {
            eat(u32::from(pair[0]) * 16 + u32::from(pair[1]));
        }
    }
    let mut triples = Vec::new();
    for a in [0u8, 5, 10] {
        for b in [0u8, 5, 10] {
            for c in [0u8, 8] {
                triples.push([a, b, c]);
            }
        }
    }
    for m in 0..8usize {
        let flags = [m & 1 != 0, m & 2 != 0, m & 4 != 0];
        for code in 0..12u8 {
            for sel in [0u8, 1] {
                for pos in &triples {
                    for w in 0..3u8 {
                        let r = zone_range(code, flags, *pos, [w; 3], sel);
                        for v in [r.lower.0, r.lower.1, r.upper.0, r.upper.1] {
                            eat(u32::from(v));
                        }
                        for note in [21u8, 40, 53, 59, 60, 65, 66, 108] {
                            eat(r.gain_16_16(note));
                        }
                    }
                }
            }
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [bool; 3] = [true; 3];

    #[test]
    fn digest_matches_the_arm_checked_python_model() {
        // `python tools/arm_zone_map.py` prints the same value after `--check` reported 0 mismatches.
        assert_eq!(digest(), 0x3282_5B10_0986_BEB1);
    }

    #[test]
    fn without_a_split_every_code_spans_the_keybed() {
        for code in 0..11 {
            let r = zone_range(code, [false; 3], [1, 4, 7], [0; 3], 0);
            assert_eq!((r.lower, r.upper), ((21, 0), (108, 0)), "code {code}");
        }
    }

    #[test]
    fn mid_only_codes_zero_and_six_are_the_two_halves() {
        let flags = [false, true, false];
        let low = zone_range(0, flags, [1, 4, 7], [0; 3], 0);
        let high = zone_range(6, flags, [1, 4, 7], [0; 3], 0);
        assert_eq!(
            (low.lower.0, low.upper.0),
            (21, 59),
            "Mid setting 4 is key 60"
        );
        assert_eq!((high.lower.0, high.upper.0), (60, 108));
    }

    #[test]
    fn four_zone_codes_follow_the_firmware_table() {
        // settings (1, 4, 7) = keys 41, 60, 77
        let z = |code| zone_range(code, ALL, [1, 4, 7], [0; 3], 0);
        assert_eq!((z(0).lower.0, z(0).upper.0), (21, 40));
        assert_eq!((z(1).lower.0, z(1).upper.0), (41, 59));
        assert_eq!((z(9).lower.0, z(9).upper.0), (60, 76), "code 9 = zone 3");
        assert_eq!(
            (z(10).lower.0, z(10).upper.0),
            (77, 108),
            "code 10 = zone 4"
        );
        assert_eq!((z(2).lower.0, z(2).upper.0), (21, 59), "zones 1-2");
        assert_eq!((z(8).lower.0, z(8).upper.0), (60, 108), "zones 3-4");
    }

    #[test]
    fn code_eleven_selector_one_is_empty() {
        let r = zone_range(11, ALL, [1, 4, 7], [0; 3], 1);
        assert_eq!((r.lower.0, r.upper.0), (0, 1));
        assert_eq!(r.gain_16_16(60), 0);
    }

    #[test]
    fn split_index_keeps_the_points_ordered() {
        assert_eq!(split_index(ALL, [0, 0, 0], 0), 0);
        assert_eq!(split_index(ALL, [0, 0, 0], 1), 1);
        assert_eq!(
            split_index(ALL, [0, 0, 0], 2),
            2,
            "ARM: k=2 with Mid+1 <= 1 gives 2"
        );
        assert_eq!(split_index(ALL, [9, 3, 1], 0), 2);
        assert_eq!(
            split_index([false, true, false], [9, 3, 1], 1),
            3,
            "single point unchanged"
        );
        assert_eq!(
            split_index([true, false, true], [4, 0, 4], 2),
            5,
            "Low+High: High above Low"
        );
    }

    #[test]
    fn crossfade_is_the_smaller_of_the_two_firmware_curves() {
        // Mid only, width 1 (6 keys): lower zone fades down over 54..65, upper zone up over 54..65.
        let flags = [false, true, false];
        let low = zone_range(0, flags, [1, 4, 7], [0, 1, 0], 0);
        let high = zone_range(6, flags, [1, 4, 7], [0, 1, 0], 0);
        assert_eq!(low.gain_16_16(53), UNITY);
        assert_eq!(high.gain_16_16(53), 0);
        assert_eq!(low.gain_16_16(54), FADE_12[11]);
        assert_eq!(high.gain_16_16(54), FADE_12[0]);
        assert_eq!(low.gain_16_16(65), FADE_12[0]);
        assert_eq!(high.gain_16_16(66), UNITY);
        assert_eq!(low.gain_16_16(66), 0);
        assert_eq!(
            high.gain_16_16(60),
            0x8000,
            "half way through the 12 key fade"
        );
        assert_eq!(high.gain(60), 0.5);
    }

    #[test]
    fn split_setting_of_inverts_the_note_table() {
        for (i, n) in SPLIT_NOTES.iter().enumerate() {
            assert_eq!(usize::from(split_setting_of(*n)), i);
        }
        assert_eq!(split_setting_of(0), 0);
        assert_eq!(split_setting_of(127), 10);
    }

    #[test]
    fn fade_curves_are_monotone_and_end_near_full_scale() {
        assert!(FADE_12.windows(2).all(|w| w[0] < w[1]));
        assert!(FADE_24.windows(2).all(|w| w[0] < w[1]));
        assert!(FADE_12[11] < UNITY && FADE_24[23] < UNITY);
    }
}

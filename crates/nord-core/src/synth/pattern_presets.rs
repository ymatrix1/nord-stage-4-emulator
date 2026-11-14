//! Factory Arpeggiator/Gate pattern presets (hardware `Edt_Syn_kArpPtnPreset`, values 0..=28).
//!
//! Values 1..=28 come from the table that the firmware's `GlueArpPtnPreset` update method indexes
//! (OS v1.66, `bank0_CODE.bin@0xC02BA860`, 20-byte structs). The routine at `0xC010FF50` turns each
//! entry into a gate word and a pan word; `tools/ns4_arp_presets.py` runs it on the ARM emulator
//! and `docs/research/arp-preset-table.csv` holds the result, which the table below repeats as
//! step strings (`E q Q .` gate codes, `C L R` pan).

use super::*;

/// Number of preset values the dial has: 0 (default pattern) and 1..=28 (table entries).
/// VERIFIED: range `0..28` of `Edt_Syn_kArpPtnPreset` (`docs/research/arm-edit-params.csv` row 341)
/// and 28 table entries (`python tools/ns4_arp_presets.py table <bank0_CODE.bin>`).
pub const PATTERN_PRESET_COUNT: usize = 29;

/// Highest preset value (`PATTERN_PRESET_COUNT - 1`), the clamp of the dial and the host parameter.
pub const PATTERN_PRESET_MAX: u8 = (PATTERN_PRESET_COUNT - 1) as u8;

/// A preset pattern: `len` steps, each a gate length in eighths of a step (`0` = rest, `8` = full
/// step) and a pan. Presets never accent (the firmware writes an all-zero accent word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatePattern {
    pub len: u8,
    pub steps: [u8; 16],
    pub pan: [PatternPan; 16],
}

/// Gate length in eighths of a step for the hardware gate codes `E`, `q`, `Q`, `.` (rest).
/// ASSUMED: `E` = 4 as before, `q` = 6 and `Q` = 8 keep the order short < medium < long. The
/// firmware only shows that `Q` is a long step followed by a played step and `q` a long step
/// followed by a rest (`S`/`L` strings of the table); the durations are unmeasured. Confirm by
/// recording the hardware gate of a preset with `Q` and `q` steps (for example presets 10 and 24).
const fn gate_eighths(code: u8) -> u8 {
    match code {
        b'E' => 4,
        b'q' => 6,
        b'Q' => 8,
        b'.' => 0,
        _ => panic!("gate code must be one of E q Q ."),
    }
}

const fn pan_of(code: u8) -> PatternPan {
    match code {
        b'C' => PatternPan::Center,
        b'L' => PatternPan::Left,
        b'R' => PatternPan::Right,
        _ => panic!("pan code must be one of C L R"),
    }
}

/// Builds a preset from its gate string and pan string (equal length, 2..=16 characters).
const fn preset(gate: &str, pan: &str) -> GatePattern {
    let (g, p) = (gate.as_bytes(), pan.as_bytes());
    assert!(g.len() == p.len() && g.len() >= 2 && g.len() <= 16);
    let mut out = GatePattern {
        len: g.len() as u8,
        steps: [0; 16],
        pan: [PatternPan::Center; 16],
    };
    let mut i = 0;
    while i < g.len() {
        out.steps[i] = gate_eighths(g[i]);
        out.pan[i] = pan_of(p[i]);
        i += 1;
    }
    out
}

/// The 29 presets, index = hardware dial value.
///
/// Index 0: ASSUMED default pattern (8 steps `E`, centred): the parameter default is 0 and a new
/// patch holds the all-`E` 8-step pattern (4 factory layers), but no firmware table entry exists
/// for 0. Indices 1..=28: VERIFIED against the firmware routine (`tools/ns4_arp_presets.py`, max
/// error 0 steps); presets 2, 4, 11, 13 and 25 also appear unedited (steps, gate and pan words
/// equal) in the factory backup, presets 5 and 27 with an edited pan
/// (`python tools/ns4_arp_presets.py check <backup>.ns4b docs/research/arp-preset-table.csv`).
/// The gate durations are `ASSUMED` (see [`gate_eighths`]).
pub const PATTERN_PRESETS: [GatePattern; PATTERN_PRESET_COUNT] = [
    preset("EEEEEEEE", "CCCCCCCC"),
    preset("EEEEEEQEEQEEEEEE", "CCRCLRCCCCCCLRCC"),
    preset("EEQEEEEEEEEEEEQE", "LRCCCCCCCRCLRCCC"),
    preset("EEQEEQEEQEEQEEEE", "CCCCCCCCCCCCCCCC"),
    preset("EEQEEEQEEEEQEEQE", "CCCCLLRRLLRRRLLL"),
    preset("EQEEQEEQEEEEQEEE", "CCCCCCCCCCCCCCCC"),
    preset("E.QE.QEE", "CCCCCCCC"),
    preset("EEQEEEQEEEQEE.E.", "CCCCLLLLRRRRRCLC"),
    preset("EEQEE.QEE.QEE...", "CCCCCCCCCCCCCCCC"),
    preset("E.QE.QE.E.QE.QEE", "CCCCCCCCCCCCCCCC"),
    preset("QEQE.q.E", "CCCCCCCC"),
    preset("EEEEQEEEEEEEEEQE", "CCCCCCCCCCCCCCCC"),
    preset("EEE.EE..EE.EE.Eq", "CCCCCCCCCCCCCCCC"),
    preset("QEEEEEQEQEEEEEQE", "LLRLLRLLRRLRRLRR"),
    preset("E.E.EEEE", "CCCCCRCL"),
    preset("QQE.QQEEE.EQEEQE", "CCCCCCCCCCCCCCCC"),
    preset("EQEEQEEQEEQEQEQE", "CCCCCCCCCCCCCCCC"),
    preset("QEEQEEEEQEEQEEQE", "CCCCCCCCCCCCCCCC"),
    preset("E.E.QEE.E.E.QEE.", "CCCCCCCCCCCCCCCC"),
    preset("QEQEQEEEEEEQEEQE", "CCCCCCCCCCCCCCCC"),
    preset("EEEEE.E.", "CCCCLCRC"),
    preset("E.E.QQE.QQE.QEEE", "CCCCLLLCRRRCLLCC"),
    preset("q.E.Qq..E.E.EE..", "CCCCCCCCCCCCCCCC"),
    preset("E...QQE.E...EEE.", "CCCCCCCCCCCCCCCC"),
    preset("q...EEEEq...q...", "CCCCCCCCCCCCCCCC"),
    preset("EE.EEEEEEEEE.EEE", "CLCCRLRCRLRCCLCC"),
    preset("EEE.EEE.EEEEEEQE", "CRLCRCLCCRLRCCCC"),
    preset("EEEEEEEEEEEEEEEE", "CLCRLCCCCRCCCRLR"),
    preset("E..EEEE.EEEEEE..", "CCCLRCLCCRLRCCCC"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_the_hardware_range() {
        assert_eq!(PATTERN_PRESETS.len(), 29);
        assert_eq!(PATTERN_PRESET_COUNT, 29);
    }

    #[test]
    fn presets_are_well_formed() {
        for (i, p) in PATTERN_PRESETS.iter().enumerate() {
            assert!(p.len == 8 || p.len == 16, "preset {i} length {}", p.len);
            assert!(
                p.steps[..usize::from(p.len)].iter().any(|&s| s > 0),
                "preset {i}"
            );
            assert!(
                p.steps[usize::from(p.len)..].iter().all(|&s| s == 0),
                "preset {i}"
            );
            assert!(p.steps.iter().all(|&s| s <= 8));
        }
    }

    #[test]
    fn preset_zero_is_the_default_pattern() {
        let d = CustomPattern::default();
        let p = PATTERN_PRESETS[0];
        assert_eq!(p.len, d.len);
        assert_eq!(p.steps[..8], d.gate[..8]);
        assert_eq!(p.pan, d.pan);
    }

    #[test]
    fn firmware_gate_codes_map_to_eighths() {
        assert_eq!(gate_eighths(b'E'), 4);
        assert_eq!(gate_eighths(b'q'), 6);
        assert_eq!(gate_eighths(b'Q'), 8);
        assert_eq!(gate_eighths(b'.'), 0);
    }

    #[test]
    fn preset_13_matches_the_factory_program_pattern() {
        // Gate QEEEEEQEQEEEEEQE and pan LLRLLRLLRRLRRLRR, held unedited by the factory programs
        // "Glitter Whl" and "Tri Pump Arp" (tools/ns4_arp_presets.py check).
        let p = PATTERN_PRESETS[13];
        assert_eq!(p.len, 16);
        assert_eq!(&p.steps[..8], &[8, 4, 4, 4, 4, 4, 8, 4]);
        assert_eq!(p.pan[0], PatternPan::Left);
        assert_eq!(p.pan[2], PatternPan::Right);
        assert_eq!(p.pan[3], PatternPan::Left);
        assert_eq!(p.pan[9], PatternPan::Right);
    }

    #[test]
    fn rests_and_pans_follow_the_strings() {
        // Preset 25: gate EE.EEEEEEEEE.EEE, pan CLCCRLRCRLRCCLCC.
        let p = PATTERN_PRESETS[25];
        assert_eq!(p.steps[2], 0);
        assert_eq!(p.steps[12], 0);
        assert_eq!(p.steps[3], 4);
        assert_eq!(p.pan[1], PatternPan::Left);
        assert_eq!(p.pan[4], PatternPan::Right);
        assert_eq!(p.pan[15], PatternPan::Center);
    }
}

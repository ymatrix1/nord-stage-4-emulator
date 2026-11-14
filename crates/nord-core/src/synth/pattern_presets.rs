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
/// step), a tie flag and a pan. Presets never accent (the firmware writes an all-zero accent word).
///
/// `tie[i]` is the hardware gate code `Q`: the note of step `i` is held into step `i + 1`, which
/// does not trigger (the arpeggio does not advance) and ends with its own gate. The last step
/// never ties across the pattern end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatePattern {
    pub len: u8,
    pub steps: [u8; 16],
    pub tie: [bool; 16],
    pub pan: [PatternPan; 16],
}

/// Characters of the 2-bit gate codes `0..=3` of the pattern word (`Edt_Syn_kArpPtnDur`).
pub(super) const GATE_CODES: &[u8; 4] = b"EqQ.";

/// Characters of the 2-bit pan codes `0..=2` of the pattern word; code 3 is unused (centre).
pub(super) const PAN_CODES: &[u8; 4] = b"CLRC";

/// Gate length in eighths of a step that the note of a step lasts by itself, for the hardware gate
/// codes `E`, `q`, `Q`, `.` (rest). `Q` additionally ties into the next step (see [`GatePattern`]).
///
/// VERIFIED: the firmware pattern compiler at `bank0_CODE.bin@0xC02424DC` turns each step into two
/// half-step slots (`[obj+0x24] = steps * 2` at `0xC011C520`) with note-on, hold and note-off
/// records: `E` is on for 1 slot (4 eighths), `q` for 2 slots (8 eighths, one full step), `Q` is on
/// for 2 slots and held into the next step (no new note: same tick). Checked by
/// `python tools/ns4_arp_gate.py gates <bank0_CODE.bin> docs/research/arp-preset-table.csv` (the
/// rule reproduces every note of the 29 presets and of all 1360 gate strings of 2 to 5 steps, 0
/// differences). That one slot is half a step (4 eighths) in time is EMPIRICAL: two slots per step
/// are structural in the compiler and the manual (p.36) says one step is one 1/8 note, but the
/// player that walks the records was not located.
pub(super) const fn gate_eighths(code: u8) -> u8 {
    match code {
        b'E' => 4,
        b'q' | b'Q' => 8,
        b'.' => 0,
        _ => panic!("gate code must be one of E q Q ."),
    }
}

pub(super) const fn pan_of(code: u8) -> PatternPan {
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
        tie: [false; 16],
        pan: [PatternPan::Center; 16],
    };
    let mut i = 0;
    while i < g.len() {
        out.steps[i] = gate_eighths(g[i]);
        out.tie[i] = g[i] == b'Q';
        out.pan[i] = pan_of(p[i]);
        i += 1;
    }
    out
}

/// The 29 presets, index = hardware dial value.
///
/// Index 0: not a table entry. The firmware treats dial value 0 as "the saved custom pattern"
/// (`GlueArpPtnPreset` listener `0xC0102F30` and update method `0xC010FDAC`, replayed by
/// `python tools/ns4_arp_gate.py dial <bank0_CODE.bin>`); the engine plays `PerfState::custom` for
/// it. This entry is only the hardware default of that pattern (8 steps `E`, centred; the
/// parameter default and 4 factory layers, EMPIRICAL). Indices 1..=28: VERIFIED against the firmware routine (`tools/ns4_arp_presets.py`, max
/// error 0 steps); presets 2, 4, 11, 13 and 25 also appear unedited (steps, gate and pan words
/// equal) in the factory backup, presets 5 and 27 with an edited pan
/// (`python tools/ns4_arp_presets.py check <backup>.ns4b docs/research/arp-preset-table.csv`).
/// The gate durations are VERIFIED relative to the firmware compiler (see [`gate_eighths`]).
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
        assert_eq!(gate_eighths(b'q'), 8);
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
    fn q_codes_tie_and_other_codes_do_not() {
        // Preset 10 = QEQE.q.E: Q at 0 and 2 tie into the next step, q (step 5) is a full step.
        let p = PATTERN_PRESETS[10];
        assert_eq!(&p.steps[..8], &[8, 4, 8, 4, 0, 8, 0, 4]);
        assert_eq!(
            &p.tie[..8],
            &[true, false, true, false, false, false, false, false]
        );
        // No preset ties a rest, and no flag sits beyond the pattern length.
        for (i, p) in PATTERN_PRESETS.iter().enumerate() {
            let n = usize::from(p.len);
            assert!((0..n).all(|k| !p.tie[k] || p.steps[k] > 0), "preset {i}");
            assert!(p.tie[n..].iter().all(|&t| !t), "preset {i}");
        }
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

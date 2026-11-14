//! Oscillator name tables of the Synth display: ANALOG and WAVE categories and the FM groups.
//!
//! Category membership, order and entry names are the firmware's: the oscillator browser tree
//! built by the OS v1.66 routine `0xC01ADBC0` (`python tools/ns4_wave_cats.py`, committed
//! derived table `docs/research/data/synth-wave-categories.csv`, note
//! `docs/research/synth-wave-categories.md`). Every table here is checked row by row against that
//! CSV by `tests::tables_match_the_firmware_tree`.
//!
//! Which engine waveform a name drives is ASSUMED (the engine has five virtual-analog shapes), so
//! inexact entries carry `exact = false` and the display marks them with `~`.

use nord_core::synth::VaWaveform;
use VaWaveform::{Pulse, Saw, Sine, SuperSaw, Triangle};

/// One waveform of the ANALOG type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveEntry {
    /// Firmware entry name (`Saw 8ve`, not `Multi Saw 8ve`; see [`WaveCat::prefix`]).
    pub name: &'static str,
    /// Nearest engine waveform.
    pub wave: VaWaveform,
    /// Fixed pulse duty (Pulse waveforms), if any.
    pub pulse: Option<f32>,
    /// The engine reproduces this waveform; `false` = nearest-shape stand-in.
    pub exact: bool,
    /// Firmware wave id: the first argument of the entry's message list in the browser tree, equal
    /// to the `type` column of `docs/research/data/synth-wave-types.csv` (VERIFIED, tests).
    pub fw_id: u8,
}

/// A waveform category (middle dial).
#[derive(Debug, Clone, Copy)]
pub struct WaveCat {
    pub name: &'static str,
    /// Words shown before the entry name on the OLED. The firmware stores `Saw 8ve` under `Multi`;
    /// the manual's OLED figure (p.9, `OSC WAVEFORM`, `ANALOG MULTI 2(6)`) and the text of p.30
    /// show `Multi Saw 8ve` and `Super Saw`. EMPIRICAL for MULTI and SUPER (manual); the other
    /// categories show the bare entry name (ASSUMED: nothing in the manual contradicts it).
    pub prefix: &'static str,
    pub waves: &'static [WaveEntry],
}

const fn w(
    name: &'static str,
    wave: VaWaveform,
    pulse: Option<f32>,
    exact: bool,
    fw_id: u8,
) -> WaveEntry {
    WaveEntry {
        name,
        wave,
        pulse,
        exact,
        fw_id,
    }
}

/// ANALOG type categories in firmware order; the index equals the `osc_category` field of the
/// Synth sound block (`PURE, SUB OSC, SYNC, SHAPE, SHAPE SINE, MULTI, SUPER, MISC`).
///
/// VERIFIED: membership, order and names against `bank0_CODE.bin@0xC01ADBC0` (OS v1.66), ids
/// against the DSP P-table (45 of 45), by `tools/tests/test_ns4_wave_cats.py` and
/// `tests::tables_match_the_firmware_tree`.
pub const ANALOG: [WaveCat; 8] = [
    WaveCat {
        name: "PURE",
        prefix: "",
        waves: &[
            w("Sine", Sine, None, true, 0),
            w("Triangle", Triangle, None, true, 1),
            w("Saw", Saw, None, true, 2),
            w("Square", Pulse, Some(0.5), true, 3),
            w("Pulse 33", Pulse, Some(0.33), true, 4),
            w("Pulse 10", Pulse, Some(0.10), true, 5),
            w("White Noise", Saw, None, false, 46),
        ],
    },
    WaveCat {
        name: "SUB OSC",
        prefix: "",
        waves: &[
            w("Sub Square Saw", Saw, None, false, 10),
            w("Sub Pulse Saw", Saw, None, false, 11),
            w("Sub Sine Saw", Saw, None, false, 12),
        ],
    },
    // The engine has no oscillator sync: nearest plain shapes (ASSUMED).
    WaveCat {
        name: "SYNC",
        prefix: "",
        waves: &[
            w("Sine", Sine, None, false, 27),
            w("Triangle", Triangle, None, false, 28),
            w("Saw", Saw, None, false, 29),
            w("Square", Pulse, Some(0.5), false, 30),
            w("Pulse 33", Pulse, Some(0.33), false, 31),
            w("Pulse 10", Pulse, Some(0.10), false, 32),
            w("Chop Saw 1", Saw, None, false, 33),
            w("Chop Saw 2", Saw, None, false, 34),
            w("Chop Square", Pulse, Some(0.5), false, 35),
        ],
    },
    WaveCat {
        name: "SHAPE",
        prefix: "",
        waves: &[
            w("Triangle", Triangle, None, false, 7),
            w("Saw", Saw, None, false, 8),
            w("Square", Pulse, None, false, 9),
            w("Half Sine", Sine, None, false, 43),
            w("Parabolic", Triangle, None, false, 44),
        ],
    },
    WaveCat {
        name: "SHAPE SINE",
        prefix: "",
        waves: &[
            w("Saw", Saw, None, false, 38),
            w("Square", Pulse, None, false, 39),
            w("Pulse", Pulse, None, false, 13),
            w("Squeeze", Sine, None, false, 41),
            w("Fold", Sine, None, false, 42),
            w("ESaw", Saw, None, false, 36),
            w("ESquare", Pulse, None, false, 37),
        ],
    },
    WaveCat {
        name: "MULTI",
        prefix: "Multi ",
        waves: &[
            w("Saw", SuperSaw, None, false, 16),
            w("Saw 8ve", SuperSaw, None, false, 17),
            w("Saw 5th", SuperSaw, None, false, 19),
            w("Saw 5th+", SuperSaw, None, false, 20),
            w("Saw 8ve 8ve+", SuperSaw, None, false, 18),
            w("Saw 8ve 5th+", SuperSaw, None, false, 21),
        ],
    },
    WaveCat {
        name: "SUPER",
        prefix: "Super ",
        waves: &[
            w("Saw", SuperSaw, None, true, 22),
            w("Square", SuperSaw, None, false, 23),
            w("Organ", SuperSaw, None, false, 24),
            w("Saw Bright", SuperSaw, None, false, 25),
            w("Square Bright", SuperSaw, None, false, 26),
        ],
    },
    WaveCat {
        name: "MISC",
        prefix: "",
        waves: &[
            w("Pink Noise", Saw, None, false, 47),
            w("Red Noise", Saw, None, false, 48),
            w("Bell", Sine, None, false, 45),
        ],
    },
];

/// One waveform of the WAVE type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableEntry {
    pub name: &'static str,
    /// Firmware wave-table id (the argument of the entry's message list). The ids are not in
    /// category order (`Organ` is `0..9`, `Acoustic` `10..15`); our synthetic 64-table bank is
    /// indexed by the flat position instead (ASSUMED, see [`table_flat_index`]).
    pub fw_id: u8,
}

/// A WAVE category.
#[derive(Debug, Clone, Copy)]
pub struct TableCat {
    pub name: &'static str,
    pub waves: &'static [TableEntry],
}

const fn t(name: &'static str, fw_id: u8) -> TableEntry {
    TableEntry { name, fw_id }
}

/// WAVE type categories in firmware order (index = `osc_category`); 6 + 6 + 14 + 10 + 10 = 46.
///
/// VERIFIED against `bank0_CODE.bin@0xC01ADBC0` like [`ANALOG`]; replaces the eight invented
/// `TABLES n` groups of eight.
pub const WAVE_TABLES: [TableCat; 5] = [
    TableCat {
        name: "BELLS/TINES",
        waves: &[
            t("Bell", 40),
            t("Bar 1", 41),
            t("Bar 2", 42),
            t("Tines", 43),
            t("Marimba", 44),
            t("Tubular Bells", 45),
        ],
    },
    TableCat {
        name: "ACOUSTIC",
        waves: &[
            t("Flute 1", 10),
            t("Flute 2", 11),
            t("Clarinet 1", 12),
            t("Clarinet 2", 13),
            t("Alto Sax", 14),
            t("Tenor Sax", 15),
        ],
    },
    TableCat {
        name: "DIGITAL",
        waves: &[
            t("2nd Spectra", 16),
            t("3rd Spectra", 17),
            t("4th Spectra", 18),
            t("5th Spectra", 19),
            t("6th Spectra", 20),
            t("7th Spectra", 21),
            t("8th Spectra", 22),
            t("Saw Random", 23),
            t("Saw Bright", 24),
            t("Saw NoFund", 26),
            t("Square Bright", 25),
            t("Ice 1", 34),
            t("Ice 2", 35),
            t("Triplets", 39),
        ],
    },
    TableCat {
        name: "ORGAN",
        waves: &[
            t("Second", 0),
            t("Third", 1),
            t("Jimmy Smith", 2),
            t("Blues", 3),
            t("Gospel", 4),
            t("Church", 5),
            t("Squabble", 6),
            t("Full Organ", 7),
            t("Full Organ+", 8),
            t("Principal", 9),
        ],
    },
    TableCat {
        name: "KEYS",
        waves: &[
            t("EPiano 1", 27),
            t("EPiano 2", 28),
            t("EPiano 3", 29),
            t("DX 1", 30),
            t("DX 2", 31),
            t("Full Tines", 32),
            t("AcPiano", 33),
            t("Clavinet 1", 36),
            t("Clavinet 2", 37),
            t("Clavinet 3", 38),
        ],
    },
];

/// Position of a WAVE entry over all categories, `0..46`: the index of the engine's wavetable bank
/// (same order as `nord_core::cbin` `wave_flat_index`). ASSUMED: the bank contents are synthetic.
pub fn table_flat_index(cat: u8, idx: u8) -> u8 {
    let cat = usize::from(cat).min(WAVE_TABLES.len() - 1);
    let before: usize = WAVE_TABLES[..cat].iter().map(|c| c.waves.len()).sum();
    let last = WAVE_TABLES[cat].waves.len() - 1;
    (before + usize::from(idx).min(last)) as u8
}

/// Oscillator TYPE dial entries (manual p.29). `FM-H` and `FM-I` both drive the engine FM mode.
pub const WAVE_TYPES: [&str; 4] = ["ANALOG", "FM-H", "FM-I", "WAVE"];

/// Number of FM groups per FM type (letters `A..E`).
///
/// VERIFIED: the loop bound `cmp r6, #5` of the FM-H and FM-I builders (`ns4_wave_cats.py --fm`).
pub const FM_GROUPS: u8 = 5;

/// FM category name for the group `algo` (`0..5`) of FM-H (`wave_type == 1`) or FM-I.
///
/// VERIFIED: the firmware formats the category as `Harmonic %c` / `Inharmonic %c` with `%c` from
/// `A` (`bank0_CODE.bin@0x2D7EC0`, `@0x2D7ED4`); upper case follows the other categories (ASSUMED).
/// Replaces the invented `ALGO A..E`.
pub fn fm_cat_name(wave_type: u8, algo: u8) -> String {
    let letter = char::from(b'A' + algo.min(FM_GROUPS - 1));
    let family = if wave_type == 2 {
        "INHARMONIC"
    } else {
        "HARMONIC"
    };
    format!("{family} {letter}")
}

/// Harmonic FM partial for index `i` (manual p.31: 0.5 to 24).
///
/// VERIFIED: 25 doubles `0.5, 1 .. 24` at `bank0_CODE.bin@0x2D82A8` (`ns4_wave_cats.py --fm`).
pub fn harmonic_partial(i: u8) -> f32 {
    if i == 0 {
        0.5
    } else {
        f32::from(i.min(24))
    }
}

/// Number of harmonic partials (0.5, 1 .. 24).
pub const HARMONIC_COUNT: u8 = 25;

/// Inharmonic FM pitch in semitones for index `i` (manual p.31: -12 to 48).
///
/// VERIFIED: the FM-I loop runs `0..0x3D` (61 items) and formats `%d` of `i - 12`.
pub fn inharmonic_semitones(i: u8) -> i8 {
    i.min(60) as i8 - 12
}

/// Number of inharmonic pitches (-12 .. 48).
pub const INHARMONIC_COUNT: u8 = 61;

#[cfg(test)]
mod tests {
    use super::*;

    /// Derived from the firmware by `tools/ns4_wave_cats.py` (names and ids only).
    const TREE: &str =
        include_str!("../../../../../../docs/research/data/synth-wave-categories.csv");

    fn rows(kind: &str) -> Vec<(usize, usize, String, String, u8)> {
        TREE.lines()
            .skip(1)
            .filter_map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                (f[0] == kind).then(|| {
                    (
                        f[1].parse().unwrap(),
                        f[3].parse().unwrap(),
                        f[2].to_string(),
                        f[4].to_string(),
                        f[5].parse().unwrap(),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn tables_match_the_firmware_tree() {
        let analog = rows("Analog");
        assert_eq!(
            analog.len(),
            ANALOG.iter().map(|c| c.waves.len()).sum::<usize>()
        );
        for (cat, wave, cat_name, name, id) in analog {
            let c = &ANALOG[cat];
            assert!(c.name.eq_ignore_ascii_case(&cat_name), "{cat_name}");
            let e = &c.waves[wave];
            assert_eq!((e.name, e.fw_id), (name.as_str(), id), "{cat_name} {wave}");
        }
        let wave = rows("Wave");
        assert_eq!(
            wave.len(),
            WAVE_TABLES.iter().map(|c| c.waves.len()).sum::<usize>()
        );
        for (cat, idx, cat_name, name, id) in wave {
            let c = &WAVE_TABLES[cat];
            assert!(c.name.eq_ignore_ascii_case(&cat_name), "{cat_name}");
            assert_eq!((c.waves[idx].name, c.waves[idx].fw_id), (name.as_str(), id));
        }
    }

    #[test]
    fn analog_categories_follow_the_osc_category_field() {
        let names: Vec<_> = ANALOG.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            [
                "PURE",
                "SUB OSC",
                "SYNC",
                "SHAPE",
                "SHAPE SINE",
                "MULTI",
                "SUPER",
                "MISC"
            ]
        );
        assert!(ANALOG.iter().all(|c| !c.waves.is_empty()));
    }

    #[test]
    fn wave_categories_have_the_cbin_sizes() {
        let sizes: Vec<_> = WAVE_TABLES.iter().map(|c| c.waves.len()).collect();
        assert_eq!(sizes, [6, 6, 14, 10, 10]);
        assert_eq!(table_flat_index(0, 0), 0);
        assert_eq!(table_flat_index(2, 3), 15);
        assert_eq!(table_flat_index(4, 9), 45);
        assert_eq!(table_flat_index(0, 63), 5, "clamped inside the category");
    }

    #[test]
    fn firmware_ids_are_unique_per_type() {
        let mut a: Vec<u8> = ANALOG
            .iter()
            .flat_map(|c| c.waves)
            .map(|e| e.fw_id)
            .collect();
        a.sort_unstable();
        a.dedup();
        assert_eq!(a.len(), 45);
        let mut t: Vec<u8> = WAVE_TABLES
            .iter()
            .flat_map(|c| c.waves)
            .map(|e| e.fw_id)
            .collect();
        t.sort_unstable();
        assert_eq!(t, (0..46).collect::<Vec<u8>>());
    }

    #[test]
    fn pure_category_lists_the_seven_waveforms_of_manual_p29() {
        let n: Vec<_> = ANALOG[0].waves.iter().map(|e| e.name).collect();
        assert_eq!(
            n,
            [
                "Sine",
                "Triangle",
                "Saw",
                "Square",
                "Pulse 33",
                "Pulse 10",
                "White Noise"
            ]
        );
    }

    #[test]
    fn pulse_entries_carry_their_duty() {
        let p33 = ANALOG[0].waves[4];
        assert_eq!(p33.wave, VaWaveform::Pulse);
        assert_eq!(p33.pulse, Some(0.33));
    }

    #[test]
    fn multi_and_super_carry_the_manual_prefix() {
        let multi = &ANALOG[5];
        assert_eq!(
            format!("{}{}", multi.prefix, multi.waves[1].name),
            "Multi Saw 8ve"
        );
        let sup = &ANALOG[6];
        assert_eq!(format!("{}{}", sup.prefix, sup.waves[0].name), "Super Saw");
        assert!(sup.waves[0].exact && !sup.waves[1].exact);
        assert!(ANALOG.iter().filter(|c| !c.prefix.is_empty()).count() == 2);
    }

    /// VERIFIED: the three SUB OSC names occur in `bank0_CODE.bin@0x2D7DEC..0x2D7E1C` (OS v1.66),
    /// checked by `tools/tests/test_ns4_oled_strings.py::test_fixed_wording_is_in_the_firmware`.
    #[test]
    fn sub_osc_names_use_the_firmware_word_order() {
        let names: Vec<_> = ANALOG[1].waves.iter().map(|w| w.name).collect();
        assert_eq!(names, ["Sub Square Saw", "Sub Pulse Saw", "Sub Sine Saw"]);
    }

    #[test]
    fn fm_pages_use_the_firmware_group_names() {
        assert_eq!(fm_cat_name(1, 0), "HARMONIC A");
        assert_eq!(fm_cat_name(2, 4), "INHARMONIC E");
        assert_eq!(
            fm_cat_name(1, 99),
            "HARMONIC E",
            "clamped to the last group"
        );
    }

    #[test]
    fn fm_partials_and_pitches_span_the_manual_ranges() {
        assert_eq!(harmonic_partial(0), 0.5);
        assert_eq!(harmonic_partial(HARMONIC_COUNT - 1), 24.0);
        assert_eq!(inharmonic_semitones(0), -12);
        assert_eq!(inharmonic_semitones(INHARMONIC_COUNT - 1), 48);
    }
}

//! Knob to strip word laws of the ARM edit ids (opt-in `SynthLayer::knob_laws`).
//!
//! The first edit capture wrote plain integers into the Q16.16 value cells, so every integer knob
//! saw position 0 and looked value independent. Re-driven with `knob << 16`
//! (`tools/arm_edit_knobs.py`, [`docs/research/synth-knob-laws.md`]) the ARM writes, for a layer
//! with the idle strip chain:
//!
//! | Edit id (knob 0..127) | DSP word | Law | Tag |
//! |---|---|---|---|
//! | `kAmpEnvAtk / Dcy / Rls`, `kFltEnvAtk / Dcy / Rls`, `kOscEnvAtk / Dcy / Rls`, `kGlideRate` | strip words 26..28, 29..31, 32..34, 55 | top byte (rate table index) `= knob - 64` | VERIFIED run |
//! | `kFltFreq` | common word 1 (`DM(0xE02E9)`) | top byte `= knob - 52` up to knob 117, then [`CUTOFF_TOP`] | VERIFIED run (#324) |
//! | `kFltRes`, `kOscCtrl` | common words 7, 6 | `-0.5 + knob / 127` (float32) | VERIFIED run |
//!
//! With `index = knob - 64` the rate tables of `fw_env_rates.rs` reproduce the displayed times
//! (`ENV_ATTACK_MS_TABLE`: attack ramp time `1 / cA` passes, ratio 1.00 at the first knob of every
//! plateau; `ENV_DECAY_MS_TABLE`: time constant `1 / (1 - a)` passes is `0.217 x` the displayed
//! time = `1 / ln 100`, i.e. the display is the time to fall by 40 dB), which VERIFIES the pass of
//! two SY frames that `strip_map.rs` ASSUMED. The strip chain therefore takes the rate index
//! straight from the stored knob instead of searching a table for a time.
//!
//! VERIFIED: tests below against `docs/research/data/synth-knob-laws.csv` (the capture, every knob).

use super::fw_env_rates::RATE_INDEX_MAX;
use serde::{Deserialize, Serialize};

/// Rate table index of an ADR / glide knob (`top byte of the strip word = knob - 64`).
pub const RATE_KNOB_OFFSET: i32 = 64;
/// Cutoff table index offset (`top byte of common word 1 = knob - 52`, knobs 0..=117).
pub const CUTOFF_KNOB_OFFSET: i32 = 52;
/// Last knob of the straight part of the cutoff law.
pub const CUTOFF_LINEAR_LAST: u8 = 117;
/// Common word 1 of knobs 118..=127 (steeper part, clipped by the pass at `0x6A000000`).
pub const CUTOFF_TOP: [u32; 10] = [
    0x42b3_3300,
    0x4466_6680,
    0x4619_9980,
    0x47cc_cd00,
    0x4980_0000,
    0x4b33_3300,
    0x4ce6_6680,
    0x4e99_9980,
    0x504c_cd00,
    0x6a00_0000,
];
/// Display time of a decay / release knob over the exponential time constant: the displayed time
/// is the time to fall by 40 dB (`ln 100` time constants). VERIFIED against the display table:
/// `1 / (1 - a) / (F / 2)` over `ENV_DECAY_MS_TABLE` is 0.217 at the first knob of each plateau.
pub const DECAY_DISPLAY_TAU: f32 = 0.217_147_24;

/// ADR knobs of one envelope (raw 0..=127 values of the stored program).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EnvKnobs {
    pub attack: u8,
    pub decay: u8,
    pub release: u8,
}

/// Envelope knobs of a layer: amp and filter envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StripKnobs {
    pub amp: EnvKnobs,
    pub filter: EnvKnobs,
}

/// Rate table index the ARM writes for a rate knob.
#[inline]
pub fn rate_index_for_knob(knob: u8) -> i32 {
    (i32::from(knob.min(127)) - RATE_KNOB_OFFSET).min(RATE_INDEX_MAX)
}

/// Strip word (Q31) the ARM writes for a rate knob: the index in the top byte.
#[inline]
pub fn rate_word_for_knob(knob: u8) -> i32 {
    rate_index_for_knob(knob) << 24
}

/// Common word 1 (Q31 cutoff position) the ARM writes for a `kFltFreq` knob.
pub fn cutoff_word_for_knob(knob: u8) -> i32 {
    let k = knob.min(127);
    if k <= CUTOFF_LINEAR_LAST {
        (i32::from(k) - CUTOFF_KNOB_OFFSET) << 24
    } else {
        CUTOFF_TOP[usize::from(k - CUTOFF_LINEAR_LAST - 1)] as i32
    }
}

/// Common word the ARM writes for a `kFltRes` / `kOscCtrl` knob (`-0.5 + knob / 127`).
#[inline]
pub fn unit_word_for_knob(knob: u8) -> f32 {
    -0.5 + f32::from(knob.min(127)) / 127.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/research/data/synth-knob-laws.csv"
    ));

    /// `(name, dm_word text, words)` of every varying row of the capture with the given name.
    fn series(name: &str, location: &str) -> Vec<u32> {
        let mut lines = CSV.lines();
        let header: Vec<&str> = lines.next().unwrap().split(',').collect();
        let col = |n: &str| header.iter().position(|h| *h == n).unwrap();
        for line in lines {
            let f: Vec<&str> = line.split(',').collect();
            if f[col("name")] == name && f[col("where")] == location && f[col("mode")] == "auto" {
                return f[col("words")]
                    .split(' ')
                    .map(|w| u32::from_str_radix(w.trim_start_matches("0x"), 16).unwrap())
                    .collect();
            }
        }
        panic!("{name} {location} not in the capture");
    }

    #[test]
    fn rate_knobs_are_the_top_byte_minus_64_for_every_adr_id() {
        let ids = [
            ("Edt_Syn_kAmpEnvAtk", "strip 0 word 26"),
            ("Edt_Syn_kAmpEnvDcy", "strip 0 word 27"),
            ("Edt_Syn_kAmpEnvRls", "strip 0 word 28"),
            ("Edt_Syn_kFltEnvAtk", "strip 0 word 29"),
            ("Edt_Syn_kFltEnvDcy", "strip 0 word 30"),
            ("Edt_Syn_kOscEnvDcy", "strip 0 word 33"),
            ("Edt_Syn_kOscEnvRls", "strip 0 word 34"),
            ("Edt_Syn_kGlideRate", "strip 0 word 55"),
        ];
        for (name, loc) in ids {
            let words = series(name, loc);
            assert_eq!(words.len(), 128, "{name}");
            for (knob, w) in words.iter().enumerate() {
                // the knob 127 index (63) is inside the table (-64..=64)
                assert_eq!(
                    rate_word_for_knob(knob as u8) as u32,
                    *w,
                    "{name} knob {knob}"
                );
            }
        }
    }

    #[test]
    fn cutoff_words_equal_the_capture_for_every_knob() {
        let words = series("Edt_Syn_kFltFreq", "common 0 word 1");
        assert_eq!(words.len(), 128);
        for (knob, w) in words.iter().enumerate() {
            assert_eq!(cutoff_word_for_knob(knob as u8) as u32, *w, "knob {knob}");
        }
        assert_eq!(cutoff_word_for_knob(0), -52 << 24);
        assert_eq!(cutoff_word_for_knob(127), 0x6A00_0000);
    }

    #[test]
    fn resonance_and_osc_ctrl_words_are_minus_half_plus_knob_over_127() {
        for (name, loc) in [
            ("Edt_Syn_kFltRes", "common 0 word 7"),
            ("Edt_Syn_kOscCtrl", "common 0 word 6"),
        ] {
            let words = series(name, loc);
            assert_eq!(words.len(), 128);
            for (knob, w) in words.iter().enumerate() {
                let got = unit_word_for_knob(knob as u8);
                let want = f32::from_bits(*w);
                assert!(
                    (got - want).abs() < 1.0e-6,
                    "{name} knob {knob}: {got} {want}"
                );
            }
        }
    }

    fn layer_with_knobs() -> super::super::SynthLayer {
        let mut s = super::super::SynthLayer::new(48_000.0);
        s.strip_knobs = Some(StripKnobs {
            amp: EnvKnobs {
                attack: 40,
                decay: 90,
                release: 70,
            },
            filter: EnvKnobs {
                attack: 10,
                decay: 127,
                release: 20,
            },
        });
        s
    }

    #[test]
    fn the_strip_chain_reads_the_knobs_only_when_the_flag_is_on() {
        use super::super::StripEnvRecords;
        let mut s = layer_with_knobs();
        let search = s.strip_params();
        s.knob_laws = true;
        let direct = s.strip_params();
        let k = s.strip_knobs.unwrap();
        assert_eq!(
            direct.amp,
            StripEnvRecords::from_knobs(k.amp, s.sustain_level)
        );
        assert_eq!(direct.filter, StripEnvRecords::from_knobs(k.filter, 0.0));
        assert_ne!(
            direct.amp, search.amp,
            "the cache key changed with the flag"
        );
        // filter decay knob 127 is Hold: full-scale sustain threshold
        assert_eq!(direct.filter.decay.thr, 1.0);
        s.knob_laws = false;
        assert_eq!(s.strip_params(), search);
        // a layer without knobs keeps the search whatever the flag says
        let mut plain = super::super::SynthLayer::new(48_000.0);
        let before = plain.strip_params();
        plain.knob_laws = true;
        assert_eq!(plain.strip_params(), before);
    }

    #[test]
    fn knob_records_use_the_rate_tables_at_index_knob_minus_64() {
        use super::super::{attack_coeff, decay_release_coeff, StripEnvRecords};
        let r = StripEnvRecords::from_knobs(
            EnvKnobs {
                attack: 64,
                decay: 80,
                release: 0,
            },
            0.25,
        );
        assert_eq!(r.attack.b, attack_coeff(0.0));
        assert_eq!(r.decay.a, decay_release_coeff(16.0));
        assert_eq!(r.decay.thr, 0.25);
        assert_eq!(r.release.a, decay_release_coeff(-64.0));
    }

    #[test]
    fn rate_index_clamps_into_the_table() {
        assert_eq!(rate_index_for_knob(0), -64);
        assert_eq!(rate_index_for_knob(64), 0);
        assert_eq!(rate_index_for_knob(127), 63);
        assert_eq!(rate_index_for_knob(200), 63);
    }

    #[test]
    fn rate_tables_reproduce_the_displayed_times() {
        use super::super::strip_map::{attack_seconds, decay_seconds};
        use crate::cbin::presets::{ENV_ATTACK_MS_TABLE, ENV_DECAY_MS_TABLE};
        // First knob of every 16: the displayed value starts a plateau of the rate table.
        for knob in (0..112u8).step_by(16) {
            let idx = rate_index_for_knob(knob);
            let atk = attack_seconds(idx) * 1000.0 / ENV_ATTACK_MS_TABLE[usize::from(knob)];
            assert!((atk - 1.0).abs() < 0.02, "attack knob {knob}: ratio {atk}");
            let dcy = decay_seconds(idx) * 1000.0 / ENV_DECAY_MS_TABLE[usize::from(knob)];
            assert!(
                (dcy / DECAY_DISPLAY_TAU - 1.0).abs() < 0.04,
                "decay knob {knob}: ratio {dcy}"
            );
        }
    }
}

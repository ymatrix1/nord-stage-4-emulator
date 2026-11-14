//! Arpeggiator / Gate pattern words stored in a Synth `0x32` Layer block.
//!
//! A program (and a `.ns4y` Synth preset) does not store the pattern preset index
//! (`Edt_Syn_kArpPtnPreset` is not saved) but the resulting words: the pattern-enable bit, the
//! length and three 32-bit words of 16 two-bit steps (accent, gate, pan). Loading a factory
//! program therefore reproduces its pattern by importing those words into the engine's
//! [`CustomPattern`].
//!
//! | field | layer-relative bit | width | meaning |
//! |---|---|---|---|
//! | `enabled` | 138 | 1 | `Edt_Syn_kArpPtnEnable` |
//! | `steps` | 206 | 4 | `steps - 2` (`Edt_Syn_kArpPtnLen` holds `steps - 1`, 1..=15, stored minus 1) |
//! | `accent` | 210 | 32 | step `n` at bits `2n..2n+1` from the LSB, `1` = accent |
//! | `gate` | 242 | 32 | `0 E`, `1 q`, `2 Q`, `3 .` (rest) |
//! | `pan` | 274 | 32 | `0 C`, `1 L`, `2 R` |
//!
//! EMPIRICAL: offsets derived from the ns4decode map ids `424-3`, `432-7`, `433-3`, `437-3`,
//! `441-3` and the field order of the firmware `Buffer_kSynLay` name table; the digit order and
//! the `E q Q .` / `C L R` letters are confirmed by 5 firmware presets that equal factory layers
//! exactly (`python tools/ns4_arp_presets.py check <backup>.ns4b docs/research/arp-preset-table.csv`).
//! The accent digit `1` = accent is EMPIRICAL (accent digits other than 0 and 1 never occur in the
//! factory backup). The same offsets are decoded by `tools/ns4_arp_patterns.py`.

use super::bitstream::{BitReader, BitWriter};
use super::container::CbinError;
use crate::synth::CustomPattern;
use serde::{Deserialize, Serialize};

const ENABLED_BIT: usize = 138;
const LENGTH_BIT: usize = 206;

/// Pattern fields of one Synth layer as stored (words, not an engine pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthPatternDecoded {
    /// Pattern mode is on for the layer.
    pub enabled: bool,
    /// Stored length plus 2 (`2..=17`; the engine clamps 17 to 16).
    pub steps: u8,
    pub accent: u32,
    pub gate: u32,
    pub pan: u32,
}

impl Default for SynthPatternDecoded {
    /// The all-zero bit pattern (a blank payload): pattern off, 2 steps, every word 0.
    fn default() -> Self {
        Self {
            enabled: false,
            steps: 2,
            accent: 0,
            gate: 0,
            pan: 0,
        }
    }
}

impl SynthPatternDecoded {
    /// Reads the pattern fields of the layer block starting at bit `layer_base`.
    pub(super) fn read(r: &mut BitReader, layer_base: usize) -> Result<Self, CbinError> {
        r.set_bit_pos(layer_base + ENABLED_BIT)?;
        let enabled = r.read_bool()?;
        r.set_bit_pos(layer_base + LENGTH_BIT)?;
        let steps = r.read_bits(4)? as u8 + 2;
        let accent = r.read_bits(32)?;
        let gate = r.read_bits(32)?;
        let pan = r.read_bits(32)?;
        Ok(Self {
            enabled,
            steps,
            accent,
            gate,
            pan,
        })
    }

    /// Writes the pattern fields back (the bits between them are left as they are).
    pub(super) fn write(&self, w: &mut BitWriter, layer_base: usize) -> Result<(), CbinError> {
        w.set_bit_pos(layer_base + ENABLED_BIT)?;
        w.write_bool(self.enabled)?;
        w.set_bit_pos(layer_base + LENGTH_BIT)?;
        w.write_bits(u32::from(self.steps.clamp(2, 17) - 2), 4)?;
        w.write_bits(self.accent, 32)?;
        w.write_bits(self.gate, 32)?;
        w.write_bits(self.pan, 32)
    }

    /// The layer holds a pattern other than the all-zero blank one (a program that never touched
    /// the pattern stores the hardware default, which differs from the blank bits).
    pub fn is_stored(&self) -> bool {
        *self != Self::default()
    }

    /// The stored words as the engine's custom pattern (gate `E q Q .` to eighths and ties).
    pub fn to_custom_pattern(&self) -> CustomPattern {
        CustomPattern::from_words(self.steps, self.gate, self.accent, self.pan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::PatternPan;

    /// Packs one code per character into a 2-bit-per-step word (step 0 = least significant).
    fn word(text: &str, chars: &str) -> u32 {
        text.chars()
            .enumerate()
            .map(|(i, c)| (chars.find(c).unwrap() as u32) << (2 * i))
            .sum()
    }

    fn write_at(buf: &mut [u8], base: usize, p: &SynthPatternDecoded) {
        let mut w = BitWriter::new(buf);
        p.write(&mut w, base).unwrap();
    }

    fn sample() -> SynthPatternDecoded {
        SynthPatternDecoded {
            enabled: true,
            steps: 8,
            accent: word("..>.....", ".>.."),
            gate: word("QEq.E.EE", "EqQ."),
            pan: word("CLRCCCCC", "CLR."),
        }
    }

    #[test]
    fn fields_round_trip_at_the_layer_offsets() {
        let mut buf = vec![0u8; 400];
        let p = sample();
        write_at(&mut buf, 248, &p);
        let mut r = BitReader::new(&buf);
        assert_eq!(SynthPatternDecoded::read(&mut r, 248).unwrap(), p);
        // Spot-check the absolute bit positions of the documented offsets.
        r.set_bit_pos(248 + 138).unwrap();
        assert!(r.read_bool().unwrap());
        r.set_bit_pos(248 + 206).unwrap();
        assert_eq!(r.read_bits(4).unwrap(), 6);
        assert_eq!(r.read_bits(32).unwrap(), p.accent);
        assert_eq!(r.read_bits(32).unwrap(), p.gate);
        assert_eq!(r.read_bits(32).unwrap(), p.pan);
    }

    #[test]
    fn blank_bits_decode_to_the_default_and_write_back_unchanged() {
        let mut buf = vec![0u8; 400];
        let mut r = BitReader::new(&buf);
        let blank = SynthPatternDecoded::read(&mut r, 248).unwrap();
        assert_eq!(blank, SynthPatternDecoded::default());
        assert!(!blank.is_stored());
        write_at(&mut buf, 248, &blank);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn a_nonblank_pattern_counts_as_stored_and_the_longest_length_is_kept_lossless() {
        let mut buf = vec![0u8; 400];
        let p = SynthPatternDecoded {
            steps: 17,
            ..sample()
        };
        assert!(p.is_stored());
        write_at(&mut buf, 248, &p);
        let mut r = BitReader::new(&buf);
        assert_eq!(SynthPatternDecoded::read(&mut r, 248).unwrap().steps, 17);
        // The engine clamps the unreachable 17 to 16.
        assert_eq!(p.to_custom_pattern().len, 16);
    }

    #[test]
    fn words_become_a_custom_pattern() {
        let c = sample().to_custom_pattern();
        assert_eq!(c.len, 8);
        // Gate QEq.E.EE: Q = full step and tie, E = 4, q = 8, . = rest.
        assert_eq!(&c.gate[..8], &[8, 4, 8, 0, 4, 0, 4, 4]);
        assert_eq!(
            &c.tie[..8],
            &[true, false, false, false, false, false, false, false]
        );
        // Accent digit 1 on step 2 only.
        assert_eq!(
            &c.accent[..8],
            &[false, false, true, false, false, false, false, false]
        );
        assert_eq!(c.pan[0], PatternPan::Center);
        assert_eq!(c.pan[1], PatternPan::Left);
        assert_eq!(c.pan[2], PatternPan::Right);
    }
}

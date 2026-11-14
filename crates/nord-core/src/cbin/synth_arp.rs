//! Arpeggiator / Gate mode fields stored in a Synth `0x32` Layer block.
//!
//! Next to the run switch (`+135`, `SynthLayerDecoded::arp_enabled`) and the pattern words
//! (`synth_pattern.rs`) a layer stores which kind of arpeggiator it plays and how:
//!
//! | field | layer-relative bit | width | meaning |
//! |---|---|---|---|
//! | `mode` | 136 | 2 | `0 Arp`, `1 Poly`, `2 Gate` (`Edt_Syn_kArpMode`) |
//! | `kb_sync` | 139 | 1 | `Edt_Syn_kArpKbdSync` |
//! | `range` | 140 | 7 | `Edt_Syn_kArpRange` raw; the envelope hardness in Gate mode |
//! | `direction` | 171 | 2 | `0 UP`, `1 DOWN`, `2 UP/DOWN`, `3 RANDOM` (`Edt_Syn_kArpDir`) |
//! | `zigzag` | 173 | 1 | `Edt_Syn_kArpZigZag` |
//! | `mst_clk` | 174 | 1 | `Edt_Syn_kArpClkEnable` (the rate is a Master Clock subdivision) |
//!
//! EMPIRICAL: offsets derived from the ns4decode map ids `424-1`, `424-4`, `424-5`, `428-4`,
//! `428-6`, `428-7` with the same base that puts the run switch (`423-8`) on 135 and the rate
//! (`428-8`) on 175, both already used by the decoder; the names come from `ns4names.ArpMode` and
//! `ArpDirection`. Checked against the factory backup with `tools/ns4_arp_modes.py`: 1152 layers,
//! every mode value is 0, 1 or 2 (1099 Arp, 41 Poly, 12 Gate), every direction value is used
//! (1114 UP, 1 DOWN, 29 UP/DOWN, 8 RANDOM), Poly and Gate occur with the run switch on and off.
//! The `range` scale is ASSUMED (see [`SynthArpDecoded::range_thirds`]): value 0 is the default of
//! 1099 layers and 127 occurs on 5.

use super::bitstream::{BitReader, BitWriter};
use super::container::CbinError;
use serde::{Deserialize, Serialize};

const MODE_BIT: usize = 136;
const KB_SYNC_BIT: usize = 139;
const RANGE_BIT: usize = 140;
const DIRECTION_BIT: usize = 171;
const ZIGZAG_BIT: usize = 173;
const MST_CLK_BIT: usize = 174;

/// Largest `range` raw value (7 bits).
pub const RANGE_RAW_MAX: u8 = 127;
/// Detents of the RANGE dial: 1 to 4 octaves in thirds (manual p.35) is `0..=9` thirds.
const RANGE_MAX_THIRDS: f32 = 9.0;

/// Arpeggiator mode fields of one Synth layer as stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SynthArpDecoded {
    /// `0` Arp, `1` Poly, `2` Gate (`3` is not used by the hardware).
    pub mode: u8,
    /// `0` UP, `1` DOWN, `2` UP/DOWN, `3` RANDOM (Arp mode only).
    pub direction: u8,
    pub zigzag: bool,
    /// The arpeggio follows a Master Clock subdivision instead of the rate in BPM.
    pub mst_clk: bool,
    pub kb_sync: bool,
    /// Raw RANGE value `0..=127`.
    pub range: u8,
}

impl SynthArpDecoded {
    /// Reads the mode fields of the layer block starting at bit `layer_base`.
    pub(super) fn read(r: &mut BitReader, layer_base: usize) -> Result<Self, CbinError> {
        r.set_bit_pos(layer_base + MODE_BIT)?;
        let mode = r.read_bits(2)? as u8;
        r.set_bit_pos(layer_base + KB_SYNC_BIT)?;
        let kb_sync = r.read_bool()?;
        r.set_bit_pos(layer_base + RANGE_BIT)?;
        let range = r.read_bits(7)? as u8;
        r.set_bit_pos(layer_base + DIRECTION_BIT)?;
        let direction = r.read_bits(2)? as u8;
        r.set_bit_pos(layer_base + ZIGZAG_BIT)?;
        let zigzag = r.read_bool()?;
        r.set_bit_pos(layer_base + MST_CLK_BIT)?;
        let mst_clk = r.read_bool()?;
        Ok(Self {
            mode,
            direction,
            zigzag,
            mst_clk,
            kb_sync,
            range,
        })
    }

    /// Writes the fields back (the bits between them are left as they are).
    pub(super) fn write(&self, w: &mut BitWriter, layer_base: usize) -> Result<(), CbinError> {
        w.set_bit_pos(layer_base + MODE_BIT)?;
        w.write_bits(u32::from(self.mode & 3), 2)?;
        w.set_bit_pos(layer_base + KB_SYNC_BIT)?;
        w.write_bool(self.kb_sync)?;
        w.set_bit_pos(layer_base + RANGE_BIT)?;
        w.write_bits(u32::from(self.range.min(RANGE_RAW_MAX)), 7)?;
        w.set_bit_pos(layer_base + DIRECTION_BIT)?;
        w.write_bits(u32::from(self.direction & 3), 2)?;
        w.set_bit_pos(layer_base + ZIGZAG_BIT)?;
        w.write_bool(self.zigzag)?;
        w.set_bit_pos(layer_base + MST_CLK_BIT)?;
        w.write_bool(self.mst_clk)
    }

    /// The RANGE dial position in thirds of an octave above one octave, `0..=9`.
    ///
    /// ASSUMED: the raw `0..=127` spreads linearly over the 10 detents of the dial (the same rule
    /// the GUI uses for its knob, `round(n * 9)`); only the two ends are supported by data (0 is
    /// the default of 1099 factory layers, 127 occurs on 5). What would confirm it: the firmware
    /// table behind `Edt_Syn_kArpRange` or a recording of a layer with a mid value.
    pub fn range_thirds(&self) -> u8 {
        let n = f32::from(self.range.min(RANGE_RAW_MAX)) / f32::from(RANGE_RAW_MAX);
        (n * RANGE_MAX_THIRDS).round() as u8
    }

    /// The Gate mode envelope hardness `0..=1` (the RANGE value in Gate mode, ASSUMED linear).
    pub fn gate_env(&self) -> f32 {
        f32::from(self.range.min(RANGE_RAW_MAX)) / f32::from(RANGE_RAW_MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SynthArpDecoded {
        SynthArpDecoded {
            mode: 2,
            direction: 3,
            zigzag: true,
            mst_clk: true,
            kb_sync: true,
            range: 127,
        }
    }

    fn write_at(buf: &mut [u8], base: usize, a: &SynthArpDecoded) {
        let mut w = BitWriter::new(buf);
        a.write(&mut w, base).unwrap();
    }

    #[test]
    fn fields_round_trip_at_the_layer_offsets() {
        let mut buf = vec![0u8; 400];
        let a = sample();
        write_at(&mut buf, 656, &a);
        let mut r = BitReader::new(&buf);
        assert_eq!(SynthArpDecoded::read(&mut r, 656).unwrap(), a);
        // Spot-check the documented absolute positions.
        r.set_bit_pos(656 + 136).unwrap();
        assert_eq!(r.read_bits(2).unwrap(), 2);
        r.set_bit_pos(656 + 139).unwrap();
        assert!(r.read_bool().unwrap());
        assert_eq!(r.read_bits(7).unwrap(), 127);
        r.set_bit_pos(656 + 171).unwrap();
        assert_eq!(r.read_bits(2).unwrap(), 3);
        assert!(r.read_bool().unwrap());
        assert!(r.read_bool().unwrap());
    }

    #[test]
    fn writing_leaves_the_neighbouring_fields_alone() {
        // Run (135), pattern enable (138) and the rate (175) sit next to the mode fields.
        let mut buf = vec![0u8; 400];
        {
            let mut w = BitWriter::new(&mut buf);
            for bit in [135, 138, 175] {
                w.set_bit_pos(248 + bit).unwrap();
                w.write_bool(true).unwrap();
            }
        }
        write_at(&mut buf, 248, &SynthArpDecoded::default());
        let mut r = BitReader::new(&buf);
        for bit in [135, 138, 175] {
            r.set_bit_pos(248 + bit).unwrap();
            assert!(r.read_bool().unwrap(), "bit {bit} was clobbered");
        }
    }

    #[test]
    fn blank_bits_decode_to_the_default_and_write_back_unchanged() {
        let mut buf = vec![0u8; 400];
        let mut r = BitReader::new(&buf);
        let blank = SynthArpDecoded::read(&mut r, 248).unwrap();
        assert_eq!(blank, SynthArpDecoded::default());
        write_at(&mut buf, 248, &blank);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn unused_mode_value_three_is_kept_lossless() {
        let mut buf = vec![0u8; 400];
        let a = SynthArpDecoded {
            mode: 3,
            ..sample()
        };
        write_at(&mut buf, 248, &a);
        let mut r = BitReader::new(&buf);
        assert_eq!(SynthArpDecoded::read(&mut r, 248).unwrap().mode, 3);
    }

    #[test]
    fn range_spreads_over_ten_detents() {
        let at = |range| SynthArpDecoded { range, ..sample() };
        assert_eq!(at(0).range_thirds(), 0);
        assert_eq!(at(127).range_thirds(), 9);
        assert_eq!(at(64).range_thirds(), 5);
        assert_eq!(at(0).gate_env(), 0.0);
        assert_eq!(at(127).gate_env(), 1.0);
    }
}

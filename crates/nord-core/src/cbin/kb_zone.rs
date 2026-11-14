//! Per-layer keyboard zone code (`Edt_Org_kKbZone`, `Edt_Pno_kKbZone`, `Edt_Syn_kKbZone`) of a program.
//!
//! The code is a 4-bit field at the start of every layer block, right before the octave shift:
//! Organ / Piano layers `+8`, Synth layers `+58` (block-relative bits). The enumeration has 12
//! values (`0..=11`, firmware default 5, `docs/research/arm-edit-params.csv`). What a value means is
//! decoded in [`crate::engine`]'s routing module; this file only reads the bits and keeps the
//! round trip lossless (nothing is written back, the stored bits stay in the raw payload).
//!
//! Spec: `docs/specs/cbin-and-presets.md` (layer blocks), `docs/specs/morph-and-zones.md` (zones).

use super::bitstream::BitReader;
use super::container::CbinError;

/// Number of layers in dispatcher order: Organ A, B, Piano A, B, Synth A, B, C.
pub const LAYER_COUNT: usize = 7;

/// Firmware default of `Edt_*_kKbZone` (VERIFIED: `arm-edit-params.csv`, 234 of 384 factory organ A layers store it).
pub const DEFAULT_CODE: u8 = 5;

/// `(program payload byte offset of the section, bit offset of the layer block, bit offset of the field in the block)`.
///
/// VERIFIED (order): the firmware field-name table of the layer buffers lists `KbZone` before
/// `OctaveShift` (`tools/ns4_fw_order_check.py`, 0 problems for the synth layer, `+58`); the organ and
/// piano layers keep the same chain (`+8`, octave at `+12`). EMPIRICAL (offsets): 384 factory
/// programs hold only the codes `0..=10` there.
const FIELDS: [(usize, usize, usize); LAYER_COUNT] = [
    (45, 144, 8),
    (45, 392, 8),
    (180, 104, 8),
    (180, 200, 8),
    (327, 248, 58),
    (327, 656, 58),
    (327, 1064, 58),
];

/// Serde default and `Default` value: all seven layers at the firmware default code.
pub fn default_codes() -> [u8; LAYER_COUNT] {
    [DEFAULT_CODE; LAYER_COUNT]
}

/// Reads the seven layer codes from a 820-byte program payload.
pub fn read_codes(payload: &[u8]) -> Result<[u8; LAYER_COUNT], CbinError> {
    let mut out = default_codes();
    for (code, (section, block, field)) in out.iter_mut().zip(FIELDS) {
        let tail = payload.get(section..).ok_or(CbinError::TooShort {
            actual: payload.len(),
            min: section + 1,
        })?;
        let mut r = BitReader::new(tail);
        r.set_bit_pos(block + field)?;
        *code = r.read_bits(4)? as u8;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(buf: &mut [u8], bit: usize, value: u8) {
        for i in 0..4 {
            let b = (value >> (3 - i)) & 1;
            let pos = bit + i;
            let mask = 0x80u8 >> (pos % 8);
            if b == 1 {
                buf[pos / 8] |= mask;
            } else {
                buf[pos / 8] &= !mask;
            }
        }
    }

    #[test]
    fn default_codes_are_the_firmware_default() {
        assert_eq!(default_codes(), [5; 7]);
    }

    #[test]
    fn codes_are_read_from_the_layer_blocks() {
        let mut payload = vec![0u8; 820];
        for (i, (section, block, field)) in FIELDS.iter().enumerate() {
            put(&mut payload[*section..], block + field, i as u8 + 1);
        }
        assert_eq!(read_codes(&payload).unwrap(), [1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn program_decode_carries_the_codes_and_pack_keeps_the_bits() {
        use crate::cbin::{CbinFile, ProgramDecoded};
        let cbin = ProgramDecoded::default().to_cbin_file(0, 0).unwrap();
        let mut payload = cbin.payload.clone();
        for (i, (section, block, field)) in FIELDS.iter().enumerate() {
            put(
                &mut payload[*section..],
                block + field,
                [0, 6, 4, 9, 1, 2, 3][i],
            );
        }
        let patched = CbinFile {
            header: cbin.header,
            payload,
        };
        let prog = ProgramDecoded::from_cbin_file(&patched).unwrap();
        assert_eq!(prog.global.kb_zone, [0, 6, 4, 9, 1, 2, 3]);
        // the decoded program re-packs with its stored bits (raw payload preserved)
        let again = prog.to_cbin_file(0, 0).unwrap();
        assert_eq!(read_codes(&again.payload).unwrap(), [0, 6, 4, 9, 1, 2, 3]);
    }

    #[test]
    fn short_payload_is_an_error() {
        assert!(read_codes(&[0u8; 10]).is_err());
    }
}

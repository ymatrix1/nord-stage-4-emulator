//! Big-endian bit-level reader and writer for Clavia Binary bitstreams.

use super::container::CbinError;

/// A sequential bit-level reader over a big-endian packed byte slice.
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    pub fn bit_pos(&self) -> usize {
        self.bit_pos
    }

    /// Sets the bit reader cursor position, validating against total payload bit count.
    pub fn set_bit_pos(&mut self, pos: usize) -> Result<(), CbinError> {
        let total_bits = self.data.len().checked_mul(8).ok_or_else(|| {
            CbinError::BitstreamError("bitstream data length overflow".to_string())
        })?;
        if pos > total_bits {
            return Err(CbinError::BitstreamError(format!(
                "cursor seek {pos} out of bounds (total bits {total_bits})"
            )));
        }
        self.bit_pos = pos;
        Ok(())
    }

    pub fn read_bits(&mut self, count: usize) -> Result<u32, CbinError> {
        if count > 32 {
            return Err(CbinError::BitstreamError(format!(
                "cannot read {count} bits into u32 (maximum 32)"
            )));
        }
        let total_bits = self.data.len().checked_mul(8).ok_or_else(|| {
            CbinError::BitstreamError("bitstream data length overflow".to_string())
        })?;
        let end_pos = self.bit_pos.checked_add(count).ok_or_else(|| {
            CbinError::BitstreamError("bitstream read cursor position overflow".to_string())
        })?;
        if end_pos > total_bits {
            return Err(CbinError::BitstreamError(format!(
                "bitstream overrun: pos {} + count {} > total bits {}",
                self.bit_pos, count, total_bits
            )));
        }
        let mut val = 0u32;
        for _ in 0..count {
            let byte_idx = self.bit_pos / 8;
            let bit_idx = 7 - (self.bit_pos % 8);
            let bit = (self.data[byte_idx] >> bit_idx) & 1;
            val = (val << 1) | u32::from(bit);
            self.bit_pos += 1;
        }
        Ok(val)
    }

    pub fn read_bool(&mut self) -> Result<bool, CbinError> {
        self.read_bits(1).map(|v| v != 0)
    }
}

/// A sequential bit-level writer into a pre-allocated big-endian packed byte buffer.
pub struct BitWriter<'a> {
    data: &'a mut [u8],
    bit_pos: usize,
}

impl<'a> BitWriter<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    pub fn bit_pos(&self) -> usize {
        self.bit_pos
    }

    /// Sets the bit writer cursor position, validating against total buffer bit count.
    pub fn set_bit_pos(&mut self, pos: usize) -> Result<(), CbinError> {
        let total_bits = self.data.len().checked_mul(8).ok_or_else(|| {
            CbinError::BitstreamError("bitstream data length overflow".to_string())
        })?;
        if pos > total_bits {
            return Err(CbinError::BitstreamError(format!(
                "writer seek {pos} out of bounds (total bits {total_bits})"
            )));
        }
        self.bit_pos = pos;
        Ok(())
    }

    pub fn write_bits(&mut self, value: u32, count: usize) -> Result<(), CbinError> {
        if count > 32 {
            return Err(CbinError::BitstreamError(format!(
                "cannot write {count} bits from u32"
            )));
        }
        let total_bits = self.data.len().checked_mul(8).ok_or_else(|| {
            CbinError::BitstreamError("bitstream data length overflow".to_string())
        })?;
        let end_pos = self.bit_pos.checked_add(count).ok_or_else(|| {
            CbinError::BitstreamError("bitstream write cursor position overflow".to_string())
        })?;
        if end_pos > total_bits {
            return Err(CbinError::BitstreamError(format!(
                "bitstream write overrun: pos {} + count {} > total bits {}",
                self.bit_pos, count, total_bits
            )));
        }
        for i in 0..count {
            let shift = count - 1 - i;
            let bit = ((value >> shift) & 1) as u8;
            let byte_idx = self.bit_pos / 8;
            let bit_idx = 7 - (self.bit_pos % 8);
            if bit != 0 {
                self.data[byte_idx] |= 1 << bit_idx;
            } else {
                self.data[byte_idx] &= !(1 << bit_idx);
            }
            self.bit_pos += 1;
        }
        Ok(())
    }

    pub fn write_bool(&mut self, val: bool) -> Result<(), CbinError> {
        self.write_bits(if val { 1 } else { 0 }, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_reader_and_writer_roundtrip() {
        let mut buf = [0u8; 8];
        let mut w = BitWriter::new(&mut buf);
        w.write_bits(0b101, 3).unwrap();
        w.write_bool(true).unwrap();
        w.write_bits(0xAB, 8).unwrap();
        w.write_bits(0x1234, 16).unwrap();

        let mut r = BitReader::new(&buf);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert!(r.read_bool().unwrap());
        assert_eq!(r.read_bits(8).unwrap(), 0xAB);
        assert_eq!(r.read_bits(16).unwrap(), 0x1234);
    }

    #[test]
    fn test_bit_reader_and_writer_bounds() {
        let mut buf = [0u8; 2];
        let mut w = BitWriter::new(&mut buf);
        assert!(w.write_bits(0xFFFF, 16).is_ok());
        assert!(w.write_bool(false).is_err()); // overrun

        let mut r = BitReader::new(&buf);
        assert!(r.read_bits(16).is_ok());
        assert!(r.read_bits(1).is_err()); // overrun
        assert!(r.read_bits(33).is_err()); // count > 32
    }

    #[test]
    fn test_bit_cursor_seek_bounds_and_overflow() {
        let mut buf = [0u8; 4];
        let mut r = BitReader::new(&buf);
        assert!(r.set_bit_pos(32).is_ok());
        assert!(r.set_bit_pos(33).is_err());
        assert!(r.set_bit_pos(usize::MAX).is_err());

        let mut w = BitWriter::new(&mut buf);
        assert!(w.set_bit_pos(32).is_ok());
        assert!(w.set_bit_pos(33).is_err());
        assert!(w.set_bit_pos(usize::MAX).is_err());
    }
}

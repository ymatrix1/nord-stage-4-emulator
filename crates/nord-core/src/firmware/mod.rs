//! Nord Stage 4 OS Update Firmware Container & SHARC DSP Partition Parser.
//!
//! Spec: `docs/specs/cbin-and-presets.md` §5, `docs/research/ns4-firmware-teardown.md`

mod lz;
mod lz_compress;
mod parse;
#[cfg(test)]
mod support;
#[cfg(test)]
mod tests_container;
#[cfg(test)]
mod tests_lz;

pub use lz::decompress_clavia_lz;
pub use lz_compress::compress_clavia_lz;

/// Fixed 32-byte (`0x20`) top-level dual-image firmware header size.
pub const FIRMWARE_TOP_HEADER_SIZE: usize = 32;

/// Calculates Clavia's one's-complement byte checksum: `0xFFFFFFFF - sum(bytes)`.
///
/// Verified on AM1705 firmware container segments (`SRAM`, `ARM0`, and uncompressed `CODE`).
#[inline]
pub fn calculate_checksum(bytes: &[u8]) -> u32 {
    let sum: u32 = bytes
        .iter()
        .fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
    0xFFFF_FFFFu32.wrapping_sub(sum)
}

/// Functional categorization of embedded DSP partitions and hardware coprocessor targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspEngineType {
    /// SHARC Organ Physical Modeling and Rotary 122 DSP (`DSPO` / `DSPOrg`).
    Organ,
    /// SHARC 3-Layer Nord Wave 2 Synth DSP (`DSPS` / `DSPSyn`).
    Synth,
    /// SHARC Per-Layer Effects and String Resonance DSP (`DSPF` / `DSPFxCore1`).
    Effects,
    /// Inter-DSP SPORT bus routing descriptor (`Fx to Org`, `to Fx`, `Host`, etc.).
    Routing,
    /// Dedicated DSP or host engine domain tag (`ORG`, `PNO`, `SMP`, `SY1`, `SYL3`).
    CoreIdentifier,
}

/// Descriptor for a firmware memory segment (`SRAM`, `CODE`, `ARM0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareSegment {
    pub name: String,
    pub bank: u8,
    pub data_offset: u32,
    pub data_size: u32,
    pub target_address: u32,
    pub checksum: u32,
}

impl FirmwareSegment {
    /// Extracts and decompresses the segment payload from raw firmware bytes.
    ///
    /// For compressed segments (e.g. `CODE`), `self.data_size` represents the declared
    /// expanded memory size at runtime, and `decompress_clavia_lz` decompresses the
    /// stream up to `self.data_size`.
    pub fn extract_payload(&self, raw: &[u8]) -> Result<Vec<u8>, String> {
        let start = self.data_offset as usize;
        if start >= raw.len() {
            return Err(format!(
                "segment {} start offset 0x{:X} exceeds firmware size 0x{:X}",
                self.name,
                start,
                raw.len()
            ));
        }
        let slice = &raw[start..];
        if self.name == "CODE" {
            decompress_clavia_lz(slice, self.data_size as usize)
        } else {
            let end = start.saturating_add(self.data_size as usize);
            if end > raw.len() {
                return Err(format!(
                    "segment {} exceeds firmware boundary: 0x{:X}..0x{:X} > 0x{:X}",
                    self.name,
                    start,
                    end,
                    raw.len()
                ));
            }
            Ok(raw[start..end].to_vec())
        }
    }

    /// Verifies if the payload matches the segment's recorded one's-complement checksum.
    pub fn verify_checksum(&self, payload: &[u8]) -> bool {
        calculate_checksum(payload) == self.checksum
    }
}

/// Discovered SHARC DSP or Host processor partition tag within a firmware image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DspPartition {
    pub tag: String,
    pub offset: usize,
    pub description: &'static str,
    pub engine_type: DspEngineType,
}

/// Parsed Nord Stage 4 OS firmware container (`RCDATA 101` payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareContainer {
    pub header_size: u32,
    pub image0_size: u32,
    pub image1_offset: u32,
    pub image1_size: u32,
    pub image0_segments: Vec<FirmwareSegment>,
    pub image1_segments: Vec<FirmwareSegment>,
    pub segments: Vec<FirmwareSegment>,
    pub dsp_partitions: Vec<DspPartition>,
}

impl FirmwareContainer {
    /// Finds a segment by name in the default Bank 0 (e.g. "SRAM", "CODE", "ARM0").
    pub fn segment(&self, name: &str) -> Option<&FirmwareSegment> {
        self.segments.iter().find(|s| s.name == name)
    }

    /// Finds a segment by bank index and name.
    pub fn segment_in_bank(&self, bank: u8, name: &str) -> Option<&FirmwareSegment> {
        match bank {
            0 => self.image0_segments.iter().find(|s| s.name == name),
            1 => self.image1_segments.iter().find(|s| s.name == name),
            _ => None,
        }
    }

    /// Returns the base byte offset in `raw` for the given firmware bank.
    pub fn bank_base(&self, bank: u8) -> Option<usize> {
        match bank {
            0 => Some(self.header_size as usize),
            1 => Some(self.image1_offset as usize),
            _ => None,
        }
    }

    /// Extracts and decompresses a segment from `raw` relative to its bank base.
    pub fn extract_segment(&self, bank: u8, name: &str, raw: &[u8]) -> Result<Vec<u8>, String> {
        let seg = self
            .segment_in_bank(bank, name)
            .ok_or_else(|| format!("segment '{name}' not found in bank {bank}"))?;
        let base = self
            .bank_base(bank)
            .ok_or_else(|| format!("invalid firmware bank {bank}"))?;
        if base > raw.len() {
            return Err(format!(
                "bank {bank} base offset 0x{base:X} exceeds firmware size 0x{:X}",
                raw.len()
            ));
        }
        seg.extract_payload(&raw[base..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_ones_complement_of_the_byte_sum() {
        assert_eq!(calculate_checksum(&[]), 0xFFFF_FFFF);
        assert_eq!(calculate_checksum(&[1, 2, 3]), 0xFFFF_FFF9);
        assert_eq!(FIRMWARE_TOP_HEADER_SIZE, 32);
    }
}

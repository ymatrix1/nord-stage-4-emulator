//! Nord Stage 4 OS Update Firmware Container & SHARC DSP Partition Parser.
//!
//! Spec: `docs/specs/cbin-and-presets.md` §5, `docs/research/ns4-firmware-teardown.md`

/// Fixed 32-byte (`0x20`) top-level dual-image firmware header size.
pub const FIRMWARE_TOP_HEADER_SIZE: usize = 32;

/// Descriptor for a firmware memory segment (`SRAM`, `CODE`, `ARM0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareSegment {
    pub name: String,
    pub data_offset: u32,
    pub data_size: u32,
    pub target_address: u32,
    pub checksum: u32,
}

/// Discovered SHARC DSP or Host processor partition tag within a firmware image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DspPartition {
    pub tag: String,
    pub offset: usize,
    pub description: &'static str,
}

/// Parsed Nord Stage 4 OS firmware container (`RCDATA 101` payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareContainer {
    pub header_size: u32,
    pub image0_size: u32,
    pub image1_offset: u32,
    pub image1_size: u32,
    pub segments: Vec<FirmwareSegment>,
    pub dsp_partitions: Vec<DspPartition>,
}

impl FirmwareContainer {
    /// Parses the top-level dual-image firmware header, segment descriptors, and SHARC DSP tags.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < FIRMWARE_TOP_HEADER_SIZE {
            return Err(format!(
                "firmware payload too short: {} bytes (minimum {})",
                bytes.len(),
                FIRMWARE_TOP_HEADER_SIZE
            ));
        }

        let header_size = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let image0_size = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let image1_offset = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        let image1_size = u32::from_be_bytes(bytes[12..16].try_into().unwrap());

        if header_size != 0x20 {
            return Err(format!(
                "unexpected firmware header_size: 0x{header_size:X}"
            ));
        }
        if header_size.saturating_add(image0_size) != image1_offset {
            return Err(format!(
                "image1_offset (0x{image1_offset:X}) != header_size + image0_size"
            ));
        }

        let mut segments = Vec::new();
        // Image 0 segment table starts at offset 0x20 + 0x14 (after 20-byte image sub-header)
        if bytes.len() >= 0x20 + 20 {
            let seg_count = u32::from_be_bytes(bytes[0x30..0x34].try_into().unwrap()) as usize;
            let mut cursor = 0x34usize;
            for _ in 0..seg_count.min(8) {
                if cursor + 44 > bytes.len() {
                    break;
                }
                let tag_bytes = &bytes[cursor..cursor + 4];
                if let Ok(name) = std::str::from_utf8(tag_bytes) {
                    let data_offset =
                        u32::from_be_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap());
                    let data_size =
                        u32::from_be_bytes(bytes[cursor + 8..cursor + 12].try_into().unwrap());
                    let target_address =
                        u32::from_be_bytes(bytes[cursor + 12..cursor + 16].try_into().unwrap());
                    let checksum =
                        u32::from_be_bytes(bytes[cursor + 16..cursor + 20].try_into().unwrap());
                    segments.push(FirmwareSegment {
                        name: name.to_string(),
                        data_offset,
                        data_size,
                        target_address,
                        checksum,
                    });
                }
                cursor += 44;
            }
        }

        let mut dsp_partitions = Vec::new();
        let known_tags: &[(&[u8; 4], &'static str)] = &[
            (
                b"DSPO",
                "SHARC Organ Physical Modeling & Rotary 122 DSP (DSPOrg)",
            ),
            (b"DSPS", "SHARC 3-Layer Nord Wave 2 Synth DSP (DSPSyn)"),
            (
                b"DSPF",
                "SHARC Per-Layer Effects & String Resonance DSP (DSPFxCore1)",
            ),
        ];

        for &(tag, description) in known_tags {
            if let Some(pos) = bytes.windows(4).position(|w| w == tag) {
                dsp_partitions.push(DspPartition {
                    tag: String::from_utf8_lossy(tag).into_owned(),
                    offset: pos,
                    description,
                });
            }
        }

        Ok(Self {
            header_size,
            image0_size,
            image1_offset,
            image1_size,
            segments,
            dsp_partitions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_firmware() -> Vec<u8> {
        let mut buf = vec![0u8; 512];
        // Top-level header (0x20 bytes)
        buf[0..4].copy_from_slice(&0x0000_0020u32.to_be_bytes());
        buf[4..8].copy_from_slice(&0x0000_0140u32.to_be_bytes()); // image0_size = 320
        buf[8..12].copy_from_slice(&0x0000_0160u32.to_be_bytes()); // image1_offset = 0x20 + 0x140
        buf[12..16].copy_from_slice(&0x0000_00A0u32.to_be_bytes()); // image1_size = 160

        // Image 0 sub-header at 0x20..0x34: segment_count = 3 at 0x30..0x34
        buf[0x30..0x34].copy_from_slice(&3u32.to_be_bytes());

        // 3 segments of 44 bytes starting at 0x34: SRAM, CODE, ARM0
        for (i, name) in [b"SRAM", b"CODE", b"ARM0"].iter().enumerate() {
            let off = 0x34 + i * 44;
            buf[off..off + 4].copy_from_slice(*name);
            buf[off + 4..off + 8].copy_from_slice(&0x02D4u32.to_be_bytes());
            buf[off + 8..off + 12].copy_from_slice(&0xFC80u32.to_be_bytes());
            buf[off + 12..off + 16].copy_from_slice(&0x8000_0000u32.to_be_bytes());
            buf[off + 16..off + 20].copy_from_slice(&0xFFF3_1619u32.to_be_bytes());
        }

        // Embed SHARC DSP partition markers
        buf[260..268].copy_from_slice(b"DSPFxCor");
        buf[280..286].copy_from_slice(b"DSPOrg");
        buf[300..306].copy_from_slice(b"DSPSyn");
        buf
    }

    #[test]
    fn test_parse_firmware_container_and_sharc_tags() {
        let raw = build_synthetic_firmware();
        let fw = FirmwareContainer::parse(&raw).expect("synthetic firmware should parse");
        assert_eq!(fw.header_size, 0x20);
        assert_eq!(fw.image0_size, 0x140);
        assert_eq!(fw.image1_offset, 0x160);
        assert_eq!(fw.segments.len(), 3);
        assert_eq!(fw.segments[0].name, "SRAM");
        assert_eq!(fw.segments[1].name, "CODE");
        assert_eq!(fw.segments[2].name, "ARM0");

        let tags: Vec<&str> = fw.dsp_partitions.iter().map(|p| p.tag.as_str()).collect();
        assert_eq!(tags, vec!["DSPO", "DSPS", "DSPF"]);
    }

    #[test]
    fn test_rejects_invalid_firmware_header() {
        assert!(FirmwareContainer::parse(&[0u8; 8]).is_err());
        let mut bad = build_synthetic_firmware();
        bad[3] = 0x99;
        assert!(FirmwareContainer::parse(&bad).is_err());
    }
}

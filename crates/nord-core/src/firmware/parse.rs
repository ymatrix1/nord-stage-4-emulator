//! `FirmwareContainer::parse`: dual-image header, segment table and SHARC DSP partition scan.

use super::*;

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

        // Parse Image 0 segments (bank 0)
        let mut image0_segments = Vec::new();
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
                    image0_segments.push(FirmwareSegment {
                        name: name.to_string(),
                        bank: 0,
                        data_offset,
                        data_size,
                        target_address,
                        checksum,
                    });
                }
                cursor += 44;
            }
        }

        // Parse Image 1 segments (bank 1)
        let mut image1_segments = Vec::new();
        let img1_base = image1_offset as usize;
        if bytes.len() >= img1_base + 20 {
            let seg_count = u32::from_be_bytes(
                bytes[img1_base + 0x10..img1_base + 0x14]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let mut cursor = img1_base + 0x14;
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
                    image1_segments.push(FirmwareSegment {
                        name: name.to_string(),
                        bank: 1,
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
        let known_tags: &[(&[u8], &'static str, DspEngineType)] = &[
            (
                b"DSPO",
                "SHARC Organ Physical Modeling & Rotary 122 DSP (DSPOrg)",
                DspEngineType::Organ,
            ),
            (
                b"DSPOrg",
                "SHARC Organ Physical Modeling Core Symbol (DSPOrg)",
                DspEngineType::Organ,
            ),
            (
                b"DSPS",
                "SHARC 3-Layer Nord Wave 2 Synth DSP (DSPSyn)",
                DspEngineType::Synth,
            ),
            (
                b"DSPSyn",
                "SHARC 3-Layer Synth Core Symbol (DSPSyn)",
                DspEngineType::Synth,
            ),
            (
                b"DSPF",
                "SHARC Per-Layer Effects & String Resonance DSP (DSPFxCore1)",
                DspEngineType::Effects,
            ),
            (
                b"DSPFxCore1",
                "SHARC Effects Pipeline Core Symbol (DSPFxCore1)",
                DspEngineType::Effects,
            ),
            (
                b"Fx to Org",
                "SPORT Bus Routing: Effects to Organ",
                DspEngineType::Routing,
            ),
            (
                b"to Fx",
                "SPORT Bus Routing: Inter-core to Effects",
                DspEngineType::Routing,
            ),
            (
                b"Host",
                "AM1705 Host Processor SPORT Bus Routing",
                DspEngineType::Routing,
            ),
            (
                b"ORG",
                "B3 Organ Physical Model Domain Tag",
                DspEngineType::CoreIdentifier,
            ),
            (
                b"PNO",
                "Grand / Upright Piano Acoustic Model Domain Tag",
                DspEngineType::CoreIdentifier,
            ),
            (
                b"SMP",
                "Sample Library Synth Domain Tag",
                DspEngineType::CoreIdentifier,
            ),
            (
                b"SY1",
                "Wave 2 Synth Layer 1 Domain Tag",
                DspEngineType::CoreIdentifier,
            ),
            (
                b"SYL3",
                "Wave 2 Synth Layer 3 Domain Tag",
                DspEngineType::CoreIdentifier,
            ),
        ];

        for &(tag, description, engine_type) in known_tags {
            if let Some(pos) = bytes.windows(tag.len()).position(|w| w == tag) {
                dsp_partitions.push(DspPartition {
                    tag: String::from_utf8_lossy(tag).into_owned(),
                    offset: pos,
                    description,
                    engine_type,
                });
            }
        }

        let segments = image0_segments.clone();

        Ok(Self {
            header_size,
            image0_size,
            image1_offset,
            image1_size,
            image0_segments,
            image1_segments,
            segments,
            dsp_partitions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_empty_input() {
        assert!(FirmwareContainer::parse(&[]).is_err());
    }
}

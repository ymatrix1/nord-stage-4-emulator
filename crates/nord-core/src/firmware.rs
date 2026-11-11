//! Nord Stage 4 OS Update Firmware Container & SHARC DSP Partition Parser.
//!
//! Spec: `docs/specs/cbin-and-presets.md` §5, `docs/research/ns4-firmware-teardown.md`

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
}

/// Decompresses a byte slice encoded with Clavia's custom 16-bit LZ token compression format.
///
/// Match token layout (16-bit little-endian word):
/// - Bits 2..0: Match length minus 2 (range: 2..9 bytes)
/// - Bits 15..3: Back-reference distance in history buffer (range: 1..8191 bytes)
///
/// Stream framing uses a flag byte before every group of up to 8 items:
/// - Bit = 1: Uncompressed literal byte (1 byte follows)
/// - Bit = 0: 16-bit match token (2 little-endian bytes follow)
pub fn decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(max_output_size.min(1024 * 1024));
    let mut in_pos = 0;

    // Extended literal run prefix at stream start (e.g. 0x00, L where run length is 17 + L bytes)
    if input.len() >= 2 && input[0] == 0x00 && input[1] > 0 {
        let run_len = 17 + input[1] as usize;
        if 2 + run_len <= input.len() {
            let copy_len = run_len.min(max_output_size);
            out.extend_from_slice(&input[2..2 + copy_len]);
            in_pos = 2 + copy_len;
        }
    }

    while in_pos < input.len() && out.len() < max_output_size {
        let flag = input[in_pos];
        in_pos += 1;

        for bit in 0..8 {
            if in_pos >= input.len() || out.len() >= max_output_size {
                break;
            }

            let is_literal = ((flag >> bit) & 1) != 0;
            if is_literal {
                out.push(input[in_pos]);
                in_pos += 1;
            } else {
                if in_pos + 2 > input.len() {
                    return Err(format!(
                        "unexpected end of stream reading LZ token at input offset 0x{in_pos:X}"
                    ));
                }
                let token = u16::from_le_bytes([input[in_pos], input[in_pos + 1]]);
                in_pos += 2;

                let length = (token & 0x07) as usize + 2;
                let distance = (token >> 3) as usize;

                if distance == 0 {
                    return Err("invalid LZ distance 0".to_string());
                }
                if distance > out.len() {
                    return Err(format!(
                        "LZ distance {distance} exceeds current output buffer length {}",
                        out.len()
                    ));
                }

                let start = out.len() - distance;
                for i in 0..length {
                    if out.len() >= max_output_size {
                        break;
                    }
                    let byte = out[start + i];
                    out.push(byte);
                }
            }
        }
    }

    Ok(out)
}

/// Compresses raw data into Clavia's custom 16-bit LZ token format.
pub fn compress_clavia_lz(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut in_pos = 0;

    while in_pos < input.len() {
        let mut commands: Vec<(bool, u8, u16)> = Vec::with_capacity(8);
        for _ in 0..8 {
            if in_pos >= input.len() {
                break;
            }

            let win_start = in_pos.saturating_sub(8191);
            let mut best_len = 0;
            let mut best_dist = 0;

            for start in win_start..in_pos {
                let dist = in_pos - start;
                let mut len = 0;
                while in_pos + len < input.len()
                    && len < 9
                    && input[start + len] == input[in_pos + len]
                {
                    len += 1;
                }
                if len >= 2 && len > best_len {
                    best_len = len;
                    best_dist = dist;
                    if len == 9 {
                        break;
                    }
                }
            }

            if best_len >= 2 {
                let token = ((best_dist as u16) << 3) | ((best_len - 2) as u16 & 0x07);
                commands.push((false, 0u8, token));
                in_pos += best_len;
            } else {
                commands.push((true, input[in_pos], 0u16));
                in_pos += 1;
            }
        }

        let mut flag = 0u8;
        for (bit, &(is_lit, _, _)) in commands.iter().enumerate() {
            if is_lit {
                flag |= 1 << bit;
            }
        }
        out.push(flag);

        for &(is_lit, byte, token) in &commands {
            if is_lit {
                out.push(byte);
            } else {
                out.extend_from_slice(&token.to_le_bytes());
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_firmware() -> Vec<u8> {
        let mut buf = vec![0u8; 1024];
        // Top-level header (0x20 bytes)
        buf[0..4].copy_from_slice(&0x0000_0020u32.to_be_bytes());
        buf[4..8].copy_from_slice(&0x0000_0140u32.to_be_bytes()); // image0_size = 320
        buf[8..12].copy_from_slice(&0x0000_0160u32.to_be_bytes()); // image1_offset = 0x20 + 0x140 = 0x160
        buf[12..16].copy_from_slice(&0x0000_0100u32.to_be_bytes()); // image1_size = 256

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

        // Image 1 sub-header at 0x160..0x174: segment_count = 3 at 0x170..0x174
        buf[0x170..0x174].copy_from_slice(&3u32.to_be_bytes());

        // 3 segments of 44 bytes starting at 0x174: SRAM, CODE, ARM0
        for (i, name) in [b"SRAM", b"CODE", b"ARM0"].iter().enumerate() {
            let off = 0x174 + i * 44;
            buf[off..off + 4].copy_from_slice(*name);
            buf[off + 4..off + 8].copy_from_slice(&0x02D4u32.to_be_bytes());
            buf[off + 8..off + 12].copy_from_slice(&0xC158u32.to_be_bytes());
            buf[off + 12..off + 16].copy_from_slice(&0x8000_0000u32.to_be_bytes());
            buf[off + 16..off + 20].copy_from_slice(&0xFFED_0AE8u32.to_be_bytes());
        }

        // Embed SHARC DSP partition markers
        buf[600..608].copy_from_slice(b"DSPFxCor");
        buf[610..620].copy_from_slice(b"DSPFxCore1");
        buf[625..631].copy_from_slice(b"DSPOrg");
        buf[640..646].copy_from_slice(b"DSPSyn");
        buf[650..659].copy_from_slice(b"Fx to Org");
        buf[665..670].copy_from_slice(b"to Fx");
        buf[675..679].copy_from_slice(b"Host");
        buf[685..688].copy_from_slice(b"ORG");
        buf[690..693].copy_from_slice(b"PNO");
        buf[695..698].copy_from_slice(b"SMP");
        buf[700..703].copy_from_slice(b"SY1");
        buf[705..709].copy_from_slice(b"SYL3");
        buf
    }

    #[test]
    fn test_parse_firmware_container_and_sharc_tags() {
        let raw = build_synthetic_firmware();
        let fw = FirmwareContainer::parse(&raw).expect("synthetic firmware should parse");
        assert_eq!(fw.header_size, 0x20);
        assert_eq!(fw.image0_size, 0x140);
        assert_eq!(fw.image1_offset, 0x160);
        assert_eq!(fw.image0_segments.len(), 3);
        assert_eq!(fw.image1_segments.len(), 3);
        assert_eq!(fw.segments.len(), 3);
        assert_eq!(fw.segments[0].name, "SRAM");
        assert_eq!(fw.segments[1].name, "CODE");
        assert_eq!(fw.segments[2].name, "ARM0");

        let sram = fw.segment("SRAM").expect("SRAM segment should exist");
        assert_eq!(sram.target_address, 0x8000_0000);
        assert_eq!(sram.bank, 0);
        assert!(fw.segment("NON_EXISTENT").is_none());

        let img1_code = fw
            .segment_in_bank(1, "CODE")
            .expect("Image 1 CODE segment should exist");
        assert_eq!(img1_code.bank, 1);
        assert_eq!(img1_code.data_size, 0xC158);

        let tags: Vec<&str> = fw.dsp_partitions.iter().map(|p| p.tag.as_str()).collect();
        assert!(tags.contains(&"DSPO"));
        assert!(tags.contains(&"DSPS"));
        assert!(tags.contains(&"DSPF"));
        assert!(tags.contains(&"DSPOrg"));
        assert!(tags.contains(&"DSPSyn"));
        assert!(tags.contains(&"DSPFxCore1"));
        assert!(tags.contains(&"Fx to Org"));
        assert!(tags.contains(&"Host"));
        assert!(tags.contains(&"ORG"));
        assert!(tags.contains(&"PNO"));
        assert!(tags.contains(&"SMP"));
    }

    #[test]
    fn test_calculate_checksum_and_verify() {
        // Deterministic synthetic buffer
        let data = vec![0x10u8, 0x20u8, 0x30u8, 0x40u8];
        let csum = calculate_checksum(&data);
        assert_eq!(csum, 0xFFFF_FFFF - (0x10 + 0x20 + 0x30 + 0x40));

        let seg = FirmwareSegment {
            name: "TEST".to_string(),
            bank: 0,
            data_offset: 0,
            data_size: 4,
            target_address: 0,
            checksum: csum,
        };
        assert!(seg.verify_checksum(&data));
        assert!(!seg.verify_checksum(&[0x10, 0x20, 0x30, 0x41]));
    }

    #[test]
    fn test_dsp_partitions_all_engines() {
        let raw = build_synthetic_firmware();
        let fw = FirmwareContainer::parse(&raw).expect("synthetic firmware should parse");

        let organ_part = fw
            .dsp_partitions
            .iter()
            .find(|p| p.tag == "DSPOrg")
            .expect("DSPOrg partition should exist");
        assert_eq!(organ_part.engine_type, DspEngineType::Organ);

        let synth_part = fw
            .dsp_partitions
            .iter()
            .find(|p| p.tag == "DSPSyn")
            .expect("DSPSyn partition should exist");
        assert_eq!(synth_part.engine_type, DspEngineType::Synth);

        let fx_part = fw
            .dsp_partitions
            .iter()
            .find(|p| p.tag == "DSPFxCore1")
            .expect("DSPFxCore1 partition should exist");
        assert_eq!(fx_part.engine_type, DspEngineType::Effects);

        let route_part = fw
            .dsp_partitions
            .iter()
            .find(|p| p.tag == "Fx to Org")
            .expect("Fx to Org routing should exist");
        assert_eq!(route_part.engine_type, DspEngineType::Routing);

        let core_part = fw
            .dsp_partitions
            .iter()
            .find(|p| p.tag == "PNO")
            .expect("PNO core should exist");
        assert_eq!(core_part.engine_type, DspEngineType::CoreIdentifier);
    }

    #[test]
    fn test_rejects_invalid_firmware_header() {
        assert!(FirmwareContainer::parse(&[0u8; 8]).is_err());
        let mut bad = build_synthetic_firmware();
        bad[3] = 0x99;
        assert!(FirmwareContainer::parse(&bad).is_err());
    }

    #[test]
    fn test_clavia_lz_roundtrip_synthetic() {
        let original = b"Nord Stage 4 OS Firmware LZ Token Compressor/Decompressor Verification Test! Repeating: 123456789 123456789 123456789 AAAAAAAAAAAAAAAAAAAA";
        let compressed = compress_clavia_lz(original);
        assert!(compressed.len() < original.len());

        let decompressed = decompress_clavia_lz(&compressed, original.len() * 2)
            .expect("decompression should succeed");
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_clavia_lz_token_bit_structure() {
        // Test token structure explicitly:
        // token: length = 7 (bits 2..0 = 5), distance = 89 (bits 15..3 = 89)
        // w = (89 << 3) | 5 = 0x02CD
        let dist = 89u16;
        let len = 7u16;
        let token = (dist << 3) | ((len - 2) & 0x07);
        assert_eq!(token, 0x02CD);

        let decoded_len = (token & 0x07) + 2;
        let decoded_dist = token >> 3;
        assert_eq!(decoded_len, 7);
        assert_eq!(decoded_dist, 89);
    }

    #[test]
    fn test_clavia_lz_bounds_protection() {
        // Distance 0 should fail
        let mut bad_token = vec![0x00u8]; // Flag: bit 0 = match
        bad_token.extend_from_slice(&0x0000u16.to_le_bytes()); // distance 0, length 2
        assert!(decompress_clavia_lz(&bad_token, 100).is_err());

        // Distance exceeding output length should fail
        let mut bad_dist = vec![0x00u8];
        bad_dist.extend_from_slice(&0x0100u16.to_le_bytes()); // distance 32 > out.len() (0)
        assert!(decompress_clavia_lz(&bad_dist, 100).is_err());
    }

    #[test]
    fn test_extract_payload_bounds() {
        let seg = FirmwareSegment {
            name: "SRAM".to_string(),
            bank: 0,
            data_offset: 500,
            data_size: 100,
            target_address: 0x8000_0000,
            checksum: 0,
        };
        let small_buf = vec![0u8; 200];
        assert!(seg.extract_payload(&small_buf).is_err());
    }

    #[test]
    fn test_clavia_lz_empty_and_single_byte() {
        let empty: &[u8] = b"";
        let c_empty = compress_clavia_lz(empty);
        let d_empty = decompress_clavia_lz(&c_empty, 10).expect("empty should decompress");
        assert_eq!(d_empty, empty);

        let single = b"Z";
        let c_single = compress_clavia_lz(single);
        let d_single = decompress_clavia_lz(&c_single, 10).expect("single should decompress");
        assert_eq!(d_single, single);
    }

    #[test]
    fn test_clavia_lz_all_match_lengths_2_through_9() {
        for target_len in 2..=9 {
            let mut data = vec![b'B'; 30];
            // Introduce unique prefix then repeating pattern of length target_len
            let pattern = vec![b'X'; target_len];
            data.extend_from_slice(&pattern);
            data.extend_from_slice(&pattern); // repeat to trigger match

            let compressed = compress_clavia_lz(&data);
            let decompressed = decompress_clavia_lz(&compressed, data.len() * 2)
                .expect("decompression of match length test must succeed");
            assert_eq!(
                decompressed, data,
                "failed roundtrip for match length {target_len}"
            );
        }
    }

    #[test]
    fn test_extract_payload_code_segment_decompression() {
        let raw_code = b"ARM926EJ-S startup code payload with repeating patterns: 00 80 e3 00 f0 21 e1 00 80 e3 00 f0 21 e1";
        let compressed_code = compress_clavia_lz(raw_code);

        // Build a buffer with a CODE segment starting at offset 64
        let offset = 64usize;
        let mut firmware_raw = vec![0u8; offset + compressed_code.len() + 32];
        firmware_raw[offset..offset + compressed_code.len()].copy_from_slice(&compressed_code);

        let code_seg = FirmwareSegment {
            name: "CODE".to_string(),
            bank: 0,
            data_offset: offset as u32,
            data_size: raw_code.len() as u32, // Declared expanded target size in RAM
            target_address: 0xC000_0000,
            checksum: 0x1234_5678,
        };

        let decompressed = code_seg
            .extract_payload(&firmware_raw)
            .expect("extract_payload on CODE segment should decompress successfully");
        assert_eq!(decompressed, raw_code);
    }

    #[test]
    fn test_clavia_lz_extended_literal_run() {
        // Construct an input stream starting with 0x00, 0x1B (run length = 17 + 27 = 44 bytes)
        let mut stream = vec![0x00u8, 0x1Bu8];
        let literal_bytes = vec![0xA5u8; 44];
        stream.extend_from_slice(&literal_bytes);

        let out = decompress_clavia_lz(&stream, 100)
            .expect("extended literal run decompression should succeed");
        assert_eq!(out, literal_bytes);
    }

    #[test]
    fn test_clavia_lz_truncated_token_error() {
        // Flag byte indicating a match token (bit 0 = 0), but only 1 byte follows instead of 2
        let truncated = vec![0x00u8, 0xCDu8];
        assert!(decompress_clavia_lz(&truncated, 100).is_err());
    }
}

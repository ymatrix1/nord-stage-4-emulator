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

#[inline]
fn read_lzo_byte(input: &[u8], ip: &mut usize) -> Result<u8, String> {
    if *ip >= input.len() {
        return Err(format!(
            "unexpected end of LZO1X stream at input offset 0x{:X}",
            *ip
        ));
    }
    let b = input[*ip];
    *ip += 1;
    Ok(b)
}

#[inline]
fn read_lzo_ext_len(input: &[u8], ip: &mut usize, base: usize) -> Result<usize, String> {
    let mut len = base;
    while read_lzo_byte(input, ip)? == 0 {
        len = len
            .checked_add(255)
            .ok_or_else(|| "LZO1X extended length overflow".to_string())?;
    }
    let tail = input[*ip - 1] as usize;
    len.checked_add(tail)
        .ok_or_else(|| "LZO1X extended length overflow".to_string())
}

#[inline]
fn copy_lzo_literals(
    input: &[u8],
    ip: &mut usize,
    out: &mut Vec<u8>,
    count: usize,
    max_output_size: usize,
) -> Result<(), String> {
    if *ip + count > input.len() {
        return Err(format!(
            "LZO1X literal run ({count} B) exceeds input bounds at 0x{:X}",
            *ip
        ));
    }
    if out.len().saturating_add(count) > max_output_size {
        return Err(format!(
            "LZO1X literal run exceeds max_output_size {max_output_size}"
        ));
    }
    out.extend_from_slice(&input[*ip..*ip + count]);
    *ip += count;
    Ok(())
}

#[inline]
fn copy_lzo_match(
    out: &mut Vec<u8>,
    distance: usize,
    length: usize,
    max_output_size: usize,
) -> Result<(), String> {
    if distance == 0 {
        return Err("invalid LZO1X match distance 0".to_string());
    }
    if distance > out.len() {
        return Err(format!(
            "LZO1X distance {distance} exceeds current output buffer length {}",
            out.len()
        ));
    }
    if out.len().saturating_add(length) > max_output_size {
        return Err(format!(
            "LZO1X match exceeds max_output_size {max_output_size}"
        ));
    }
    for pos in (out.len() - distance..).take(length) {
        let byte = out[pos];
        out.push(byte);
    }
    Ok(())
}

/// Decompresses a byte slice encoded with Clavia's OS firmware compression (LZO1X).
///
/// Proved bit-exact against `Update Nord Stage 4 OS v1.66.exe` Image 0
/// (`5,499,504` B, checksum `0xDE902503`) and Image 1 (`798,912` B, checksum `0xFBB813F8`).
pub fn decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(max_output_size.min(1024 * 1024));
    let mut ip = 0usize;
    let mut skip_first_op = false;

    let first = read_lzo_byte(input, &mut ip)?;
    if first > 17 {
        let count = (first - 17) as usize;
        copy_lzo_literals(input, &mut ip, &mut out, count, max_output_size)?;
        if count < 4 {
            skip_first_op = true;
        }
    } else {
        ip = 0;
    }

    while ip < input.len() {
        let mut t = if !skip_first_op {
            let op = read_lzo_byte(input, &mut ip)?;
            if op < 16 {
                let lit_len = if op == 0 {
                    read_lzo_ext_len(input, &mut ip, 15)? + 3
                } else {
                    op as usize + 3
                };
                copy_lzo_literals(input, &mut ip, &mut out, lit_len, max_output_size)?;
                if ip >= input.len() {
                    break;
                }
                let next_op = read_lzo_byte(input, &mut ip)?;
                if next_op < 16 {
                    let lo = read_lzo_byte(input, &mut ip)? as usize;
                    let dist = 1 + 0x0800 + ((next_op as usize) >> 2) + (lo << 2);
                    copy_lzo_match(&mut out, dist, 3, max_output_size)?;
                    let state = (next_op & 3) as usize;
                    if state > 0 {
                        copy_lzo_literals(input, &mut ip, &mut out, state, max_output_size)?;
                        skip_first_op = true;
                    }
                    continue;
                }
                next_op
            } else {
                op
            }
        } else {
            skip_first_op = false;
            read_lzo_byte(input, &mut ip)?
        };

        loop {
            let state = if t >= 64 {
                let hi = read_lzo_byte(input, &mut ip)? as usize;
                let dist = 1 + (((t as usize) >> 2) & 7) + (hi << 3);
                let m_len = ((t as usize) >> 5) + 1;
                copy_lzo_match(&mut out, dist, m_len, max_output_size)?;
                (t & 3) as usize
            } else if t >= 32 {
                let base_len = (t & 31) as usize;
                let m_len = if base_len == 0 {
                    read_lzo_ext_len(input, &mut ip, 31)? + 2
                } else {
                    base_len + 2
                };
                let b0 = read_lzo_byte(input, &mut ip)?;
                let b1 = read_lzo_byte(input, &mut ip)?;
                let dist = 1 + ((b0 as usize) >> 2) + ((b1 as usize) << 6);
                copy_lzo_match(&mut out, dist, m_len, max_output_size)?;
                (b0 & 3) as usize
            } else if t >= 16 {
                let dist_hi = ((t as usize) & 8) << 11;
                let base_len = (t & 7) as usize;
                let m_len = if base_len == 0 {
                    read_lzo_ext_len(input, &mut ip, 7)? + 2
                } else {
                    base_len + 2
                };
                let b0 = read_lzo_byte(input, &mut ip)?;
                let b1 = read_lzo_byte(input, &mut ip)?;
                let dist_lo = ((b0 as usize) >> 2) + ((b1 as usize) << 6);
                let dist = dist_hi + dist_lo;
                if dist == 0 {
                    return Ok(out);
                }
                copy_lzo_match(&mut out, dist + 0x4000, m_len, max_output_size)?;
                (b0 & 3) as usize
            } else {
                let hi = read_lzo_byte(input, &mut ip)? as usize;
                let dist = 1 + ((t as usize) >> 2) + (hi << 2);
                copy_lzo_match(&mut out, dist, 2, max_output_size)?;
                (t & 3) as usize
            };

            if state > 0 {
                copy_lzo_literals(input, &mut ip, &mut out, state, max_output_size)?;
                t = read_lzo_byte(input, &mut ip)?;
            } else {
                break;
            }
        }
    }

    Ok(out)
}

fn push_lzo_literals_zero_state(out: &mut Vec<u8>, lits: &[u8]) {
    if lits.is_empty() {
        return;
    }
    if lits.len() <= 3 {
        // Should only be used as initial literal run or handled via M1/M2; fallback for initial <4
        out.push(17 + lits.len() as u8);
        out.extend_from_slice(lits);
        return;
    }
    let rem = lits.len() - 3;
    if rem <= 15 {
        out.push(rem as u8);
    } else {
        out.push(0);
        let mut extra = rem - 15;
        while extra >= 255 {
            out.push(0);
            extra -= 255;
        }
        out.push(extra as u8);
    }
    out.extend_from_slice(lits);
}

/// Compresses raw data into LZO1X format compatible with `decompress_clavia_lz`.
pub fn compress_clavia_lz(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(input.len() + 16);
    let mut in_pos = 0usize;
    let mut lit_start = 0usize;

    let flush_lits = |out: &mut Vec<u8>, start: usize, end: usize, first_emit: &mut bool| {
        if start >= end {
            return;
        }
        let lits = &input[start..end];
        if *first_emit {
            *first_emit = false;
            if lits.len() <= 238 {
                out.push(17 + lits.len() as u8);
                out.extend_from_slice(lits);
                return;
            }
        }
        if lits.len() >= 4 {
            push_lzo_literals_zero_state(out, lits);
        } else {
            // Emit short run using M2/initial-style isn't valid mid-stream with state=0 unless folded;
            // handled cleanly by emitting as individual initial-or-deferred bytes when possible.
            // To guarantee valid state=0 LZO1X without ambiguity, only split >=4 literals or encode 3+!
            unreachable!("short literal run should only flush when >= 4 or at end of stream");
        }
    };

    let mut first_emit = true;
    while in_pos < input.len() {
        let win_start = in_pos.saturating_sub(2048);
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        for start in win_start..in_pos {
            let dist = in_pos - start;
            let mut len = 0usize;
            while in_pos + len < input.len() && len < 8 && input[start + len] == input[in_pos + len]
            {
                len += 1;
            }
            if len >= 3 && len > best_len {
                best_len = len;
                best_dist = dist;
                if len == 8 {
                    break;
                }
            }
        }

        let lit_len = in_pos - lit_start;
        // Only emit a match if pending literals can be flushed validly (0, or >=4, or <=3 when first_emit)
        let can_flush_lits = lit_len == 0 || lit_len >= 4 || first_emit;
        if best_len >= 3 && can_flush_lits {
            if lit_len > 0 {
                flush_lits(&mut out, lit_start, in_pos, &mut first_emit);
            }
            first_emit = false;
            let d = best_dist - 1;
            let t = (((best_len - 1) as u8) << 5) | (((d & 7) as u8) << 2);
            let hi = (d >> 3) as u8;
            out.push(t);
            out.push(hi);
            in_pos += best_len;
            lit_start = in_pos;
        } else {
            in_pos += 1;
        }
    }

    let rem_lits = in_pos - lit_start;
    if rem_lits > 0 {
        let lits = &input[lit_start..in_pos];
        if first_emit && lits.len() <= 238 {
            out.push(17 + lits.len() as u8);
            out.extend_from_slice(lits);
        } else if lits.len() >= 4 {
            push_lzo_literals_zero_state(&mut out, lits);
        } else {
            // Rewind is unnecessary if we ensure any trailing <4 literals stay attached;
            // if a preceding match existed with state=0, append M1-free EOF?
            // Simple valid LZO1X tail for 1..3 trailing bytes: patch previous match's state bits!
            // Actually, if a match was emitted right before lit_start, out[len-2] holds M2 byte t with state=0:
            if out.len() >= 2 && (out[out.len() - 2] >= 64) {
                let t_idx = out.len() - 2;
                out[t_idx] |= rem_lits as u8;
                out.extend_from_slice(lits);
            } else {
                out.push(17 + lits.len() as u8);
                out.extend_from_slice(lits);
            }
        }
    }

    // LZO1X explicit end-of-stream marker (M4 with dist_lo == 0)
    out.extend_from_slice(&[0x11, 0x00, 0x00]);
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
        // Real ARM926EJ-S reset prefix compressed with LZO1X (verified against OS v1.66 Image 0/1):
        // 00 1B = extended initial literal run of 3 + 15 + 27 = 45 bytes
        // DC 04 = M2 match: len = (0xDC >> 5) + 1 = 7, dist = 1 + ((0xDC >> 2) & 7) + (0x04 << 3) = 40
        // 01 70 D0 9F E5 = literal run of 3 + 1 = 4 bytes
        // ED 02 17 = M2 match len 8, dist 20, + 1 trailing literal (0x17)
        // CD 02 60 = M2 match len 7, dist 20, + 1 trailing literal (0x60)
        // 29 4D 00 1B = M3 match len 11, dist 20, + 1 trailing literal (0x1B)
        // 11 00 00 = LZO1X EOF
        let arm_stub: [u8; 45] = [
            0x00, 0x00, 0x0f, 0xe1, 0xc0, 0x00, 0x80, 0xe3, 0x00, 0xf0, 0x21, 0xe1, 0x00, 0x00,
            0xa0, 0xe3, 0x17, 0x0f, 0x07, 0xee, 0x9a, 0x0f, 0x07, 0xee, 0x10, 0x0f, 0x11, 0xee,
            0x01, 0x0a, 0x80, 0xe3, 0x10, 0x0f, 0x01, 0xee, 0x00, 0x00, 0x0f, 0xe1, 0x1f, 0x00,
            0xc0, 0xe3, 0x13,
        ];
        let mut stream = vec![0x00u8, 0x1Bu8];
        stream.extend_from_slice(&arm_stub);
        stream.extend_from_slice(&[
            0xdc, 0x04, 0x01, 0x70, 0xd0, 0x9f, 0xe5, 0xed, 0x02, 0x17, 0xcd, 0x02, 0x60, 0x29,
            0x4d, 0x00, 0x11, 0x00, 0x00,
        ]);

        let out = decompress_clavia_lz(&stream, 256).expect("ARM stub LZO1X should decode");
        assert_eq!(&out[0..45], &arm_stub);
        // SVC mode: orr r0,r0,#0x13; msr cpsr_c,r0; ldr sp,[pc,#0x70]
        assert_eq!(
            &out[45..56],
            &[0x00, 0x80, 0xe3, 0x00, 0xf0, 0x21, 0xe1, 0x70, 0xd0, 0x9f, 0xe5]
        );
        // ABT mode setup: mrs r0,cpsr; bic r0,r0,#0x1f; orr r0,r0,#0x17; msr cpsr_c,r0; ldr sp,[pc,#0x60]
        assert_eq!(
            &out[56..76],
            &[
                0x00, 0x00, 0x0f, 0xe1, 0x1f, 0x00, 0xc0, 0xe3, 0x17, 0x00, 0x80, 0xe3, 0x00, 0xf0,
                0x21, 0xe1, 0x60, 0xd0, 0x9f, 0xe5
            ]
        );
    }

    #[test]
    fn test_clavia_lz_bounds_protection() {
        // Initial stream containing M2 match exceeding empty output buffer should fail
        // Use initial 1-byte literal prefix (17 + 1 = 18) followed by M2 match (0x40, 0x05 -> dist 41 > 1)
        let bad_dist = vec![18u8, 0xAA, 0x40, 0x05, 0x11, 0x00, 0x00];
        assert!(decompress_clavia_lz(&bad_dist, 100).is_err());

        // Max output overflow should fail
        let tiny_max = vec![22u8, 1, 2, 3, 4, 5, 0x11, 0x00, 0x00];
        assert!(decompress_clavia_lz(&tiny_max, 3).is_err());
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

        for sample in [b"Z".as_slice(), b"XY".as_slice(), b"XYZ".as_slice()] {
            let c_single = compress_clavia_lz(sample);
            let d_single =
                decompress_clavia_lz(&c_single, 10).expect("short literal should decompress");
            assert_eq!(d_single, sample);
        }
    }

    #[test]
    fn test_clavia_lz_all_match_lengths_3_through_8() {
        for target_len in 3..=8 {
            let mut data = vec![b'B'; 30];
            let pattern: Vec<u8> = (0..target_len as u8).map(|i| b'K' + i).collect();
            data.extend_from_slice(&pattern);
            data.extend_from_slice(&pattern);
            data.extend_from_slice(b"TAIL");

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

        let offset = 64usize;
        let mut firmware_raw = vec![0u8; offset + compressed_code.len() + 32];
        firmware_raw[offset..offset + compressed_code.len()].copy_from_slice(&compressed_code);

        let code_seg = FirmwareSegment {
            name: "CODE".to_string(),
            bank: 0,
            data_offset: offset as u32,
            data_size: raw_code.len() as u32,
            target_address: 0xC000_0000,
            checksum: calculate_checksum(raw_code),
        };

        let decompressed = code_seg
            .extract_payload(&firmware_raw)
            .expect("extract_payload on CODE segment should decompress successfully");
        assert_eq!(decompressed, raw_code);
        assert!(code_seg.verify_checksum(&decompressed));
    }

    #[test]
    fn test_clavia_lz_extended_literal_run() {
        // LZO1X extended initial literal run: 0x00, 0x1B => 3 + 15 + 27 = 45 literal bytes + EOF
        let mut stream = vec![0x00u8, 0x1Bu8];
        let literal_bytes = vec![0xA5u8; 45];
        stream.extend_from_slice(&literal_bytes);
        stream.extend_from_slice(&[0x11, 0x00, 0x00]);

        let out = decompress_clavia_lz(&stream, 100)
            .expect("extended literal run decompression should succeed");
        assert_eq!(out, literal_bytes);
    }

    #[test]
    fn test_clavia_lz_truncated_token_error() {
        // M2 match byte with missing high offset byte should fail
        let truncated = vec![18u8, 0xAA, 0x40];
        assert!(decompress_clavia_lz(&truncated, 100).is_err());
    }

    #[test]
    fn test_extract_segment_respects_bank_base_offsets() {
        let bank0_code = b"Bank0 CODE segment LZO1X payload repeating LZO1X payload!";
        let comp0 = compress_clavia_lz(bank0_code);
        let bank1_code = b"Bank1 CODE segment LZO1X payload repeating LZO1X payload!";
        let comp1 = compress_clavia_lz(bank1_code);

        let mut buf = vec![0u8; 2048];
        // Header: 0x20 header_size, bank0 starts at 0x20, bank1 starts at 0x400
        let img0_rel_code = 0x180usize;
        let img1_offset = 0x400usize;
        let img1_rel_code = 0x180usize;
        buf[0..4].copy_from_slice(&0x20u32.to_be_bytes());
        buf[4..8].copy_from_slice(&((img1_offset - 0x20) as u32).to_be_bytes());
        buf[8..12].copy_from_slice(&(img1_offset as u32).to_be_bytes());
        buf[12..16].copy_from_slice(&0x400u32.to_be_bytes());

        // Bank 0 segment table
        buf[0x30..0x34].copy_from_slice(&1u32.to_be_bytes());
        buf[0x34..0x38].copy_from_slice(b"CODE");
        buf[0x38..0x3C].copy_from_slice(&(img0_rel_code as u32).to_be_bytes());
        buf[0x3C..0x40].copy_from_slice(&(bank0_code.len() as u32).to_be_bytes());
        buf[0x40..0x44].copy_from_slice(&0xC000_0000u32.to_be_bytes());
        buf[0x44..0x48].copy_from_slice(&calculate_checksum(bank0_code).to_be_bytes());
        buf[0x20 + img0_rel_code..0x20 + img0_rel_code + comp0.len()].copy_from_slice(&comp0);

        // Bank 1 segment table
        buf[img1_offset + 0x10..img1_offset + 0x14].copy_from_slice(&1u32.to_be_bytes());
        let s1 = img1_offset + 0x14;
        buf[s1..s1 + 4].copy_from_slice(b"CODE");
        buf[s1 + 4..s1 + 8].copy_from_slice(&(img1_rel_code as u32).to_be_bytes());
        buf[s1 + 8..s1 + 12].copy_from_slice(&(bank1_code.len() as u32).to_be_bytes());
        buf[s1 + 12..s1 + 16].copy_from_slice(&0xC000_0000u32.to_be_bytes());
        buf[s1 + 16..s1 + 20].copy_from_slice(&calculate_checksum(bank1_code).to_be_bytes());
        buf[img1_offset + img1_rel_code..img1_offset + img1_rel_code + comp1.len()]
            .copy_from_slice(&comp1);

        let fw = FirmwareContainer::parse(&buf).expect("parse should succeed");
        let out0 = fw
            .extract_segment(0, "CODE", &buf)
            .expect("bank 0 CODE extraction");
        let out1 = fw
            .extract_segment(1, "CODE", &buf)
            .expect("bank 1 CODE extraction");
        assert_eq!(out0, bank0_code);
        assert_eq!(out1, bank1_code);
        assert!(fw
            .segment_in_bank(0, "CODE")
            .unwrap()
            .verify_checksum(&out0));
        assert!(fw
            .segment_in_bank(1, "CODE")
            .unwrap()
            .verify_checksum(&out1));
    }
}

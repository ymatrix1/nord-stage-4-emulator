//! Nord Sample Library 4 (`.nsmp4`) binary parser and multisample metadata models.
//!
//! Spec: `docs/specs/cbin-and-presets.md`, section "NSMP Sample Library Metadata".
//! Metadata parsing runs at load time; this module does not render sample audio.

use super::container::{CbinError, CbinFile, CbinHeader, CbinKind};
use serde::{Deserialize, Serialize};

/// A single key mapping zone within a multisample instrument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleZone {
    /// Zero-based zone index.
    pub zone_index: u16,
    /// Root MIDI note number (e.g. 60 = C4).
    pub root_key: u8,
    /// Lower MIDI note boundary (inclusive).
    pub key_low: u8,
    /// Upper MIDI note boundary (inclusive).
    pub key_high: u8,
    /// Minimum velocity boundary (0..127).
    pub vel_low: u8,
    /// Maximum velocity boundary (1..127).
    pub vel_high: u8,
    /// Associated stroke chunk ID.
    pub stroke_id: u32,
}

impl SampleZone {
    /// Returns true if this zone matches the given MIDI note and velocity.
    #[inline]
    #[must_use]
    pub fn matches(&self, note: u8, velocity: u8) -> bool {
        note >= self.key_low
            && note <= self.key_high
            && velocity >= self.vel_low
            && velocity <= self.vel_high
    }
}

/// A sample stroke descriptor; audio payload contents are not decoded or retained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleStroke {
    /// Unique stroke identifier matching zone definitions.
    pub stroke_id: u32,
    /// Stroke chunk version / ID.
    pub chunk_id: u32,
    /// Length of the raw payload in bytes.
    pub payload_bytes: usize,
    /// Root MIDI note stored in the `\0stk` header.
    pub root_key: u8,
    /// Native sample rate in Hz stored in the `\0stk` header.
    pub sample_rate: u16,
    /// Number of audio channels (`1` mono, `2` stereo).
    pub channels: u8,
}

/// Decoded PCM audio and loop metadata from a single `.nsmp4` `\0stk` chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Nsmp4DecodedStroke {
    /// Unique stroke identifier matching `\0map` zones.
    pub stroke_id: u32,
    /// Root MIDI note stored in the `\0stk` header (`+0x05`).
    pub root_key: u8,
    /// Native sample rate in Hz (`+0x06..+0x08`).
    pub sample_rate: u16,
    /// Number of audio channels (`1` mono, `2` stereo).
    pub channels: u8,
    /// Playback gain in dB (`+0x39..+0x3D`).
    pub gain_db: f32,
    /// Left (or mono) normalized PCM samples (`raw / 8192.0`).
    pub left: Vec<f32>,
    /// Right channel normalized PCM samples when `channels == 2`.
    pub right: Option<Vec<f32>>,
    /// Seamless sustain loop start frame index (`w3` boundary), if looped.
    pub loop_start: Option<usize>,
    /// Seamless sustain loop end frame index (`total_frames`), if looped.
    pub loop_end: Option<usize>,
}

impl Nsmp4DecodedStroke {
    /// Returns the linear amplitude multiplier corresponding to `gain_db`.
    pub fn gain_linear(&self) -> f32 {
        if self.gain_db.is_finite() {
            10.0f32.powf(self.gain_db.clamp(-24.0, 24.0) / 20.0)
        } else {
            1.0
        }
    }
}

/// A tagged chunk within the NSMP chunk stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NsmpChunk<'a> {
    /// 4-byte chunk tag (e.g., `\x00hdr`, `\x00cat`, `\x00map`, `\x00stk`, `\x00sty`, `meta`).
    pub tag: [u8; 4],
    /// Chunk version or sub-id.
    pub chunk_id: u32,
    /// Length of payload in bytes.
    pub length: u32,
    /// Payload slice.
    pub payload: &'a [u8],
}

/// Parsed Nord Sample Library 4 (`.nsmp4`) metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nsmp4Instrument {
    /// Container header information.
    pub header: CbinHeader,
    /// Instrument name (from `\x00hdr` chunk).
    pub name: String,
    /// Author / sound designer name.
    pub author: String,
    /// Direct 128-key mapping table (maps MIDI note 0..127 to assigned root note).
    pub key_map: Vec<u8>,
    /// All active sample zones.
    pub zones: Vec<SampleZone>,
    /// All parsed stroke descriptors.
    pub strokes: Vec<SampleStroke>,
}

impl Nsmp4Instrument {
    /// Parses an `.nsmp4` file from raw binary slice.
    pub fn parse(bytes: &[u8]) -> Result<Self, CbinError> {
        let cbin = CbinFile::parse(bytes)?;
        if cbin.header.kind != CbinKind::SampleLibrary {
            return Err(CbinError::BitstreamError(
                "Expected nsmp kind for .nsmp4 file".to_string(),
            ));
        }

        let chunks = parse_nsmp_chunks(bytes)?;
        let mut name = String::new();
        let mut author = String::new();
        let mut key_map = vec![0u8; 128];
        let mut zones = Vec::new();
        let mut strokes = Vec::new();

        for chunk in chunks {
            match &chunk.tag {
                b"\0hdr" => {
                    let (n, a) = parse_hdr_chunk(chunk.payload);
                    name = n;
                    author = a;
                }
                b"\0map" => {
                    let (km, z) = parse_map_chunk(chunk.payload);
                    key_map = km;
                    zones = z;
                }
                b"\0stk" => {
                    if let Some(stroke) = parse_stk_chunk(&chunk) {
                        strokes.push(stroke);
                    }
                }
                _ => {}
            }
        }

        // Default name from kind if header string is missing
        if name.is_empty() {
            name = format!("Sample Bank {:#04x}", cbin.header.location_index);
        }

        // If direct map table was empty, synthesize from zones if available
        if key_map.iter().all(|&k| k == 0) && !zones.is_empty() {
            for zone in &zones {
                for note in zone.key_low..=zone.key_high {
                    if (note as usize) < key_map.len() {
                        key_map[note as usize] = zone.root_key;
                    }
                }
            }
        }

        Ok(Self {
            header: cbin.header,
            name,
            author,
            key_map,
            zones,
            strokes,
        })
    }

    /// Finds the best matching zone for a given MIDI note and velocity.
    #[must_use]
    pub fn find_zone(&self, note: u8, velocity: u8) -> Option<&SampleZone> {
        self.zones.iter().find(|z| z.matches(note, velocity))
    }
}

/// Parses big-endian tagged chunk hierarchy starting at offset 0x3C.
pub fn parse_nsmp_chunks(data: &[u8]) -> Result<Vec<NsmpChunk<'_>>, CbinError> {
    if data.len() < 0x3C {
        return Err(CbinError::TooShort {
            actual: data.len(),
            min: 0x3C,
        });
    }
    if &data[0x2C..0x30] != b"NSMP" {
        return Err(CbinError::BitstreamError(
            "Expected NSMP marker at offset 0x2C".into(),
        ));
    }

    let mut chunks = Vec::new();
    let mut pos = 0x3C;

    while pos < data.len() {
        let header_end = pos
            .checked_add(12)
            .ok_or_else(|| CbinError::BitstreamError("Chunk header offset overflow".into()))?;
        if header_end > data.len() {
            return Err(CbinError::BitstreamError(format!(
                "Truncated chunk header at offset {pos:#x}"
            )));
        }
        let tag = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
        let chunk_id =
            u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let chunk_len =
            u32::from_be_bytes([data[pos + 8], data[pos + 9], data[pos + 10], data[pos + 11]])
                as usize;

        let payload_start = header_end;
        let payload_end = payload_start
            .checked_add(chunk_len)
            .ok_or_else(|| CbinError::BitstreamError("Chunk payload length overflow".into()))?;
        if payload_end > data.len() {
            return Err(CbinError::BitstreamError(format!(
                "Chunk payload out of bounds: end {payload_end} > total {}",
                data.len()
            )));
        }

        chunks.push(NsmpChunk {
            tag,
            chunk_id,
            length: chunk_len as u32,
            payload: &data[payload_start..payload_end],
        });

        pos = payload_end;
    }

    Ok(chunks)
}

fn parse_hdr_chunk(payload: &[u8]) -> (String, String) {
    if payload.len() < 32 {
        return (String::new(), String::new());
    }

    let mut name = String::new();
    if payload.len() >= 42 {
        let name_slice = &payload[10..42];
        let nul_pos = name_slice
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(name_slice.len());
        name = String::from_utf8_lossy(&name_slice[..nul_pos])
            .trim()
            .to_string();
    }

    let mut author = String::new();
    if payload.len() >= 106 {
        let author_slice = &payload[74..106];
        let nul_pos = author_slice
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(author_slice.len());
        author = String::from_utf8_lossy(&author_slice[..nul_pos])
            .trim()
            .to_string();
    }

    (name, author)
}

fn parse_map_chunk(payload: &[u8]) -> (Vec<u8>, Vec<SampleZone>) {
    let mut key_map = vec![0u8; 128];
    let mut zones = Vec::new();

    if payload.len() >= 1286 {
        for (k, entry) in key_map.iter_mut().enumerate().take(128) {
            let offset = 6 + k * 10;
            *entry = payload[offset + 6];
        }
    }

    if payload.len() >= 0x524 + 16 {
        let mut p = 0x524;
        let mut idx = 0;
        while p + 16 <= payload.len() {
            let rec = &payload[p..p + 16];
            let vel_max = rec[1];
            let root = rec[2];
            let high = rec[3];
            let low = rec[4];
            let stroke_id = u32::from_be_bytes([rec[10], rec[11], rec[12], rec[13]]);
            let vel_min = rec[15];

            if stroke_id > 0 {
                zones.push(SampleZone {
                    zone_index: idx,
                    root_key: root,
                    key_low: low,
                    key_high: high,
                    vel_low: vel_min,
                    vel_high: vel_max,
                    stroke_id,
                });
                idx += 1;
            }
            p += 16;
        }
    }

    (key_map, zones)
}

fn parse_stk_chunk(chunk: &NsmpChunk<'_>) -> Option<SampleStroke> {
    if chunk.payload.len() < 4 {
        return None;
    }
    let stroke_id = u32::from_be_bytes([
        chunk.payload[0],
        chunk.payload[1],
        chunk.payload[2],
        chunk.payload[3],
    ]);
    let root_key = chunk.payload.get(5).copied().unwrap_or(60);
    let sample_rate = if chunk.payload.len() >= 8 {
        u16::from_be_bytes([chunk.payload[6], chunk.payload[7]])
    } else {
        0
    };
    let channels = chunk.payload.get(8).copied().unwrap_or(1).clamp(1, 2);
    Some(SampleStroke {
        stroke_id,
        chunk_id: chunk.chunk_id,
        payload_bytes: chunk.payload.len(),
        root_key,
        sample_rate,
        channels,
    })
}

/// Decodes a single `.nsmp4` `\0stk` chunk payload using the `CNSP` fixed-LPC bitstream codec.
pub fn decode_nsmp_stk_payload(payload: &[u8]) -> Result<Nsmp4DecodedStroke, CbinError> {
    if payload.len() < 0x44 {
        return Err(CbinError::BitstreamError(
            ".nsmp4 \\0stk payload shorter than 68-byte header".to_string(),
        ));
    }

    let stroke_id = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let root_key = payload[0x05];
    let sample_rate = u16::from_be_bytes([payload[0x06], payload[0x07]]);
    let channels = payload[0x08];
    if channels != 1 && channels != 2 {
        return Err(CbinError::BitstreamError(format!(
            "Unsupported .nsmp4 channel count: {channels}"
        )));
    }

    let w1 = u32::from_be_bytes([payload[0x12], payload[0x13], payload[0x14], payload[0x15]]);
    let w3 = u32::from_be_bytes([payload[0x24], payload[0x25], payload[0x26], payload[0x27]]);
    let is_sustained = payload[0x29] == 0 && payload[0x2A] == 0;
    let gain_db = f32::from_be_bytes([payload[0x39], payload[0x3A], payload[0x3B], payload[0x3C]]);

    let mut p = 0x44usize;
    while p + 4 <= payload.len() && payload[p..p + 4] == [0, 0, 0, 0] {
        p += 4;
    }
    let hdr_end = p;
    let target_loop_rel = (is_sustained && w3 > w1).then_some(((w3 - w1) as usize) * 4);

    let mut left = Vec::new();
    let mut right = (channels == 2).then(Vec::new);
    let mut hist_l = [0i64; 4];
    let mut hist_r = [0i64; 4];
    let mut loop_start: Option<usize> = None;

    while p + 4 <= payload.len() {
        // Last 4 bytes of `\0stk` hold the end-of-stream sentinel (`0x00800020` / `0x00800040`).
        if p + 4 == payload.len() {
            break;
        }

        if let Some(rel) = target_loop_rel {
            if loop_start.is_none() && (p - hdr_end) == rel {
                loop_start = Some(left.len());
            }
        }

        let w = u32::from_be_bytes([payload[p], payload[p + 1], payload[p + 2], payload[p + 3]]);
        p += 4;

        let ns = (w & 0x3FFF) as usize;
        let spc = if channels == 1 { ns } else { ns / 2 };
        let order = ((w >> 14) & 0x7) as u8;
        let nbits = ((w >> 19) & 0xF) + 1;

        if spc == 0 || order > 4 {
            return Err(CbinError::BitstreamError(format!(
                "Invalid .nsmp4 frame header word 0x{w:08x} (spc={spc}, order={order})"
            )));
        }

        let wpc = (spc * (nbits as usize)).div_ceil(32);
        let frame_bytes = wpc * (channels as usize) * 4;
        if p + frame_bytes > payload.len() {
            return Err(CbinError::BitstreamError(
                ".nsmp4 frame payload truncated before end of chunk".to_string(),
            ));
        }

        if channels == 1 {
            decode_nsmp_channel_words(
                &payload[p..p + frame_bytes],
                4,
                wpc,
                spc,
                order,
                nbits,
                &mut hist_l,
                &mut left,
            );
        } else {
            decode_nsmp_channel_words(
                &payload[p..p + frame_bytes],
                8,
                wpc,
                spc,
                order,
                nbits,
                &mut hist_l,
                &mut left,
            );
            if let Some(ref mut r_out) = right {
                decode_nsmp_channel_words(
                    &payload[p + 4..p + frame_bytes],
                    8,
                    wpc,
                    spc,
                    order,
                    nbits,
                    &mut hist_r,
                    r_out,
                );
            }
        }

        p += frame_bytes;
    }

    let total_frames = left.len();
    let (loop_start, loop_end) = match loop_start {
        Some(s3) if s3 < total_frames => (Some(s3), Some(total_frames)),
        _ => (None, None),
    };

    Ok(Nsmp4DecodedStroke {
        stroke_id,
        root_key,
        sample_rate,
        channels,
        gain_db,
        left,
        right,
        loop_start,
        loop_end,
    })
}

#[inline]
pub(crate) fn fixed_lpc_predict(order: u8, hist: &[i64; 4]) -> i64 {
    match order {
        0 => 0,
        1 => hist[3],
        2 => 2 * hist[3] - hist[2],
        3 => 3 * hist[3] - 3 * hist[2] + hist[1],
        4 => 4 * hist[3] - 6 * hist[2] + 4 * hist[1] - hist[0],
        _ => 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_nsmp_channel_words(
    words_bytes: &[u8],
    stride: usize,
    wpc: usize,
    spc: usize,
    order: u8,
    nbits: u32,
    hist: &mut [i64; 4],
    out: &mut Vec<f32>,
) {
    let mut bit_buf: u64 = 0;
    let mut bits_avail: u32 = 0;
    let mut word_idx = 0usize;
    let mask = (1u64 << nbits) - 1;
    let sign_shift = 32 - nbits;

    out.reserve(spc);
    for _ in 0..spc {
        while bits_avail < nbits && word_idx < wpc {
            let off = word_idx * stride;
            let word = u32::from_be_bytes([
                words_bytes[off],
                words_bytes[off + 1],
                words_bytes[off + 2],
                words_bytes[off + 3],
            ]);
            bit_buf = (bit_buf << 32) | u64::from(word);
            bits_avail += 32;
            word_idx += 1;
        }
        bits_avail -= nbits;
        let raw = ((bit_buf >> bits_avail) & mask) as u32;
        if bits_avail > 0 {
            bit_buf &= (1u64 << bits_avail) - 1;
        } else {
            bit_buf = 0;
        }
        let residual = ((raw << sign_shift) as i32) >> sign_shift;
        let sample = fixed_lpc_predict(order, hist) + i64::from(residual);
        *hist = [hist[1], hist[2], hist[3], sample];
        out.push((sample as f32) / 8192.0);
    }
}

impl Nsmp4Instrument {
    /// Decodes all `\0stk` chunks from the original `.nsmp4` byte slice.
    pub fn decode_strokes(&self, bytes: &[u8]) -> Result<Vec<Nsmp4DecodedStroke>, CbinError> {
        decode_nsmp4_strokes(bytes)
    }
}

/// Decodes all `\0stk` chunks inside an `.nsmp4` byte slice into normalized PCM strokes.
pub fn decode_nsmp4_strokes(bytes: &[u8]) -> Result<Vec<Nsmp4DecodedStroke>, CbinError> {
    let chunks = parse_nsmp_chunks(bytes)?;
    let mut out = Vec::new();
    for chunk in chunks {
        if &chunk.tag == b"\0stk" {
            out.push(decode_nsmp_stk_payload(chunk.payload)?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_nsmp4() -> Vec<u8> {
        // 48-byte CBIN header + 12-byte NSMP header prefix
        let mut data = vec![0u8; 48];
        data[0..4].copy_from_slice(b"CBIN");
        data[4..8].copy_from_slice(&1u32.to_le_bytes());
        data[8..12].copy_from_slice(b"nsmp");
        data[12..14].copy_from_slice(&12u16.to_le_bytes());
        data[14..16].copy_from_slice(&10u16.to_le_bytes());
        data[16..20].copy_from_slice(&458753u32.to_le_bytes());
        data[20..22].copy_from_slice(&410u16.to_le_bytes());
        data[24..28].copy_from_slice(&0x12345678u32.to_le_bytes());
        data[0x2c..0x30].copy_from_slice(b"NSMP");

        // 12 bytes of NSMP payload prefix (at 0x30..0x3c)
        data.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x04, 0x00, 0x02, 0x00, 0x05,
        ]);

        // Chunk 1: \0hdr
        let hdr_name = b"Synthetic Kalimba\0";
        let mut hdr_payload = vec![0u8; 112];
        hdr_payload[10..10 + hdr_name.len()].copy_from_slice(hdr_name);
        data.extend_from_slice(b"\0hdr");
        data.extend_from_slice(&11u32.to_be_bytes());
        data.extend_from_slice(&(hdr_payload.len() as u32).to_be_bytes());
        data.extend_from_slice(&hdr_payload);

        // Chunk 2: \0map
        let mut map_payload = vec![0u8; 1576];
        map_payload[0..6].copy_from_slice(&[0x1a, 0x8d, 0xab, 0x00, 0x00, 0x00]);
        for k in 0..128 {
            let off = 6 + k * 10;
            map_payload[off + 6] = 60; // assign all to middle C root
            map_payload[off + 7] = 72;
        }
        // Zone at 0x524
        let p = 0x524;
        map_payload[p + 1] = 127; // vel_max
        map_payload[p + 2] = 60; // root
        map_payload[p + 3] = 72; // high
        map_payload[p + 4] = 48; // low
        map_payload[p + 10..p + 14].copy_from_slice(&1u32.to_be_bytes()); // stroke_id = 1
        map_payload[p + 15] = 1; // vel_min

        data.extend_from_slice(b"\0map");
        data.extend_from_slice(&21u32.to_be_bytes());
        data.extend_from_slice(&(map_payload.len() as u32).to_be_bytes());
        data.extend_from_slice(&map_payload);

        // Chunk 3: \0stk
        let mut stk_payload = vec![0u8; 64];
        stk_payload[0..4].copy_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(b"\0stk");
        data.extend_from_slice(&11u32.to_be_bytes());
        data.extend_from_slice(&(stk_payload.len() as u32).to_be_bytes());
        data.extend_from_slice(&stk_payload);

        data
    }

    #[test]
    fn test_parse_synthetic_nsmp4() {
        let bytes = build_synthetic_nsmp4();
        let inst = Nsmp4Instrument::parse(&bytes).expect("valid synthetic nsmp4");
        assert_eq!(inst.name, "Synthetic Kalimba");
        assert_eq!(inst.zones.len(), 1);
        let zone = &inst.zones[0];
        assert_eq!(zone.root_key, 60);
        assert_eq!(zone.key_low, 48);
        assert_eq!(zone.key_high, 72);
        assert_eq!(zone.vel_low, 1);
        assert_eq!(zone.vel_high, 127);
        assert_eq!(zone.stroke_id, 1);
        assert_eq!(inst.strokes.len(), 1);
        assert_eq!(inst.strokes[0].stroke_id, 1);

        // Test zone lookup
        let hit = inst.find_zone(60, 100);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().root_key, 60);
        assert!(inst.find_zone(36, 100).is_none());
    }

    #[test]
    fn test_rejects_wrong_container_kind() {
        let mut bytes = build_synthetic_nsmp4();
        bytes[8..12].copy_from_slice(b"ns4p");
        assert!(Nsmp4Instrument::parse(&bytes).is_err());
    }

    #[test]
    fn test_rejects_truncated_chunk_header() {
        let mut bytes = build_synthetic_nsmp4();
        bytes.extend_from_slice(b"tail");
        assert!(parse_nsmp_chunks(&bytes).is_err());
    }

    #[test]
    fn test_rejects_chunk_payload_out_of_bounds() {
        let mut bytes = build_synthetic_nsmp4();
        let last_chunk = bytes.len() - 64 - 12;
        bytes[last_chunk + 8..last_chunk + 12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_nsmp_chunks(&bytes).is_err());
    }

    #[test]
    fn test_rejects_missing_nsmp_marker() {
        let mut bytes = build_synthetic_nsmp4();
        bytes[0x2c..0x30].copy_from_slice(b"NOPE");
        assert!(parse_nsmp_chunks(&bytes).is_err());
    }

    fn pack_msb_words(residuals: &[i32], nbits: u32) -> Vec<u32> {
        let spc = residuals.len();
        let wpc = (spc * (nbits as usize)).div_ceil(32);
        let mask = (1u32 << nbits) - 1;
        let mut words = Vec::with_capacity(wpc);
        let mut acc: u64 = 0;
        let mut bits = 0u32;
        for &r in residuals {
            let masked = (r as u32) & mask;
            acc = (acc << nbits) | u64::from(masked);
            bits += nbits;
            while bits >= 32 {
                bits -= 32;
                words.push((acc >> bits) as u32);
                acc &= if bits > 0 { (1u64 << bits) - 1 } else { 0 };
            }
        }
        if bits > 0 {
            words.push((acc << (32 - bits)) as u32);
        }
        while words.len() < wpc {
            words.push(0);
        }
        words
    }

    fn encode_lpc_residuals(samples: &[i32], order: u8, hist: &mut [i64; 4]) -> Vec<i32> {
        let mut out = Vec::with_capacity(samples.len());
        for &s in samples {
            let pred = fixed_lpc_predict(order, hist);
            let res = i64::from(s) - pred;
            out.push(res as i32);
            *hist = [hist[1], hist[2], hist[3], i64::from(s)];
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn build_stk_with_frames(
        stroke_id: u32,
        root_key: u8,
        sample_rate: u16,
        channels: u8,
        gain_db: f32,
        sustained: bool,
        frames: &[(u8, u32, Vec<i32>, Vec<i32>)],
        loop_frame_idx: Option<usize>,
    ) -> Vec<u8> {
        let mut payload = vec![0u8; 0x44 + 8]; // includes two 0x0000_0000 padding words
        payload[0..4].copy_from_slice(&stroke_id.to_be_bytes());
        payload[0x05] = root_key;
        payload[0x06..0x08].copy_from_slice(&sample_rate.to_be_bytes());
        payload[0x08] = channels;
        let w1: u32 = 0x1000;
        payload[0x12..0x16].copy_from_slice(&w1.to_be_bytes());
        if !sustained {
            payload[0x29] = 0x11;
            payload[0x2A] = 0x18;
        }
        payload[0x39..0x3D].copy_from_slice(&gain_db.to_be_bytes());

        let hdr_end = payload.len();
        let mut hist_l = [0i64; 4];
        let mut hist_r = [0i64; 4];

        for (idx, (order, nbits, l_samps, r_samps)) in frames.iter().enumerate() {
            if loop_frame_idx == Some(idx) {
                let w3 = w1 + ((payload.len() - hdr_end) / 4) as u32;
                payload[0x24..0x28].copy_from_slice(&w3.to_be_bytes());
            }
            let spc = l_samps.len();
            let ns = if channels == 1 { spc } else { spc * 2 };
            let loop_seek_bit = u32::from(loop_frame_idx == Some(idx)) << 18;
            let ctrl = (ns as u32 & 0x3FFF)
                | (u32::from(*order) << 14)
                | loop_seek_bit
                | ((*nbits - 1) << 19);
            payload.extend_from_slice(&ctrl.to_be_bytes());

            let res_l = encode_lpc_residuals(l_samps, *order, &mut hist_l);
            let words_l = pack_msb_words(&res_l, *nbits);
            if channels == 1 {
                for w in words_l {
                    payload.extend_from_slice(&w.to_be_bytes());
                }
            } else {
                let res_r = encode_lpc_residuals(r_samps, *order, &mut hist_r);
                let words_r = pack_msb_words(&res_r, *nbits);
                for (wl, wr) in words_l.iter().zip(words_r.iter()) {
                    payload.extend_from_slice(&wl.to_be_bytes());
                    payload.extend_from_slice(&wr.to_be_bytes());
                }
            }
        }

        // EOF sentinel (4 bytes)
        let sentinel: u32 = if channels == 1 {
            0x0080_0020
        } else {
            0x0080_0040
        };
        payload.extend_from_slice(&sentinel.to_be_bytes());
        payload
    }

    #[test]
    fn test_decode_nsmp_mono_all_lpc_orders_and_sustain_loop() {
        let f0 = vec![10, 20, 35, 55, 80, 110]; // order 0
        let f1 = vec![112, 115, 117, 118, 120, 121]; // order 1
        let f2 = vec![122, 123, 124, 125, 126, 127]; // order 2
        let f3 = vec![129, 132, 136, 141, 147, 154]; // order 3
        let f4 = vec![162, 171, 181, 192, 204, 217]; // order 4 (loop starts here)
        let frames = vec![
            (0u8, 8u32, f0.clone(), Vec::new()),
            (1, 4, f1.clone(), Vec::new()),
            (2, 4, f2.clone(), Vec::new()),
            (3, 5, f3.clone(), Vec::new()),
            (4, 6, f4.clone(), Vec::new()),
        ];

        let stk = build_stk_with_frames(7, 60, 35_002, 1, 3.0, true, &frames, Some(4));
        let decoded = decode_nsmp_stk_payload(&stk).expect("decode mono nsmp4");
        assert_eq!(decoded.stroke_id, 7);
        assert_eq!(decoded.root_key, 60);
        assert_eq!(decoded.sample_rate, 35_002);
        assert_eq!(decoded.channels, 1);
        assert!((decoded.gain_db - 3.0).abs() < 1e-5);
        assert!(decoded.right.is_none());
        assert_eq!(decoded.loop_start, Some(24));
        assert_eq!(decoded.loop_end, Some(30));

        let expected: Vec<i32> = [f0, f1, f2, f3, f4].concat();
        assert_eq!(decoded.left.len(), expected.len());
        for (got, exp) in decoded.left.iter().zip(expected.iter()) {
            let exp_f = (*exp as f32) / 8192.0;
            assert!((got - exp_f).abs() < 1e-6);
        }
    }

    #[test]
    fn test_decode_nsmp_stereo_oneshot_roundtrip() {
        let l0 = vec![12, 24, 36, 48, 60, 72, 84, 96];
        let r0 = vec![-12, -24, -36, -48, -60, -72, -84, -96];
        let l1 = vec![108, 120, 132, 144, 156, 168];
        let r1 = vec![-108, -120, -132, -144, -156, -168];
        let frames = vec![
            (1u8, 8u32, l0.clone(), r0.clone()),
            (2, 6, l1.clone(), r1.clone()),
        ];

        let stk = build_stk_with_frames(9, 64, 34_800, 2, -2.5, false, &frames, Some(1));
        let decoded = decode_nsmp_stk_payload(&stk).expect("decode stereo nsmp4");
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.sample_rate, 34_800);
        assert_eq!(decoded.loop_start, None);
        assert_eq!(decoded.loop_end, None);

        let exp_l: Vec<i32> = [l0, l1].concat();
        let exp_r: Vec<i32> = [r0, r1].concat();
        let got_r = decoded.right.as_ref().expect("stereo right channel");
        assert_eq!(decoded.left.len(), exp_l.len());
        assert_eq!(got_r.len(), exp_r.len());
        for ((gl, gr), (el, er)) in decoded
            .left
            .iter()
            .zip(got_r.iter())
            .zip(exp_l.iter().zip(exp_r.iter()))
        {
            assert!((*gl - (*el as f32) / 8192.0).abs() < 1e-6);
            assert!((*gr - (*er as f32) / 8192.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_decode_nsmp_rejects_short_or_corrupt_chunks() {
        assert!(decode_nsmp_stk_payload(&[0u8; 32]).is_err());

        let mut bad_ch = vec![0u8; 0x48];
        bad_ch[0x08] = 3;
        assert!(decode_nsmp_stk_payload(&bad_ch).is_err());
    }
}

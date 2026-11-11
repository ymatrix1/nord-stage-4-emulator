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
    Some(SampleStroke {
        stroke_id,
        chunk_id: chunk.chunk_id,
        payload_bytes: chunk.payload.len(),
    })
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
}

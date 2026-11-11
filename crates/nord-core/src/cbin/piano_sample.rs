//! Nord Piano Library (`.npno`) Binary Parser and 4-Bit DPCM Audio Decoder.
//!
//! Spec: `docs/specs/cbin-and-presets.md` §5.2, `docs/research/nord-piano-npno-and-codec.md`

use super::container::CBIN_HEADER_SIZE;

/// Size of the 128-note MIDI mapping lookup tables in bytes.
pub const MIDI_NOTE_TABLE_SIZE: usize = 128;

/// Descriptor for an individual acoustic piano multisample stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct NpnoStroke {
    pub data_offset: usize,
    pub data_size: usize,
    pub root_key: u8,
    pub sample_rate: u32,
    pub loop_start: Option<u32>,
    pub loop_end: Option<u32>,
}

/// Parsed Nord Piano (`.npno`) instrument format.
#[derive(Debug, Clone, PartialEq)]
pub struct NpnoInstrument {
    pub name: String,
    pub long_name: String,
    /// 128-entry lookup table mapping MIDI key (0..127) to multisample root key.
    pub key_map: [u8; MIDI_NOTE_TABLE_SIZE],
    /// 128-entry signed micro-cents detuning calibration per key (-128..+127 cents).
    pub tuning_cents: [i8; MIDI_NOTE_TABLE_SIZE],
    /// 128-entry velocity split threshold / layer boundary per key.
    pub velocity_tiers: [u8; MIDI_NOTE_TABLE_SIZE],
    /// Indexed multisample stroke descriptors.
    pub strokes: Vec<NpnoStroke>,
}

impl NpnoInstrument {
    /// Parses a complete `.npno` binary file.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < CBIN_HEADER_SIZE + 0x138 {
            return Err(format!(
                "npno buffer too short: {} bytes (minimum {})",
                bytes.len(),
                CBIN_HEADER_SIZE + 0x138
            ));
        }

        // Validate CBIN header magic and format tag
        if &bytes[0..4] != b"CBIN" {
            return Err("invalid CBIN magic".to_string());
        }
        let format_version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if format_version != 1 {
            return Err(format!("unsupported CBIN version {format_version}"));
        }
        if &bytes[8..12] != b"npno" {
            return Err(format!(
                "expected container format 'npno', found '{}'",
                String::from_utf8_lossy(&bytes[8..12])
            ));
        }

        // CNSP signature check at offset 0x2C
        if bytes.len() >= 0x30 && &bytes[0x2C..0x30] != b"CNSP" {
            return Err("missing CNSP marker at offset 0x2C".to_string());
        }

        // Instrument names
        let name_raw = if bytes.len() >= 0x68 {
            &bytes[0x48..0x68]
        } else {
            &[]
        };
        let long_name_raw = if bytes.len() >= 0x88 {
            &bytes[0x68..0x88]
        } else {
            &[]
        };

        let name = String::from_utf8_lossy(name_raw)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        let long_name = String::from_utf8_lossy(long_name_raw)
            .trim_end_matches('\0')
            .trim()
            .to_string();

        // 128-note key mapping table at 0x00B8..0x0138
        let mut key_map = [0u8; MIDI_NOTE_TABLE_SIZE];
        if bytes.len() >= 0x0138 {
            key_map.copy_from_slice(&bytes[0x00B8..0x0138]);
        }

        // 128-note tuning cents table at 0x01C8..0x0248 (if present)
        let mut tuning_cents = [0i8; MIDI_NOTE_TABLE_SIZE];
        if bytes.len() >= 0x0248 {
            for (i, &b) in bytes[0x01C8..0x0248].iter().enumerate() {
                tuning_cents[i] = b as i8;
            }
        }

        // 128-note velocity tiers table at 0x0238..0x02B8 (if present)
        let mut velocity_tiers = [0u8; MIDI_NOTE_TABLE_SIZE];
        if bytes.len() >= 0x02B8 {
            velocity_tiers.copy_from_slice(&bytes[0x0238..0x02B8]);
        }

        // Parse stroke descriptor entries
        let mut strokes = Vec::new();
        // Strokes table begins at 0x0760 when present
        if bytes.len() >= 0x0760 + 32 {
            let mut cursor = 0x0760;
            while cursor + 32 <= bytes.len() && cursor < 0x1F30 {
                let off =
                    u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                let sz =
                    u32::from_be_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
                if off > 0 && off < bytes.len() && sz > 0 {
                    strokes.push(NpnoStroke {
                        data_offset: off,
                        data_size: sz.min(bytes.len() - off),
                        root_key: 60, // default Middle C fallback
                        sample_rate: 44100,
                        loop_start: None,
                        loop_end: None,
                    });
                }
                cursor += 32;
            }
        }

        Ok(Self {
            name,
            long_name,
            key_map,
            tuning_cents,
            velocity_tiers,
            strokes,
        })
    }

    /// Resolves the multisample root key for a given MIDI note number (0..127).
    pub fn root_key_for_note(&self, note: u8) -> Option<u8> {
        let note_idx = (note as usize).min(MIDI_NOTE_TABLE_SIZE - 1);
        let root = self.key_map[note_idx];
        if root == 0xFF {
            None
        } else {
            Some(root)
        }
    }

    /// Returns the fine-tuning detune in cents for a given MIDI note number.
    pub fn cents_detune_for_note(&self, note: u8) -> i8 {
        let note_idx = (note as usize).min(MIDI_NOTE_TABLE_SIZE - 1);
        self.tuning_cents[note_idx]
    }
}

/// Decodes 4-bit signed differential PCM (DPCM) audio bytes into 32-bit floating point samples.
///
/// Each byte encodes two 4-bit signed samples (high nibble first, then low nibble).
/// Samples represent signed delta deviations integrated into the cumulative waveform:
/// `s[n] = s[n-1] + delta * scale`.
pub fn decode_4bit_dpcm(raw_payload: &[u8], scale: f32) -> Vec<f32> {
    let mut pcm = Vec::with_capacity(raw_payload.len() * 2);
    let mut accumulator = 0.0f32;

    for &byte in raw_payload {
        // High nibble
        let hi_raw = (byte >> 4) & 0x0F;
        let hi_delta = if hi_raw >= 8 {
            (hi_raw as i8) - 16
        } else {
            hi_raw as i8
        };
        accumulator += (hi_delta as f32) * scale;
        accumulator = accumulator.clamp(-1.0, 1.0);
        pcm.push(accumulator);

        // Low nibble
        let lo_raw = byte & 0x0F;
        let lo_delta = if lo_raw >= 8 {
            (lo_raw as i8) - 16
        } else {
            lo_raw as i8
        };
        accumulator += (lo_delta as f32) * scale;
        accumulator = accumulator.clamp(-1.0, 1.0);
        pcm.push(accumulator);
    }

    pcm
}

/// Encodes 32-bit floating point audio samples into 4-bit differential PCM (DPCM) bytes.
///
/// Emits 1 byte per 2 input samples.
pub fn encode_4bit_dpcm(pcm: &[f32], scale: f32) -> Vec<u8> {
    if pcm.is_empty() || scale <= 0.0 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(pcm.len().div_ceil(2));
    let mut accumulator = 0.0f32;
    let inv_scale = 1.0 / scale;

    let mut i = 0;
    while i < pcm.len() {
        // High sample
        let target_hi = pcm[i].clamp(-1.0, 1.0);
        let diff_hi = (target_hi - accumulator) * inv_scale;
        let delta_hi = diff_hi.round().clamp(-8.0, 7.0) as i8;
        accumulator += (delta_hi as f32) * scale;
        accumulator = accumulator.clamp(-1.0, 1.0);
        let nibble_hi = (delta_hi as u8) & 0x0F;

        // Low sample (or 0 if odd length)
        let nibble_lo = if i + 1 < pcm.len() {
            let target_lo = pcm[i + 1].clamp(-1.0, 1.0);
            let diff_lo = (target_lo - accumulator) * inv_scale;
            let delta_lo = diff_lo.round().clamp(-8.0, 7.0) as i8;
            accumulator += (delta_lo as f32) * scale;
            accumulator = accumulator.clamp(-1.0, 1.0);
            (delta_lo as u8) & 0x0F
        } else {
            0
        };

        out.push((nibble_hi << 4) | nibble_lo);
        i += 2;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_npno() -> Vec<u8> {
        let mut buf = vec![0u8; 0x1F50];
        // 48-byte CBIN header
        buf[0..4].copy_from_slice(b"CBIN");
        buf[4..8].copy_from_slice(&1u32.to_le_bytes());
        buf[8..12].copy_from_slice(b"npno");
        buf[12..16].copy_from_slice(&3u32.to_le_bytes());
        buf[16..20].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        // CNSP tag
        buf[0x2C..0x30].copy_from_slice(b"CNSP");

        // Names
        let name = b"Grand Imperial\0";
        buf[0x48..0x48 + name.len()].copy_from_slice(name);
        let long_name = b"Bosendorfer 290 Grand Imperial\0";
        buf[0x68..0x68 + long_name.len()].copy_from_slice(long_name);

        // Key map table (0..127) at 0x00B8
        for note in 0..128 {
            let root = match note {
                0..=24 => 25,
                25..=29 => 25,
                30..=34 => 30,
                35..=39 => 35,
                40..=44 => 40,
                45..=49 => 45,
                50..=54 => 50,
                55..=59 => 55,
                60..=64 => 60,
                108..=127 => 0xFF,
                _ => 60,
            };
            buf[0x00B8 + note] = root;
        }

        // Tuning cents table at 0x01C8
        buf[0x01C8 + 60] = 3i8 as u8; // +3 cents on Middle C
        buf[0x01C8 + 61] = -5i8 as u8; // -5 cents on C#4

        // Velocity tiers table at 0x0238
        buf[0x0238 + 60] = 64;

        // Stroke entry at 0x0760
        buf[0x0760..0x0764].copy_from_slice(&0x0000_1000u32.to_be_bytes()); // offset 4096
        buf[0x0764..0x0768].copy_from_slice(&0x0000_0800u32.to_be_bytes()); // size 2048

        buf
    }

    #[test]
    fn test_parse_npno_header_and_tables() {
        let raw = build_synthetic_npno();
        let npno = NpnoInstrument::parse(&raw).expect("synthetic npno should parse");

        assert_eq!(npno.name, "Grand Imperial");
        assert_eq!(npno.long_name, "Bosendorfer 290 Grand Imperial");
        assert_eq!(npno.root_key_for_note(21), Some(25)); // A0 mapped to C#1
        assert_eq!(npno.root_key_for_note(60), Some(60)); // Middle C
        assert_eq!(npno.root_key_for_note(110), None); // Out of physical range

        assert_eq!(npno.cents_detune_for_note(60), 3);
        assert_eq!(npno.cents_detune_for_note(61), -5);
        assert_eq!(npno.velocity_tiers[60], 64);

        assert_eq!(npno.strokes.len(), 1);
        assert_eq!(npno.strokes[0].data_offset, 0x1000);
        assert_eq!(npno.strokes[0].data_size, 0x0800);
    }

    #[test]
    fn test_parse_npno_rejects_corrupted() {
        assert!(NpnoInstrument::parse(&[0u8; 10]).is_err());

        let mut bad_magic = build_synthetic_npno();
        bad_magic[8..12].copy_from_slice(b"nsmp"); // wrong kind
        assert!(NpnoInstrument::parse(&bad_magic).is_err());

        let mut bad_cnsp = build_synthetic_npno();
        bad_cnsp[0x2C..0x30].copy_from_slice(b"XXXX");
        assert!(NpnoInstrument::parse(&bad_cnsp).is_err());
    }

    #[test]
    fn test_dpcm_4bit_decode_distribution() {
        // Feed sample bytes with deltas: 0, +1, -1, +2, -2
        // Byte 0: 0x01 -> hi = 0, lo = +1
        // Byte 1: 0xF2 -> hi = -1, lo = +2
        // Byte 2: 0xE0 -> hi = -2, lo = 0
        let bytes = [0x01, 0xF2, 0xE0];
        let pcm = decode_4bit_dpcm(&bytes, 0.01);
        assert_eq!(pcm.len(), 6);

        // Check integrated values
        // s0 = 0
        // s1 = 0 + 0.01 = 0.01
        // s2 = 0.01 - 0.01 = 0.00
        // s3 = 0.00 + 0.02 = 0.02
        // s4 = 0.02 - 0.02 = 0.00
        // s5 = 0.00 + 0.00 = 0.00
        assert!((pcm[0] - 0.0).abs() < 1e-5);
        assert!((pcm[1] - 0.01).abs() < 1e-5);
        assert!((pcm[2] - 0.0).abs() < 1e-5);
        assert!((pcm[3] - 0.02).abs() < 1e-5);
        assert!((pcm[4] - 0.0).abs() < 1e-5);
        assert!((pcm[5] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_dpcm_4bit_roundtrip_reconstruction() {
        // Sine wave test
        let mut original = Vec::new();
        for i in 0..100 {
            let t = (i as f32) * 0.1;
            original.push(t.sin() * 0.5);
        }

        let scale = 0.05f32;
        let encoded = encode_4bit_dpcm(&original, scale);
        assert_eq!(encoded.len(), 50); // 100 samples -> 50 bytes (2 samples/byte)

        let decoded = decode_4bit_dpcm(&encoded, scale);
        assert_eq!(decoded.len(), 100);

        // Verify correlation / reasonable tracking error
        let mut max_err = 0.0f32;
        for i in 0..100 {
            let err = (original[i] - decoded[i]).abs();
            if err > max_err {
                max_err = err;
            }
        }
        assert!(max_err < 0.1, "tracking error {max_err} too high");
    }

    #[test]
    fn test_dpcm_empty_and_clamping() {
        assert!(decode_4bit_dpcm(&[], 0.1).is_empty());
        assert!(encode_4bit_dpcm(&[], 0.1).is_empty());

        // Extreme deltas should clamp to [-1.0, 1.0]
        let extreme = [0x77, 0x77, 0x77, 0x77];
        let pcm = decode_4bit_dpcm(&extreme, 0.5);
        for s in pcm {
            assert!((-1.0..=1.0).contains(&s));
        }
    }
}

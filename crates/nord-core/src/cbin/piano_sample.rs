//! Nord Piano Library (`.npno`) Binary Parser and `CNSP` Paged Fixed-LPC Decoder.
//!
//! Spec: `docs/specs/cbin-and-presets.md` §5.2, `docs/research/nsmp4-and-npno-cnsp-codec.md` §3

use super::container::CBIN_HEADER_SIZE;
use super::sample::fixed_lpc_predict;

/// Size of the 128-note MIDI mapping lookup tables in bytes.
pub const MIDI_NOTE_TABLE_SIZE: usize = 128;

/// Base offset of the `.npno` `CNSP` payload relative to file byte 0.
pub const NPNO_CNSP_BASE_OFFSET: usize = 0x002C;

/// Storage sample rate (Hz) used by Nord `.npno` piano sample banks.
pub const NPNO_SAMPLE_RATE_HZ: u32 = 35_000;

/// Byte offset of the 128-entry `[u16 BE; 128]` key stroke count table.
pub const NPNO_KEY_COUNTS_OFFSET: usize = 0x064E;

/// Byte offset where the stride-`0x76` stroke descriptor table begins.
pub const NPNO_STROKE_TABLE_OFFSET: usize = 0x0756;

/// Size in bytes of each stroke descriptor record (`0x76` = 118 bytes).
pub const NPNO_STROKE_RECORD_SIZE: usize = 0x76;

/// Normal sustain stroke kind (`flags_hi == 0`).
pub const NPNO_STROKE_NORMAL: u8 = 0;
/// Damper-down / sympathetic resonance stroke kind (`flags_hi == 1`).
pub const NPNO_STROKE_DAMPER: u8 = 1;
/// Key-off release stroke kind (`flags_hi == 2`).
pub const NPNO_STROKE_RELEASE: u8 = 2;

/// Descriptor for an individual acoustic/electric piano multisample stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct NpnoStroke {
    pub data_offset: usize,
    pub data_size: usize,
    pub root_key: u8,
    pub sample_rate: u32,
    pub channels: u16,
    pub npages: u16,
    pub total_frames: usize,
    pub vel_low: u8,
    pub stroke_kind: u8,
    pub init_left: [i16; 4],
    pub init_right: [i16; 4],
    pub loop_start: Option<u32>,
    pub loop_end: Option<u32>,
}

/// Parsed Nord Piano (`.npno`) instrument format.
#[derive(Debug, Clone, PartialEq)]
pub struct NpnoInstrument {
    pub name: String,
    pub long_name: String,
    /// Audio channel count stored at `0x064A` (`1` = mono, `2` = stereo).
    pub channels: u16,
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

        let mut channels = if bytes.len() >= 0x064C {
            u16::from_be_bytes([bytes[0x064A], bytes[0x064B]])
        } else {
            1
        };
        if channels != 1 && channels != 2 {
            channels = 1;
        }

        // Parse 0x064E..0x074E key counts and 0x0756.. stride-0x76 stroke descriptor table.
        let mut strokes = Vec::new();
        if bytes.len() >= NPNO_STROKE_TABLE_OFFSET {
            let nstrokes = u16::from_be_bytes([bytes[0x064C], bytes[0x064D]]) as usize;
            let mut key_for_idx = Vec::with_capacity(nstrokes);
            for key in 0..MIDI_NOTE_TABLE_SIZE {
                let off = NPNO_KEY_COUNTS_OFFSET + key * 2;
                let count = u16::from_be_bytes([bytes[off], bytes[off + 1]]) as usize;
                for _ in 0..count {
                    key_for_idx.push(key as u8);
                }
            }

            let page_bytes = 1022usize * usize::from(channels);
            for k in 0..nstrokes {
                let rec_off = NPNO_STROKE_TABLE_OFFSET + k * NPNO_STROKE_RECORD_SIZE;
                if rec_off + NPNO_STROKE_RECORD_SIZE > bytes.len() {
                    break;
                }
                let rec = &bytes[rec_off..rec_off + NPNO_STROKE_RECORD_SIZE];
                let rel_off = u32::from_be_bytes([rec[8], rec[9], rec[10], rec[11]]) as usize;
                let data_offset = NPNO_CNSP_BASE_OFFSET + rel_off;
                let u0c = u16::from_be_bytes([rec[12], rec[13]]);
                let stroke_kind = (u0c >> 8) as u8;
                let vel_low = (u0c & 0xFF) as u8;
                let total_frames =
                    u32::from_be_bytes([rec[14], rec[15], rec[16], rec[17]]) as usize;
                let npages = u16::from_be_bytes([rec[18], rec[19]]);
                let data_size = usize::from(npages) * page_bytes;

                let mut init_left = [0i16; 4];
                let mut init_right = [0i16; 4];
                for i in 0..4 {
                    init_left[i] = i16::from_be_bytes([rec[0x14 + i * 2], rec[0x15 + i * 2]]);
                    init_right[i] = i16::from_be_bytes([rec[0x1C + i * 2], rec[0x1D + i * 2]]);
                }

                let root_key = key_for_idx.get(k).copied().unwrap_or(60);
                if npages > 0 && data_offset < bytes.len() {
                    strokes.push(NpnoStroke {
                        data_offset,
                        data_size: data_size.min(bytes.len() - data_offset),
                        root_key,
                        sample_rate: NPNO_SAMPLE_RATE_HZ,
                        channels,
                        npages,
                        total_frames,
                        vel_low,
                        stroke_kind,
                        init_left,
                        init_right,
                        loop_start: None,
                        loop_end: None,
                    });
                }
            }
        }

        Ok(Self {
            name,
            long_name,
            channels,
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

/// Decodes a single `.npno` paged `CNSP` fixed-LPC stroke into normalized `f32` PCM (`y / 8192.0`).
///
/// Supports both mono (`channels == 1`, 1022-byte pages) and sample-interleaved stereo
/// (`channels == 2`, 2044-byte pages), handling the 64-frame page overlap continuity window.
pub fn decode_npno_stroke(
    stroke: &NpnoStroke,
    file_bytes: &[u8],
) -> Result<(Vec<f32>, Option<Vec<f32>>), String> {
    let ch = usize::from(stroke.channels);
    if ch != 1 && ch != 2 {
        return Err(format!("unsupported .npno channel count {ch}"));
    }
    let page_bytes = 1022 * ch;
    let npages = usize::from(stroke.npages);
    let total_bytes = npages
        .checked_mul(page_bytes)
        .ok_or_else(|| "npno stroke size overflow".to_string())?;
    let end = stroke
        .data_offset
        .checked_add(total_bytes)
        .ok_or_else(|| "npno stroke offset overflow".to_string())?;
    if end > file_bytes.len() {
        return Err(format!(
            "npno stroke truncated: need byte {end}, buffer length {}",
            file_bytes.len()
        ));
    }

    let mut hl: [i64; 4] = stroke.init_left.map(i64::from);
    let mut hr: [i64; 4] = stroke.init_right.map(i64::from);
    let mut left_i = Vec::with_capacity(stroke.total_frames);
    let mut right_i = if ch == 2 {
        Vec::with_capacity(stroke.total_frames)
    } else {
        Vec::new()
    };

    for page_idx in 0..npages {
        let p_off = stroke.data_offset + page_idx * page_bytes;
        let page = &file_bytes[p_off..p_off + page_bytes];
        let b1 = page[1];
        let order = b1 >> 5;
        let nbits = u32::from(b1 & 0x0F);
        if order > 4 || nbits == 0 {
            return Err(format!(
                "invalid .npno page header byte 0x{b1:02x} (order={order}, nbits={nbits})"
            ));
        }

        let payload = &page[2..];
        let nf = ((payload.len() * 8) / ch) / (nbits as usize);
        let mask = (1u64 << nbits) - 1;
        let sign_shift = 32 - nbits;

        let mut bit_buf: u64 = 0;
        let mut bits_avail: u32 = 0;
        let mut word_idx = 0usize;

        let mut next_res = || -> Result<i32, String> {
            while bits_avail < nbits {
                let wi = word_idx * 2;
                if wi + 1 >= payload.len() {
                    return Err("unexpected end of .npno page bitstream".to_string());
                }
                let word = u16::from_be_bytes([payload[wi], payload[wi + 1]]);
                bit_buf |= u64::from(word) << bits_avail;
                bits_avail += 16;
                word_idx += 1;
            }
            let raw = (bit_buf & mask) as u32;
            bit_buf >>= nbits;
            bits_avail -= nbits;
            Ok(((raw << sign_shift) as i32) >> sign_shift)
        };

        let mut page_l = Vec::with_capacity(nf);
        let mut page_r = if ch == 2 {
            Vec::with_capacity(nf)
        } else {
            Vec::new()
        };

        for _ in 0..nf {
            let res_l = next_res()?;
            let yl = fixed_lpc_predict(order, &hl) + i64::from(res_l);
            hl = [hl[1], hl[2], hl[3], yl];
            page_l.push(yl);

            if ch == 2 {
                let res_r = next_res()?;
                let yr = fixed_lpc_predict(order, &hr) + i64::from(res_r);
                hr = [hr[1], hr[2], hr[3], yr];
                page_r.push(yr);
            }
        }

        let skip = if page_idx == 0 {
            0
        } else {
            64.min(page_l.len())
        };
        left_i.extend_from_slice(&page_l[skip..]);
        if ch == 2 {
            right_i.extend_from_slice(&page_r[skip..]);
        }

        // Next page initializes from the 4 samples ending 64 frames before the end of this page.
        if page_idx + 1 < npages && page_l.len() >= 68 {
            let tail = page_l.len() - 64;
            hl = [
                page_l[tail - 4],
                page_l[tail - 3],
                page_l[tail - 2],
                page_l[tail - 1],
            ];
            if ch == 2 {
                hr = [
                    page_r[tail - 4],
                    page_r[tail - 3],
                    page_r[tail - 2],
                    page_r[tail - 1],
                ];
            }
        }
    }

    if stroke.total_frames > 0 {
        left_i.truncate(stroke.total_frames);
        if ch == 2 {
            right_i.truncate(stroke.total_frames);
        }
    }

    let left = left_i.into_iter().map(|s| (s as f32) / 8192.0).collect();
    let right = (ch == 2).then(|| right_i.into_iter().map(|s| (s as f32) / 8192.0).collect());
    Ok((left, right))
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
                0..=29 => 25,
                30..=34 => 30,
                35..=39 => 35,
                40..=44 => 40,
                45..=49 => 45,
                50..=54 => 50,
                55..=59 => 55,
                60..=107 => 60,
                _ => 0xFF,
            };
            buf[0x00B8 + note] = root;
        }

        // Tuning cents table at 0x01C8
        buf[0x01C8 + 60] = 3i8 as u8; // +3 cents on Middle C
        buf[0x01C8 + 61] = -5i8 as u8; // -5 cents on C#4

        // Velocity tiers table at 0x0238
        buf[0x0238 + 60] = 64;

        // Channel count (1) and nstrokes (1)
        buf[0x064A..0x064C].copy_from_slice(&1u16.to_be_bytes());
        buf[0x064C..0x064E].copy_from_slice(&1u16.to_be_bytes());
        buf[NPNO_KEY_COUNTS_OFFSET + 60 * 2..NPNO_KEY_COUNTS_OFFSET + 60 * 2 + 2]
            .copy_from_slice(&1u16.to_be_bytes());

        // 0x76-byte stroke descriptor at 0x0756 pointing to offset 0x1000 (rel_off = 0x1000 - 0x2C)
        let rec =
            &mut buf[NPNO_STROKE_TABLE_OFFSET..NPNO_STROKE_TABLE_OFFSET + NPNO_STROKE_RECORD_SIZE];
        let rel_off = (0x1000 - NPNO_CNSP_BASE_OFFSET) as u32;
        rec[8..12].copy_from_slice(&rel_off.to_be_bytes());
        rec[12..14].copy_from_slice(&0x0001u16.to_be_bytes()); // normal, vel_low = 1
        rec[14..18].copy_from_slice(&512u32.to_be_bytes()); // total_frames
        rec[18..20].copy_from_slice(&1u16.to_be_bytes()); // npages = 1

        buf
    }

    #[test]
    fn test_parse_npno_header_and_tables() {
        let raw = build_synthetic_npno();
        let npno = NpnoInstrument::parse(&raw).expect("synthetic npno should parse");

        assert_eq!(npno.name, "Grand Imperial");
        assert_eq!(npno.long_name, "Bosendorfer 290 Grand Imperial");
        assert_eq!(npno.channels, 1);
        assert_eq!(npno.root_key_for_note(21), Some(25)); // A0 mapped to C#1
        assert_eq!(npno.root_key_for_note(60), Some(60)); // Middle C
        assert_eq!(npno.root_key_for_note(110), None); // Out of physical range

        assert_eq!(npno.cents_detune_for_note(60), 3);
        assert_eq!(npno.cents_detune_for_note(61), -5);
        assert_eq!(npno.velocity_tiers[60], 64);

        assert_eq!(npno.strokes.len(), 1);
        assert_eq!(npno.strokes[0].data_offset, 0x1000);
        assert_eq!(npno.strokes[0].data_size, 1022);
        assert_eq!(npno.strokes[0].root_key, 60);
        assert_eq!(npno.strokes[0].sample_rate, NPNO_SAMPLE_RATE_HZ);
        assert_eq!(npno.strokes[0].total_frames, 512);
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

    fn encode_npno_page(
        residuals: &[i32],
        channels: usize,
        b0: u8,
        order: u8,
        nbits: u32,
    ) -> Vec<u8> {
        let page_bytes = 1022 * channels;
        let payload_len = page_bytes - 2;
        let mut page = vec![0u8; page_bytes];
        page[0] = b0;
        page[1] = (order << 5) | ((nbits as u8) & 0x0F);

        let mask = (1u64 << nbits) - 1;
        let mut bit_buf: u64 = 0;
        let mut bits_used: u32 = 0;
        let mut word_idx = 0usize;

        for &r in residuals {
            let masked = (r as u64) & mask;
            bit_buf |= masked << bits_used;
            bits_used += nbits;
            while bits_used >= 16 {
                let w = (bit_buf & 0xFFFF) as u16;
                let wi = 2 + word_idx * 2;
                if wi + 1 < page_bytes {
                    page[wi..wi + 2].copy_from_slice(&w.to_be_bytes());
                }
                bit_buf >>= 16;
                bits_used -= 16;
                word_idx += 1;
            }
        }
        if bits_used > 0 {
            let wi = 2 + word_idx * 2;
            if wi + 1 < 2 + payload_len {
                let w = (bit_buf & 0xFFFF) as u16;
                page[wi..wi + 2].copy_from_slice(&w.to_be_bytes());
            }
        }
        page
    }

    fn encode_npno_lpc_sequence(samples: &[i32], order: u8, init: [i16; 4]) -> Vec<i32> {
        let mut hist = init.map(i64::from);
        let mut out = Vec::with_capacity(samples.len());
        for &s in samples {
            let pred = fixed_lpc_predict(order, &hist);
            out.push((i64::from(s) - pred) as i32);
            hist = [hist[1], hist[2], hist[3], i64::from(s)];
        }
        out
    }

    #[test]
    fn test_decode_npno_mono_multipage_overlap_and_truncation() {
        let nbits = 8u32;
        let order = 2u8;
        let nf = (1020 * 8) / (nbits as usize); // 1020 frames/page
        let total_unique = nf + 300; // span 2 pages, truncated mid-page-1
        let signal: Vec<i32> = (0..total_unique)
            .map(|i| (((i as f32) * 0.05).sin() * 1500.0).round() as i32)
            .collect();

        // Pad page 1 source up to full 64 + (nf - 64) = nf samples
        let mut padded_signal = signal.clone();
        while padded_signal.len() < nf + (nf - 64) {
            let prev = *padded_signal.last().unwrap_or(&0);
            padded_signal.push(prev.saturating_sub(1).max(-1500));
        }

        let init = [-40i16, -20, -10, -2];
        let p0_samples = &padded_signal[0..nf];
        let p0_res = encode_npno_lpc_sequence(p0_samples, order, init);
        let p0_bytes = encode_npno_page(&p0_res, 1, 0, order, nbits);

        let p1_init = [
            p0_samples[nf - 68] as i16,
            p0_samples[nf - 67] as i16,
            p0_samples[nf - 66] as i16,
            p0_samples[nf - 65] as i16,
        ];
        let p1_samples = &padded_signal[nf - 64..nf - 64 + nf];
        let p1_res = encode_npno_lpc_sequence(p1_samples, 4, p1_init);
        let p1_bytes = encode_npno_page(&p1_res, 1, 3, 4, nbits);

        let mut raw = build_synthetic_npno();
        let data_off = 0x1000usize;
        raw.resize(data_off + p0_bytes.len() + p1_bytes.len(), 0);
        raw[data_off..data_off + p0_bytes.len()].copy_from_slice(&p0_bytes);
        raw[data_off + p0_bytes.len()..data_off + p0_bytes.len() + p1_bytes.len()]
            .copy_from_slice(&p1_bytes);

        let rec =
            &mut raw[NPNO_STROKE_TABLE_OFFSET..NPNO_STROKE_TABLE_OFFSET + NPNO_STROKE_RECORD_SIZE];
        rec[14..18].copy_from_slice(&(total_unique as u32).to_be_bytes());
        rec[18..20].copy_from_slice(&2u16.to_be_bytes());
        for i in 0..4 {
            rec[0x14 + i * 2..0x16 + i * 2].copy_from_slice(&init[i].to_be_bytes());
        }

        let inst = NpnoInstrument::parse(&raw).expect("parse mono multipage npno");
        let (left, right) = decode_npno_stroke(&inst.strokes[0], &raw).expect("decode mono stroke");
        assert!(right.is_none());
        assert_eq!(left.len(), total_unique);
        for (i, (&got, &exp_i)) in left.iter().zip(signal.iter()).enumerate() {
            let exp = (exp_i as f32) / 8192.0;
            assert!(
                (got - exp).abs() < 1e-6,
                "sample {i}: got {got}, expected {exp}"
            );
        }
    }

    #[test]
    fn test_decode_npno_stereo_interleaved_and_errors() {
        let nbits = 8u32;
        let order = 1u8;
        let nf = (2042 * 8 / 2) / (nbits as usize); // 1021 frames
        let total_frames = 600usize;
        let left_s: Vec<i32> = (0..nf).map(|i| ((i as i32) * 3) % 4000).collect();
        let right_s: Vec<i32> = (0..nf).map(|i| -(((i as i32) * 5) % 4000)).collect();
        let init_l = [10i16, 20, 30, 40];
        let init_r = [-10i16, -20, -30, -40];
        let res_l = encode_npno_lpc_sequence(&left_s, order, init_l);
        let res_r = encode_npno_lpc_sequence(&right_s, order, init_r);
        let mut interleaved = Vec::with_capacity(nf * 2);
        for (l, r) in res_l.into_iter().zip(res_r.into_iter()) {
            interleaved.push(l);
            interleaved.push(r);
        }
        let page = encode_npno_page(&interleaved, 2, 1, order, nbits);

        let mut raw = build_synthetic_npno();
        raw[0x064A..0x064C].copy_from_slice(&2u16.to_be_bytes());
        let data_off = 0x1000usize;
        raw.resize(data_off + page.len(), 0);
        raw[data_off..data_off + page.len()].copy_from_slice(&page);

        let rec =
            &mut raw[NPNO_STROKE_TABLE_OFFSET..NPNO_STROKE_TABLE_OFFSET + NPNO_STROKE_RECORD_SIZE];
        rec[12..14].copy_from_slice(&((2u16 << 8) | 30).to_be_bytes()); // release stroke, vel 30
        rec[14..18].copy_from_slice(&(total_frames as u32).to_be_bytes());
        rec[18..20].copy_from_slice(&1u16.to_be_bytes());
        for i in 0..4 {
            rec[0x14 + i * 2..0x16 + i * 2].copy_from_slice(&init_l[i].to_be_bytes());
            rec[0x1C + i * 2..0x1E + i * 2].copy_from_slice(&init_r[i].to_be_bytes());
        }

        let inst = NpnoInstrument::parse(&raw).expect("parse stereo npno");
        assert_eq!(inst.channels, 2);
        assert_eq!(inst.strokes[0].stroke_kind, NPNO_STROKE_RELEASE);
        assert_eq!(inst.strokes[0].vel_low, 30);

        let (l, r_opt) = decode_npno_stroke(&inst.strokes[0], &raw).expect("decode stereo");
        let r = r_opt.expect("stereo right channel");
        assert_eq!(l.len(), total_frames);
        assert_eq!(r.len(), total_frames);
        for i in 0..total_frames {
            assert!((l[i] - (left_s[i] as f32) / 8192.0).abs() < 1e-6);
            assert!((r[i] - (right_s[i] as f32) / 8192.0).abs() < 1e-6);
        }

        // Error checks: bad order (5), truncated buffer
        let mut bad_order = inst.strokes[0].clone();
        bad_order.npages = 1;
        let mut bad_raw = raw.clone();
        bad_raw[data_off + 1] = 5 << 5 | 8;
        assert!(decode_npno_stroke(&bad_order, &bad_raw).is_err());

        assert!(decode_npno_stroke(&inst.strokes[0], &raw[..data_off + 10]).is_err());
    }
}

//! Capture of audio buffers from the oracle's data memory and WAV / CSV output.
use crate::core::Core;

/// Sample format of a captured buffer word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    /// IEEE single float bits.
    F32,
    /// Signed 1.31 fixed point, scaled to -1.0..1.0.
    Q31,
}

/// Where a channel's samples live: `[@]ADDR:COUNT[:FMT[:STRIDE]]`.
///
/// `@ADDR` means "the pointer stored at ADDR, read after the pass" (the pass buffers are double
/// buffered through such pointers, `docs/research/org-dsp-map.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BufSpec {
    pub addr: u32,
    pub indirect: bool,
    pub count: u32,
    pub format: Format,
    pub stride: u32,
}

fn number(s: &str) -> Option<u32> {
    match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Parses `[@]ADDR:COUNT[:f32|q31[:STRIDE]]`.
pub fn parse_spec(spec: &str) -> Result<BufSpec, String> {
    let bad = || format!("bad buffer spec {spec:?} (want [@]ADDR:COUNT[:f32|q31[:STRIDE]])");
    let (indirect, rest) = match spec.strip_prefix('@') {
        Some(r) => (true, r),
        None => (false, spec),
    };
    let mut it = rest.split(':');
    let addr = number(it.next().ok_or_else(bad)?).ok_or_else(bad)?;
    let count = number(it.next().ok_or_else(bad)?).ok_or_else(bad)?;
    let format = match it.next() {
        None | Some("f32") => Format::F32,
        Some("q31") => Format::Q31,
        Some(_) => return Err(bad()),
    };
    let stride = match it.next() {
        None => 1,
        Some(s) => number(s).filter(|s| *s > 0).ok_or_else(bad)?,
    };
    if it.next().is_some() {
        return Err(bad());
    }
    Ok(BufSpec {
        addr,
        indirect,
        count,
        format,
        stride,
    })
}

/// Converts one data word to a sample.
pub fn sample(word: u32, format: Format) -> f32 {
    match format {
        Format::F32 => f32::from_bits(word),
        Format::Q31 => (f64::from(word as i32) / 2_147_483_648.0) as f32,
    }
}

/// Reads the samples of `spec` from the core's data memory (unmapped words read 0).
pub fn capture(core: &mut Core, spec: &BufSpec) -> Vec<(u32, u32, f32)> {
    let base = if spec.indirect {
        core.dm_read(spec.addr).unwrap_or(0)
    } else {
        spec.addr
    };
    (0..spec.count)
        .map(|k| {
            let a = base.wrapping_add(k * spec.stride);
            let w = core.dm_read(a).unwrap_or(0);
            (a, w, sample(w, spec.format))
        })
        .collect()
}

/// Mono-or-multichannel 32-bit float WAV (format tag 3) from per-channel sample vectors.
pub fn wav_bytes(channels: &[Vec<f32>], rate: u32) -> Vec<u8> {
    let n = channels.iter().map(Vec::len).max().unwrap_or(0);
    let ch = channels.len().max(1) as u16;
    let data_len = (n * usize::from(ch) * 4) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&ch.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(ch) * 4).to_le_bytes());
    out.extend_from_slice(&(ch * 4).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for k in 0..n {
        for c in channels {
            let v = c.get(k).copied().unwrap_or(0.0);
            // non-finite samples would poison a player: write silence (the CSV keeps the raw word)
            out.extend_from_slice(&(if v.is_finite() { v } else { 0.0 }).to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse_with_defaults_and_errors() {
        let s = parse_spec("0xB0000:64").unwrap();
        assert_eq!((s.addr, s.count, s.stride), (0xB0000, 64, 1));
        assert!(!s.indirect && s.format == Format::F32);
        let s = parse_spec("@0xB2EF4:32:q31:2").unwrap();
        assert!(s.indirect && s.format == Format::Q31 && s.stride == 2);
        for bad in ["", "1", "x:2", "1:2:u8", "1:2:f32:0", "1:2:f32:1:9"] {
            assert!(parse_spec(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn samples_decode_float_and_fixed() {
        assert_eq!(sample(0x3F80_0000, Format::F32), 1.0);
        assert_eq!(sample(0x4000_0000, Format::Q31), 0.5);
        assert_eq!(sample(0x8000_0000, Format::Q31), -1.0);
    }

    #[test]
    fn capture_follows_direct_and_indirect_addresses() {
        let mut c = Core::new(&[]);
        c.dm_write(0xB_0010, 0x3F80_0000).unwrap();
        c.dm_write(0xB_0012, 0x4000_0000).unwrap();
        c.dm_write(0xB_0100, 0xB_0010).unwrap();
        let direct = capture(&mut c, &parse_spec("0xB0010:2:f32:2").unwrap());
        assert_eq!(
            direct,
            vec![(0xB_0010, 0x3F80_0000, 1.0), (0xB_0012, 0x4000_0000, 2.0)]
        );
        let indirect = capture(&mut c, &parse_spec("@0xB0100:1").unwrap());
        assert_eq!(indirect[0].0, 0xB_0010);
    }

    #[test]
    fn wav_has_a_valid_float_header_and_interleaves() {
        let w = wav_bytes(&[vec![1.0, 2.0], vec![3.0, f32::NAN]], 48_000);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([w[20], w[21]]), 3);
        assert_eq!(u16::from_le_bytes([w[22], w[23]]), 2);
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), 48_000);
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 16);
        assert_eq!(w.len(), 44 + 16);
        let f = |o: usize| f32::from_le_bytes([w[o], w[o + 1], w[o + 2], w[o + 3]]);
        assert_eq!((f(44), f(48), f(52), f(56)), (1.0, 3.0, 2.0, 0.0));
    }
}

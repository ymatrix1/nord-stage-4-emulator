//! Dump writers: CSV with `# key: value` metadata header and a 32-bit float WAV.
use crate::exec::Render;
use std::fmt::Write as _;

/// CSV text: metadata lines, a header row `index,<series...>` and one row per sample index.
/// Samples are printed with 9 significant digits (`{:e}`), enough to round-trip an f32 exactly.
pub fn to_csv(render: &Render, extra_meta: &[(String, String)]) -> String {
    let mut out = String::from("# ns4 oracle dump v1\n");
    for (k, v) in extra_meta.iter().chain(render.meta.iter()) {
        let _ = writeln!(out, "# {k}: {v}");
    }
    for (k, s) in render.series.iter().enumerate() {
        let _ = writeln!(
            out,
            "# series {k}: {} ({} samples)",
            s.name,
            s.samples.len()
        );
    }
    out.push_str("index");
    for s in &render.series {
        let _ = write!(out, ",{}", s.name);
    }
    out.push('\n');
    let rows = render
        .series
        .iter()
        .map(|s| s.samples.len())
        .max()
        .unwrap_or(0);
    for n in 0..rows {
        let _ = write!(out, "{n}");
        for s in &render.series {
            match s.samples.get(n) {
                Some(v) => {
                    let _ = write!(out, ",{v:e}");
                }
                None => out.push(','),
            }
        }
        out.push('\n');
    }
    out
}

/// IEEE-float WAV (format tag 3), one channel per series of the first series' length.
pub fn to_wav(render: &Render, rate: u32) -> Result<Vec<u8>, String> {
    let first = render.series.first().ok_or("no series to write")?;
    let frames = first.samples.len();
    let chans: Vec<&crate::exec::Series> = render
        .series
        .iter()
        .filter(|s| s.samples.len() == frames)
        .take(8)
        .collect();
    let channels = chans.len() as u16;
    let data_len = frames as u32 * u32::from(channels) * 4;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&3u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * u32::from(channels) * 4).to_le_bytes());
    w.extend_from_slice(&(channels * 4).to_le_bytes());
    w.extend_from_slice(&32u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for n in 0..frames {
        for s in &chans {
            w.extend_from_slice(&s.samples[n].to_le_bytes());
        }
    }
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::Series;

    fn render() -> Render {
        Render {
            meta: vec![("rate".into(), "48000".into())],
            series: vec![
                Series {
                    name: "L".into(),
                    samples: vec![0.5, -1.0],
                },
                Series {
                    name: "R".into(),
                    samples: vec![0.25, 2.0],
                },
            ],
            ..Render::default()
        }
    }

    #[test]
    fn csv_has_header_metadata_and_exact_floats() {
        let csv = to_csv(&render(), &[("scenario".into(), "t".into())]);
        assert!(csv.contains("# scenario: t\n# rate: 48000\n"), "{csv}");
        assert!(
            csv.contains("index,L,R\n0,5e-1,2.5e-1\n1,-1e0,2e0\n"),
            "{csv}"
        );
        let back: f32 = "1.2345678e-3".parse().unwrap();
        assert_eq!(format!("{back:e}").parse::<f32>().unwrap(), back);
    }

    #[test]
    fn wav_header_and_payload() {
        let w = to_wav(&render(), 48000).unwrap();
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([w[20], w[21]]), 3);
        assert_eq!(u16::from_le_bytes([w[22], w[23]]), 2);
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), 48000);
        assert_eq!(w.len(), 44 + 16);
        assert_eq!(f32::from_le_bytes([w[44], w[45], w[46], w[47]]), 0.5);
        assert!(to_wav(&Render::default(), 48000).is_err());
    }
}

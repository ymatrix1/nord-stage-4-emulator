//! `cargo run -p nord-core --example sample_dump -- FILE.nsmp4 OUT.f32 [--stroke N] [--start FRAME] [--frames N]`
//!
//! Decodes one stroke of a local `.nsmp4` file (`ns4 backup X.ns4b --extract ENTRY FILE`) and writes
//! `N` frames from `FRAME` as interleaved little-endian float32 (left, right; a mono stroke is
//! duplicated) for the `fill ... raw path=` scenario command of `sharc-ref`. The output is a
//! derived test input; it is never committed (`.reference/` material stays local).
use nord_core::cbin::sample::decode_nsmp4_strokes;

fn run(args: &[String]) -> Result<String, String> {
    let [input, output, ..] = args else {
        return Err(
            "usage: sample_dump FILE.nsmp4 OUT.f32 [--stroke N] [--start FRAME] [--frames N]"
                .into(),
        );
    };
    let opt = |name: &str, default: usize| -> Result<usize, String> {
        match args.windows(2).find(|w| w[0] == name) {
            None => Ok(default),
            Some(w) => w[1].parse().map_err(|_| format!("bad {name}")),
        }
    };
    let (stroke, start, frames) = (
        opt("--stroke", 0)?,
        opt("--start", 0)?,
        opt("--frames", 128)?,
    );
    let bytes = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
    let strokes = decode_nsmp4_strokes(&bytes).map_err(|e| format!("{e:?}"))?;
    let s = strokes
        .get(stroke)
        .ok_or_else(|| format!("stroke {stroke} of {}", strokes.len()))?;
    if start + frames > s.left.len() {
        return Err(format!("stroke has {} frames", s.left.len()));
    }
    let mut out = Vec::with_capacity(frames * 8);
    for n in start..start + frames {
        let l = s.left[n];
        let r = s.right.as_ref().map_or(l, |r| r[n]);
        out.extend_from_slice(&l.to_le_bytes());
        out.extend_from_slice(&r.to_le_bytes());
    }
    std::fs::write(output, &out).map_err(|e| format!("{output}: {e}"))?;
    Ok(format!(
        "stroke {stroke}: {} channel(s), {} Hz, {} frames; wrote {frames} frames from {start}",
        s.channels,
        s.sample_rate,
        s.left.len()
    ))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_arguments_and_files_are_errors() {
        assert!(run(&[]).is_err());
        assert!(run(&["/nonexistent.nsmp4".into(), "/tmp/x.f32".into()]).is_err());
        assert!(run(&["a".into(), "b".into(), "--frames".into(), "x".into()]).is_err());
    }
}

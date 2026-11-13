//! Deterministic test-signal generators for the scenario `fill` command.
//!
//! Every generator is a pure function of its parameters (no OS randomness), so a scenario renders
//! the same samples on every host. The Python comparator and the Rust reference renderer rebuild
//! the same signals from the metadata lines in the dump header (`docs/research/oracle-vs-engine.md`).
use std::collections::HashMap;

/// Options of a command: `key=value` words of the command line.
pub type Opts = HashMap<String, String>;

pub fn num(opts: &Opts, key: &str, default: f64) -> Result<f64, String> {
    match opts.get(key) {
        None => Ok(default),
        Some(v) => v.parse::<f64>().map_err(|_| format!("bad {key}={v}")),
    }
}

/// `count` samples of signal `kind`. The value is computed in f64 and rounded once to f32.
///
/// | kind | parameters |
/// |---|---|
/// | `sine` | `freq` (cycles per sample), `amp`, `phase` (cycles) |
/// | `sweep` | `f0`, `f1` (cycles per sample at the start and end), `amp`: linear chirp |
/// | `ramp` | `start`, `step` |
/// | `impulse` | `at` (index), `amp` |
/// | `dc` | `amp` |
/// | `noise` | `seed`, `amp`: xorshift32 white noise, uniform in `[-amp, amp)` |
/// | `hash` | `mul`, `add`: `((n*mul + add) % 64) / 64 - 0.5`, exact in f32 (no libm, golden tests) |
/// | `raw` | `path` (little-endian float32 file, local test input, never committed), `chan` (0-based channel), `chans` (channels interleaved in the file, default 2), `skip` (frames) |
/// | `zero` | none |
pub fn generate(kind: &str, count: usize, opts: &Opts) -> Result<Vec<f32>, String> {
    let amp = num(opts, "amp", 0.5)?;
    let mut out = Vec::with_capacity(count);
    match kind {
        "zero" => out.resize(count, 0.0),
        "dc" => out.resize(count, amp as f32),
        "sine" => {
            let freq = num(opts, "freq", 0.01)?;
            let phase = num(opts, "phase", 0.0)?;
            for n in 0..count {
                let cycles = freq * n as f64 + phase;
                out.push((amp * (std::f64::consts::TAU * cycles).sin()) as f32);
            }
        }
        "sweep" => {
            let f0 = num(opts, "f0", 0.001)?;
            let f1 = num(opts, "f1", 0.2)?;
            let len = count.max(1) as f64;
            for n in 0..count {
                let t = n as f64;
                // phase(t) = f0 t + (f1 - f0) t^2 / (2 len)
                let cycles = f0 * t + (f1 - f0) * t * t / (2.0 * len);
                out.push((amp * (std::f64::consts::TAU * cycles).sin()) as f32);
            }
        }
        "ramp" => {
            let start = num(opts, "start", 0.0)?;
            let step = num(opts, "step", 1.0 / 256.0)?;
            for n in 0..count {
                out.push((start + step * n as f64) as f32);
            }
        }
        "impulse" => {
            let at = num(opts, "at", 0.0)? as usize;
            out.resize(count, 0.0);
            if at < count {
                out[at] = amp as f32;
            }
        }
        "hash" => {
            let mul = num(opts, "mul", 37.0)? as u64;
            let add = num(opts, "add", 11.0)? as u64;
            for n in 0..count as u64 {
                out.push(((n * mul + add) % 64) as f32 / 64.0 - 0.5);
            }
        }
        "raw" => {
            let path = opts.get("path").ok_or("raw needs path=FILE")?;
            let chans = num(opts, "chans", 2.0)? as usize;
            let chan = num(opts, "chan", 0.0)? as usize;
            let skip = num(opts, "skip", 0.0)? as usize;
            if chans == 0 || chan >= chans {
                return Err(format!("raw: chan={chan} of chans={chans}"));
            }
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let words = bytes.as_chunks::<4>().0;
            for n in 0..count {
                let w = words.get((skip + n) * chans + chan).ok_or_else(|| {
                    format!(
                        "{path}: only {} frames, need {}",
                        words.len() / chans,
                        skip + count
                    )
                })?;
                out.push(f32::from_le_bytes(*w));
            }
        }
        "noise" => {
            let mut state = num(opts, "seed", 1.0)? as u32;
            if state == 0 {
                state = 1;
            }
            for _ in 0..count {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let u = f64::from(state) / f64::from(u32::MAX) * 2.0 - 1.0;
                out.push((amp * u) as f32);
            }
        }
        other => return Err(format!("unknown signal kind {other:?}")),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(pairs: &[(&str, &str)]) -> Opts {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn raw_reads_one_channel_of_an_interleaved_float_file() {
        let path = std::env::temp_dir().join("nord_sharc_raw_signal.f32");
        let words: Vec<u8> = [1.0f32, -1.0, 2.0, -2.0, 3.0, -3.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        std::fs::write(&path, words).unwrap();
        let p = path.to_string_lossy().to_string();
        let right = generate(
            "raw",
            2,
            &opts(&[("path", &p), ("chan", "1"), ("skip", "1")]),
        )
        .unwrap();
        assert_eq!(right, vec![-2.0, -3.0]);
        assert!(generate("raw", 4, &opts(&[("path", &p)])).is_err());
        assert!(generate("raw", 1, &opts(&[("path", &p), ("chan", "2")])).is_err());
        assert!(generate("raw", 1, &opts(&[])).is_err());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn sine_hits_quarter_period_peak() {
        let s = generate("sine", 8, &opts(&[("freq", "0.125"), ("amp", "1")])).unwrap();
        assert!((s[2] - 1.0).abs() < 1e-6 && s[0].abs() < 1e-6, "{s:?}");
    }

    #[test]
    fn ramp_impulse_dc_zero() {
        let r = generate("ramp", 3, &opts(&[("start", "1"), ("step", "0.5")])).unwrap();
        assert_eq!(r, vec![1.0, 1.5, 2.0]);
        let i = generate("impulse", 4, &opts(&[("at", "2"), ("amp", "1")])).unwrap();
        assert_eq!(i, vec![0.0, 0.0, 1.0, 0.0]);
        assert_eq!(
            generate("dc", 2, &opts(&[("amp", "0.25")])).unwrap(),
            [0.25; 2]
        );
        assert_eq!(generate("zero", 2, &Opts::new()).unwrap(), [0.0; 2]);
    }

    #[test]
    fn sweep_starts_at_zero_and_is_bounded() {
        let s = generate("sweep", 64, &opts(&[("amp", "0.5")])).unwrap();
        assert_eq!(s[0], 0.0);
        assert!(s.iter().all(|v| v.abs() <= 0.5 + 1e-6));
    }

    #[test]
    fn hash_is_exact() {
        let s = generate("hash", 3, &opts(&[("mul", "37"), ("add", "11")])).unwrap();
        assert_eq!(
            s,
            vec![11.0 / 64.0 - 0.5, 48.0 / 64.0 - 0.5, 21.0 / 64.0 - 0.5]
        );
    }

    #[test]
    fn noise_is_deterministic_and_seeded() {
        let a = generate("noise", 16, &opts(&[("seed", "7")])).unwrap();
        let b = generate("noise", 16, &opts(&[("seed", "7")])).unwrap();
        let c = generate("noise", 16, &opts(&[("seed", "8")])).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(generate("saw", 4, &Opts::new()).is_err());
        assert!(generate("sine", 4, &opts(&[("freq", "x")])).is_err());
    }
}

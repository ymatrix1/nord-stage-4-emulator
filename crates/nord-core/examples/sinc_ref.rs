//! `cargo run -p nord-core --example sinc_ref -- ORACLE.csv [--accum f32|f64] [--round floor|nearest]`
//!
//! Reference render of the SY1SY2 sample resampler (Stage B, `0x1C10EE`) with `SincKernelSet`, for
//! the same inputs as an oracle dump written by `sharc-ref` (`docs/research/oracle-vs-engine.md`).
//! Reads the `# ref.*` metadata lines and the captured ring (`srcL`, `srcR`) of the dump and prints
//! a CSV with the same series names (`L`, `R`, or `k` for `ref.mode kernel`) for
//! `tools/ns4_oracle_compare.py`.
//!
//! Position model (VERIFIED against the oracle, see the document): a 32-bit word `pos` with the
//! frame index in bits 31..25, the phase index in bits 24..17 (lower bits ignored: floor), and
//! `pos += 2 * step` after every output frame, `step = round(ratio * 2^24)`.
#[path = "../tests/support/ext40.rs"]
mod ext40;
use nord_core::sinc::{SincKernelSet, PHASES, TAPS};
use std::collections::HashMap;

/// A parsed oracle dump: `# key: value` metadata and the numeric columns.
struct Dump {
    meta: HashMap<String, String>,
    cols: HashMap<String, Vec<f32>>,
}

fn parse_dump(text: &str) -> Result<Dump, String> {
    let mut meta = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    let mut cols: HashMap<String, Vec<f32>> = HashMap::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            if let Some((k, v)) = rest.split_once(": ") {
                meta.insert(k.to_string(), v.to_string());
            }
        } else if names.is_empty() {
            names = line.split(',').skip(1).map(str::to_string).collect();
        } else {
            for (name, cell) in names.iter().zip(line.split(',').skip(1)) {
                if !cell.is_empty() {
                    let v: f32 = cell.parse().map_err(|_| format!("bad cell {cell:?}"))?;
                    cols.entry(name.clone()).or_default().push(v);
                }
            }
        }
    }
    Ok(Dump { meta, cols })
}

fn param(meta: &HashMap<String, String>, key: &str) -> Result<f64, String> {
    meta.get(key)
        .ok_or_else(|| format!("dump has no `{key}` metadata"))?
        .parse()
        .map_err(|_| format!("bad `{key}`"))
}

/// Arithmetic of the 12-tap sum and the gain multiply.
#[derive(Clone, Copy, PartialEq)]
enum Arith {
    /// Sequential f32 multiply-accumulate, tap 0 first (what a plain Rust engine does).
    F32,
    /// f64 accumulation, one rounding to f32 at the end (infinite-precision bound).
    F64,
    /// The DSP's 40-bit extended format (32 significant bits) after every multiply and add,
    /// then the store to DM: `trunc` drops the low 8 mantissa bits, otherwise rounds to nearest.
    Ext40 { trunc_store: bool, trunc_ops: bool },
}

/// Voice parameters of one render (the `ref.*` metadata of the dump plus the command line).
struct Voice<'a> {
    kernel: usize,
    gain: f32,
    ratio: f64,
    pos_frames: f64,
    frames: usize,
    arith: Arith,
    /// Round the phase to nearest instead of the hardware's floor (`--round nearest`).
    nearest: bool,
    /// The oracle's measured table of this kernel (3072 values, phase-major) instead of the engine's.
    table: Option<&'a [f32]>,
}

/// One resampled channel of a 128-frame ring.
fn render_channel(set: &SincKernelSet, v: &Voice, ring: &[f32]) -> Vec<f32> {
    let Voice {
        kernel,
        gain,
        ratio,
        pos_frames,
        frames,
        arith,
        nearest,
        table,
    } = *v;
    let step = (ratio * f64::from(1u32 << 24)).round() as u32;
    let mut pos = (pos_frames * f64::from(1u32 << 25)).round() as i64 as u32;
    let kern = set.get(kernel);
    let mut out = Vec::with_capacity(frames);
    for _ in 0..frames {
        let (mut idx, mut phase) = ((pos >> 25) as usize, ((pos >> 17) & 0xFF) as usize);
        if nearest && (pos >> 16) & 1 == 1 {
            phase += 1;
            if phase == PHASES {
                phase = 0;
                idx += 1;
            }
        }
        let w = match table {
            Some(t) => &t[phase * TAPS..(phase + 1) * TAPS],
            None => kern.weights(phase),
        };
        let x = |t: usize| ring[(idx + t) % ring.len()];
        let value = match arith {
            Arith::F32 => w.iter().enumerate().fold(0.0_f32, |a, (t, w)| a + w * x(t)) * gain,
            Arith::F64 => {
                let sum = w
                    .iter()
                    .enumerate()
                    .fold(0.0_f64, |a, (t, w)| a + f64::from(*w) * f64::from(x(t)));
                (sum as f32) * gain
            }
            Arith::Ext40 {
                trunc_store,
                trunc_ops,
            } => ext40::mac_ext40(w, x, gain, trunc_ops, trunc_store),
        };
        out.push(value);
        pos = pos.wrapping_add(step.wrapping_mul(2));
    }
    out
}

fn run(args: &[String]) -> Result<String, String> {
    let path = args
        .first()
        .ok_or("usage: sinc_ref ORACLE.csv [--accum f32|f64] [--round floor|nearest]")?;
    let opt = |name: &str| {
        args.windows(2)
            .find(|w| w[0] == name)
            .map(|w| w[1].as_str())
    };
    let arith = match opt("--accum") {
        None | Some("f32") => Arith::F32,
        Some("f64") => Arith::F64,
        Some("ext40") => Arith::Ext40 {
            trunc_store: true,
            trunc_ops: false,
        },
        Some("ext40-near") => Arith::Ext40 {
            trunc_store: false,
            trunc_ops: false,
        },
        Some("ext40-trunc") => Arith::Ext40 {
            trunc_store: true,
            trunc_ops: true,
        },
        Some(other) => return Err(format!("unknown --accum {other:?}")),
    };
    let table: Option<Vec<f32>> = match opt("--kernel-from") {
        None => None,
        Some(p) => {
            let d = parse_dump(&std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?)?;
            Some(
                d.cols
                    .get("k")
                    .ok_or("kernel dump has no series k")?
                    .clone(),
            )
        }
    };
    let nearest = args
        .windows(2)
        .any(|w| w[0] == "--round" && w[1] == "nearest");
    let dump = parse_dump(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?)?;
    let set = SincKernelSet::new();
    let kernel = param(&dump.meta, "ref.kernel").unwrap_or(0.0) as usize;
    let mode = dump.meta.get("ref.mode").map_or("", String::as_str);
    let mut out = String::from("# ns4 sinc_ref reference render\n");
    match mode {
        "kernel" => {
            out.push_str("index,k\n");
            for i in 0..6 * PHASES * TAPS {
                let (k, j) = (i / (PHASES * TAPS), i % (PHASES * TAPS));
                let v = set.get(k).weights(j / TAPS)[j % TAPS];
                out.push_str(&format!("{i},{v:e}\n"));
            }
        }
        "resample" => {
            let frames = dump.cols.get("L").map_or(0, Vec::len);
            let (gain, ratio, pos) = (
                param(&dump.meta, "ref.gain")? as f32,
                param(&dump.meta, "ref.ratio")?,
                param(&dump.meta, "ref.pos_frames")?,
            );
            // a dump holds all six measured kernels, 3072 values each
            let voice = Voice {
                kernel,
                gain,
                ratio,
                pos_frames: pos,
                frames,
                arith,
                nearest,
                table: table
                    .as_deref()
                    .map(|t| &t[kernel * PHASES * TAPS..(kernel + 1) * PHASES * TAPS]),
            };
            let mut chans = Vec::new();
            for src in ["srcL", "srcR"] {
                let ring = dump
                    .cols
                    .get(src)
                    .ok_or(format!("dump has no series {src}"))?;
                chans.push(render_channel(&set, &voice, ring));
            }
            out.push_str("index,L,R\n");
            for (n, (l, r)) in chans[0].iter().zip(&chans[1]).enumerate() {
                out.push_str(&format!("{n},{l:e},{r:e}\n"));
            }
        }
        other => return Err(format!("unsupported ref.mode {other:?}")),
    }
    Ok(out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => print!("{text}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

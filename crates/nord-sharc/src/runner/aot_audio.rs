//! Audio-output comparison of the approximate AOT tier against the exact interpreter.
//!
//! The exact core ([`Core::run`] semantics, the oracle) and a core running the generated block
//! built with `--tier approx` (host `f32`, not bit-exact) get the same boot, messages and passes.
//! Their states diverge by design, so nothing but the pass buffers is compared: after each pass
//! the buffers named by `bufs` are read from both cores and judged by SNR, maximum error and band
//! energies, with the tolerances fixed in [`Tolerance`] before any run.
use super::aot_hot::{run_pass, AotRun};
use super::audio::{capture, BufSpec};
use crate::core::Core;

/// Pass criteria, written down before the first measurement (see the research note).
#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    /// Whole-run SNR of the approximate output against the exact one, dB.
    pub snr_db: f64,
    /// Worst single-pass SNR (passes whose reference energy is non-zero), dB.
    pub worst_pass_snr_db: f64,
    /// Largest band-energy difference in the spectrum, dB, over bands within 80 dB of the peak.
    pub band_db: f64,
    /// Passes whose step count differs (control flow took another path).
    pub step_mismatches: u32,
}

pub const TOLERANCE: Tolerance = Tolerance {
    snr_db: 100.0,
    worst_pass_snr_db: 80.0,
    band_db: 0.1,
    step_mismatches: 0,
};

/// What the comparison measured.
#[derive(Debug, Clone)]
pub struct AudioReport {
    pub passes: u32,
    pub samples: usize,
    pub snr_db: f64,
    pub worst_pass_snr_db: f64,
    /// Pass index (warmup first) of that worst SNR and its reference energy.
    pub worst_pass: (usize, f64),
    /// Worst single-pass SNR over the measured passes only (after the warmup passes).
    pub worst_measured_snr_db: f64,
    pub max_abs_err: f64,
    /// Largest `|err| / |exact|` over affected words with `|exact| > 1e-30`.
    pub max_rel_err: f64,
    pub peak: f64,
    /// Distinct addresses whose word differs in some pass (the domain of the metrics).
    pub affected_words: usize,
    pub step_mismatches: u32,
    /// SNR over four consecutive windows of passes: error growth over time.
    pub window_snr_db: [f64; 4],
    pub band_db: f64,
    /// Words that differ bitwise (all passes) and the lowest and highest differing address.
    pub diff_words: u64,
    pub diff_span: Option<(u32, u32)>,
    /// Hot-block calls made by the approximate core, and passes after which its full state
    /// differs from the exact core's (they may differ in registers without differing in output).
    pub block_calls: u64,
    pub state_diff_passes: u32,
    pub first_state_diff: Option<String>,
}

impl AudioReport {
    /// Whether every measured value is inside `t`.
    pub fn within(&self, t: &Tolerance) -> bool {
        self.snr_db >= t.snr_db
            && self.worst_pass_snr_db >= t.worst_pass_snr_db
            && self.band_db <= t.band_db
            && self.step_mismatches <= t.step_mismatches
    }

    pub fn report(&self, label: &str) -> String {
        format!(
            "{label}: {} passes {} samples | SNR {:.1} dB | worst pass {:.1} dB (#{} energy {:.2e}; after warmup {:.1} dB) | max err {:.3e} (max rel {:.3e}, peak {:.3e}, {} affected addresses) | windows {:.1}/{:.1}/{:.1}/{:.1} dB | spectrum {:.4} dB | step mismatches {} | differing words {} span {} | block calls {} | passes with state diff {} ({}) | {}",
            self.passes,
            self.samples,
            self.snr_db,
            self.worst_pass_snr_db,
            self.worst_pass.0,
            self.worst_pass.1,
            self.worst_measured_snr_db,
            self.max_abs_err,
            self.max_rel_err,
            self.peak,
            self.affected_words,
            self.window_snr_db[0],
            self.window_snr_db[1],
            self.window_snr_db[2],
            self.window_snr_db[3],
            self.band_db,
            self.step_mismatches,
            self.diff_words,
            self.diff_span.map_or("none".to_string(), |(a, b)| format!("0x{a:x}..0x{b:x}")),
            self.block_calls,
            self.state_diff_passes,
            self.first_state_diff.as_deref().unwrap_or("-"),
            if self.within(&TOLERANCE) { "WITHIN TOLERANCE" } else { "OUTSIDE TOLERANCE" },
        )
    }
}

/// `10 log10(signal / noise)`, infinite when there is no error.
pub fn snr_db(signal: f64, noise: f64) -> f64 {
    if noise == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (signal / noise).log10()
    }
}

/// In-place radix-2 FFT of `(re, im)` pairs (length a power of two).
fn fft(x: &mut [(f64, f64)]) {
    let n = x.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            x.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (a, b) = (x[start + k], x[start + k + len / 2]);
                let t = (b.0 * wr - b.1 * wi, b.0 * wi + b.1 * wr);
                x[start + k] = (a.0 + t.0, a.1 + t.1);
                x[start + k + len / 2] = (a.0 - t.0, a.1 - t.1);
            }
        }
        len <<= 1;
    }
}

/// Energy of `bands` equal slices of the lower half spectrum of the first power-of-two samples.
pub fn band_energies(samples: &[f64], bands: usize) -> Vec<f64> {
    let n = if samples.len() < 2 {
        0
    } else {
        1 << (usize::BITS - 1 - samples.len().leading_zeros())
    };
    let mut x: Vec<(f64, f64)> = samples[..n].iter().map(|&v| (v, 0.0)).collect();
    fft(&mut x);
    let per = (n / 2) / bands;
    (0..bands)
        .map(|b| {
            x[b * per..(b + 1) * per]
                .iter()
                .map(|c| c.0 * c.0 + c.1 * c.1)
                .sum()
        })
        .collect()
}

/// Largest dB difference between two band-energy sets over bands within 80 dB of the peak.
pub fn max_band_diff_db(reference: &[f64], test: &[f64]) -> f64 {
    let peak = reference.iter().cloned().fold(0.0, f64::max);
    reference
        .iter()
        .zip(test)
        .filter(|(r, _)| **r > peak * 1e-8)
        .map(|(r, t)| (10.0 * (t / r).log10()).abs())
        .fold(0.0, f64::max)
}

/// One captured word of both cores after a pass.
struct Word {
    pass: usize,
    addr: u32,
    exact: f64,
    approx: f64,
    differs: bool,
}

/// The finite float value of a word (non-finite words count as 0).
fn finite(f: f32) -> f64 {
    if f.is_finite() {
        f64::from(f)
    } else {
        0.0
    }
}

/// Runs both cores over boot, `warmup` and `passes` passes, comparing `bufs` after every pass.
///
/// The buffers hold floats mixed with integers and flags, so SNR, errors and spectrum are taken
/// over the *affected words*: the addresses whose word differs bitwise in at least one pass. That
/// is conservative (unaffected words would only raise the SNR) and keeps integer data out of the
/// signal energy.
pub fn audio_compare(r: &AotRun, bufs: &[BufSpec]) -> Result<AudioReport, String> {
    let (mut exact, mut approx) = (core_of(r, false)?, core_of(r, true)?);
    exact.run(r.boot);
    approx.run_aot(r.boot);
    for m in r.msgs.iter().filter(|m| m.pass == 0) {
        super::msg::deliver(&mut exact, r.msg_base, m)?;
        super::msg::deliver(&mut approx, r.msg_base, m)?;
    }
    let total = (r.warmup + r.passes) as usize;
    let mut words = Vec::new();
    let (mut step_mismatches, mut state_diff_passes, mut first_state_diff) = (0, 0, None);
    for pass in 0..total {
        let (se, _) = run_pass(&mut exact, &r.cfg, false)?;
        let (sa, _) = run_pass(&mut approx, &r.cfg, true)?;
        step_mismatches += u32::from(se != sa);
        if let Some(m) = exact.state_mismatch(&approx) {
            state_diff_passes += 1;
            first_state_diff.get_or_insert(m);
        }
        for b in bufs {
            let (xe, xa) = (capture(&mut exact, b), capture(&mut approx, b));
            for ((addr, we, e), (_, wa, a)) in xe.into_iter().zip(xa) {
                words.push(Word {
                    pass,
                    addr,
                    exact: finite(e),
                    approx: finite(a),
                    differs: we != wa,
                });
            }
        }
    }
    let affected: std::collections::BTreeSet<u32> =
        words.iter().filter(|w| w.differs).map(|w| w.addr).collect();
    let (mut sig, mut err) = (vec![0.0f64; total], vec![0.0f64; total]);
    let (mut max_abs, mut peak, mut max_rel) = (0.0f64, 0.0f64, 0.0f64);
    let (mut ref_stream, mut test_stream) = (Vec::new(), Vec::new());
    for w in words.iter().filter(|w| affected.contains(&w.addr)) {
        let d = w.exact - w.approx;
        sig[w.pass] += w.exact * w.exact;
        err[w.pass] += d * d;
        max_abs = max_abs.max(d.abs());
        peak = peak.max(w.exact.abs());
        if w.exact.abs() > 1e-30 {
            max_rel = max_rel.max(d.abs() / w.exact.abs());
        }
        ref_stream.push(w.exact);
        test_stream.push(w.approx);
    }
    let window = |lo: usize, hi: usize| snr_db(sig[lo..hi].iter().sum(), err[lo..hi].iter().sum());
    let q = total.div_ceil(4);
    let windows = std::array::from_fn(|i| window((i * q).min(total), ((i + 1) * q).min(total)));
    let (worst_p, worst) = (0..total)
        .filter(|&p| sig[p] > 0.0)
        .map(|p| (p, snr_db(sig[p], err[p])))
        .fold((0, f64::INFINITY), |a, b| if b.1 < a.1 { b } else { a });
    let diff_words = words.iter().filter(|w| w.differs).count() as u64;
    Ok(AudioReport {
        passes: total as u32,
        samples: ref_stream.len(),
        snr_db: snr_db(sig.iter().sum(), err.iter().sum()),
        worst_pass_snr_db: worst,
        worst_measured_snr_db: (r.warmup as usize..total)
            .filter(|&p| sig[p] > 0.0)
            .map(|p| snr_db(sig[p], err[p]))
            .fold(f64::INFINITY, f64::min),
        worst_pass: (worst_p, sig.get(worst_p).copied().unwrap_or(0.0)),
        max_abs_err: max_abs,
        max_rel_err: max_rel,
        peak,
        step_mismatches,
        window_snr_db: windows,
        diff_words,
        diff_span: affected.first().zip(affected.last()).map(|(a, b)| (*a, *b)),
        affected_words: affected.len(),
        block_calls: approx.aot_hot_stats().calls,
        state_diff_passes,
        first_state_diff,
        band_db: max_band_diff_db(
            &band_energies(&ref_stream, 32),
            &band_energies(&test_stream, 32),
        ),
    })
}

fn core_of(r: &AotRun, aot: bool) -> Result<Core, String> {
    let mut c = Core::new(r.image);
    if let Some(pc) = r.pc {
        c.pc = pc;
    }
    c.lenient = r.lenient;
    for &(a, v) in r.devs {
        c.preset_device(a, v);
    }
    if aot {
        c.enable_aot_hot(r.ranges)?;
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snr_of_known_noise() {
        assert!(snr_db(1.0, 0.0).is_infinite());
        assert!((snr_db(100.0, 1.0) - 20.0).abs() < 1e-12);
    }

    #[test]
    fn fft_puts_a_sine_in_its_band_and_bands_compare() {
        let n = 1024;
        let sine = |f: f64, a: f64| -> Vec<f64> {
            (0..n)
                .map(|i| a * (2.0 * std::f64::consts::PI * f * i as f64 / n as f64).sin())
                .collect()
        };
        // bin 100 of 512 is in band 6 of 32 (16 bins per band)
        let e = band_energies(&sine(100.0, 1.0), 32);
        let top = e
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert_eq!(top, 6);
        let half = band_energies(&sine(100.0, 0.5), 32);
        let d = max_band_diff_db(&e, &half);
        assert!((d - 6.0206).abs() < 0.01, "{d}");
        assert_eq!(max_band_diff_db(&e, &e), 0.0);
    }

    #[test]
    fn tolerance_check_uses_every_criterion() {
        let ok = AudioReport {
            passes: 1,
            samples: 8,
            snr_db: 120.0,
            worst_pass_snr_db: 110.0,
            worst_pass: (0, 1.0),
            worst_measured_snr_db: 110.0,
            max_abs_err: 0.0,
            max_rel_err: 0.0,
            affected_words: 0,
            peak: 1.0,
            step_mismatches: 0,
            window_snr_db: [120.0; 4],
            band_db: 0.01,
            diff_words: 0,
            diff_span: None,
            block_calls: 0,
            state_diff_passes: 0,
            first_state_diff: None,
        };
        assert!(ok.within(&TOLERANCE));
        for bad in [
            AudioReport {
                snr_db: 50.0,
                ..ok.clone()
            },
            AudioReport {
                worst_pass_snr_db: 10.0,
                ..ok.clone()
            },
            AudioReport {
                band_db: 3.0,
                ..ok.clone()
            },
            AudioReport {
                step_mismatches: 1,
                ..ok.clone()
            },
        ] {
            assert!(!bad.within(&TOLERANCE));
            assert!(bad.report("x").contains("OUTSIDE TOLERANCE"));
        }
    }
}

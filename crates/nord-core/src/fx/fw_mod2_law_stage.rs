//! Frame-by-frame adapter around the generated firmware Mod 2 routines that are driven by
//! tabulated laws: `PHAS` ([`super::fw_phas0`], `kFx2Type` 0) and `FLANG` (2, 3), behind `FirmwareFx::mod2` (opt-in, D22).
//!
//! The routines run once per pass on 2 blocks of two stereo frames, so the stage collects 4
//! frames and releases them one call later: **latency 4 frames (83 us at 48 kHz)**. The delay and
//! rate words are in DSP frames at `FIRMWARE_FRAME_RATE_HZ`: phase increments are scaled by
//! `F / fs`, the delay words listed per type by `fs / F` (a delay keeps its duration in seconds).
//!
//! Provenance (`docs/research/fx-mod2-types.md` s8): the routines are VERIFIED against the
//! oracle; the instance row, the type words and the 11 samples of every `kFx2Amt` / `kFx2Rate`
//! law are the derived values of [`super::fw_mod2_data`] (VERIFIED at the samples, EMPIRICAL
//! between them: linear interpolation). Words that both laws write (`FLANG` `c[-16]`, `PHAS 0`
//! `c[-28]`) use `A(amt) R(rate) / R(64)`, 64 being the value of the type switch (ASSUMED: the
//! captures vary one parameter at a time). `FLANG` `c[-15] = -100 - c[-16]` holds at all 22
//! samples (VERIFIED, test `flang_center_plus_swing_is_constant`).

use super::fw_chorus::{CTX_I1, RING_BASE};
use super::fw_mod2_data as d;
use super::fw_mod2_data::Law;
use super::fw_mod2_run::{process, Mod2State, Routine};
use super::fw_mod2_tables::PHAS_TABLE;

/// Offset of the `PHAS 0` table inside the ring array (`c[0x1A]` points there).
const PHAS_TABLE_AT: usize = 0x100;
const DEFAULT_PARAM: u32 = 64;

/// One wired type: its routine, rows, type words, laws and the words that scale.
struct Spec {
    routine: Routine,
    row: &'static [(i32, u32)],
    words: &'static [(i32, u32)],
    amt: &'static [Law],
    rate: &'static [Law],
    /// Integer words that are phase increments per call (scaled by `F / fs`).
    phase: &'static [i32],
    /// Words that are delays in DSP frames (float; scaled by `fs / F`).
    delay: &'static [i32],
}

fn spec(ty: u8) -> Option<Spec> {
    let (routine, row, words, amt, rate, phase, delay): (_, _, _, _, _, &[i32], &[i32]) = match ty {
        0 => (
            Routine::Phas0,
            d::ROW_PHAS0,
            d::WORDS_0,
            d::AMT_0,
            d::RATE_0,
            &[-32],
            &[],
        ),
        2 => (
            Routine::Flang,
            d::ROW_FLANG,
            d::WORDS_2,
            d::AMT_2,
            d::RATE_2,
            &[-20],
            &[-16, -15],
        ),
        3 => (
            Routine::Flang,
            d::ROW_FLANG,
            d::WORDS_3,
            d::AMT_3,
            d::RATE_3,
            &[-20],
            &[-16, -15],
        ),
        _ => return None,
    };
    Some(Spec {
        routine,
        row,
        words,
        amt,
        rate,
        phase,
        delay,
    })
}

fn index(offset: i32) -> usize {
    (offset + 0x20) as usize
}

/// A law at parameter `v` (`0..=127`): linear between the neighbouring samples, exact at them.
fn eval(law: &Law, v: u32) -> f32 {
    let v = v.min(127);
    let k = d::SAMPLES.iter().rposition(|&s| s <= v).unwrap_or(0).min(9);
    let (x0, x1) = (d::SAMPLES[k], d::SAMPLES[k + 1]);
    let t = (v - x0) as f32 / (x1 - x0) as f32;
    let (a, b) = (law.words[k], law.words[k + 1]);
    if law.float {
        let (a, b) = (f32::from_bits(a), f32::from_bits(b));
        a + (b - a) * t
    } else {
        let (a, b) = (a as i32 as f32, b as i32 as f32);
        a + (b - a) * t
    }
}

fn to_word(law: &Law, x: f32) -> u32 {
    if law.float {
        x.to_bits()
    } else {
        x.round() as i32 as u32
    }
}

/// Firmware routine behind a law table, plus the 4-frame buffers.
#[derive(Debug, Clone)]
pub(super) struct FwLawStage {
    ty: u8,
    st: Mod2State,
    /// `F / fs` for phase increments, `fs / F` for delays.
    phase_scale: f64,
    delay_scale: f64,
    input: [[f32; 2]; 4],
    output: [[f32; 2]; 4],
    pos: usize,
}

impl FwLawStage {
    /// Stage for `kFx2Type` `ty` (0, 2, 3), `kFx2Rate` and `kFx2Amt` in `0..=127`, at
    /// `fs`; `None` for the other types.
    pub(super) fn new(ty: u8, rate: u32, amount: u32, fs: f32) -> Option<Self> {
        let sp = spec(ty)?;
        let ratio = crate::organ::FIRMWARE_FRAME_RATE_HZ / f64::from(fs);
        let mut st = Mod2State::new();
        for &(o, w) in sp.row.iter().filter(|(o, _)| (-0x20..=0x2A).contains(o)) {
            // Pointer words of the image are placeholders (the ARM allocates the rings).
            st.ctx[index(o)] = if (0xB_1000..0xB_2100).contains(&w) {
                RING_BASE
            } else {
                w
            };
        }
        // A variation switch from its base type writes only the words that differ (same module,
        // `fx-type-modules.csv`): the words of the base type stay (EMPIRICAL, see the docs).
        if matches!(ty, 3 | 9 | 11) {
            for &(o, w) in spec(ty - 1).map_or(&[][..], |b| b.words) {
                st.ctx[index(o)] = w;
            }
        }
        for &(o, w) in sp.words {
            st.ctx[index(o)] = w;
        }
        let mut me = Self {
            ty,
            st,
            phase_scale: ratio,
            delay_scale: 1.0 / ratio,
            input: [[0.0; 2]; 4],
            output: [[0.0; 2]; 4],
            pos: 0,
        };
        me.fix_pointers(sp.routine);
        me.scale_words(&sp, false);
        me.set_params(rate, amount);
        Some(me)
    }

    /// The `kFx2Type` this stage was built for.
    pub(super) fn ty(&self) -> u8 {
        self.ty
    }

    /// Words the ARM allocates at run time: self pointers and the table pointer.
    fn fix_pointers(&mut self, routine: Routine) {
        let c = &mut self.st.ctx;
        match routine {
            Routine::Flang => c[index(-0x11)] = CTX_I1 - 0x13,
            Routine::Phas0 => {
                c[index(-0x1F)] = CTX_I1 - 0x17;
                c[index(0x1A)] = RING_BASE + PHAS_TABLE_AT as u32;
                self.st.ring[PHAS_TABLE_AT..PHAS_TABLE_AT + PHAS_TABLE.len()]
                    .copy_from_slice(&PHAS_TABLE);
            }
            Routine::Ens | Routine::Phas1 | Routine::Vibe | Routine::Spin => {}
        }
    }

    /// Scales the phase increments (`F / fs`) and the delays (`fs / F`) of `sp`: the words a law
    /// writes (`laws`) or, once at construction, the static ones.
    fn scale_words(&mut self, sp: &Spec, laws: bool) {
        let covered = |o: i32| sp.amt.iter().chain(sp.rate).any(|l| l.off == o);
        for &o in sp.phase.iter().filter(|&&o| covered(o) == laws) {
            let w = self.st.ctx[index(o)] as i32;
            self.st.ctx[index(o)] = (f64::from(w) * self.phase_scale).round() as i32 as u32;
        }
        for &o in sp.delay.iter().filter(|&&o| covered(o) == laws) {
            let w = self.st.ctx[index(o)];
            self.st.ctx[index(o)] =
                ((f64::from(f32::from_bits(w)) * self.delay_scale) as f32).to_bits();
        }
    }

    /// Writes the targets of `kFx2Rate` and `kFx2Amt`.
    pub(super) fn set_params(&mut self, rate: u32, amount: u32) {
        let Some(sp) = spec(self.ty) else { return };
        let (rate, amount) = (rate.min(127), amount.min(127));
        let norm = |laws: &[Law], off: i32| {
            laws.iter()
                .find(|l| l.off == off)
                .map(|l| eval(l, DEFAULT_PARAM))
        };
        for law in sp.amt.iter().chain(sp.rate) {
            let both =
                sp.amt.iter().any(|l| l.off == law.off) && sp.rate.iter().any(|l| l.off == law.off);
            let is_amt = sp.amt.iter().any(|l| std::ptr::eq(l, law));
            let x = if both {
                // `A(amt) R(rate) / R(64)`: written once, from the amount law.
                if !is_amt {
                    continue;
                }
                let r = sp
                    .rate
                    .iter()
                    .find(|l| l.off == law.off)
                    .map_or(1.0, |l| eval(l, rate));
                let d0 = norm(sp.rate, law.off).filter(|x| *x != 0.0).unwrap_or(1.0);
                eval(law, amount) * r / d0
            } else {
                eval(law, if is_amt { amount } else { rate })
            };
            self.st.ctx[index(law.off)] = to_word(law, x);
        }
        if sp.routine == Routine::Flang {
            // Centre + swing is constant (VERIFIED at all samples): c[-15] = -100 - c[-16].
            let swing = f32::from_bits(self.st.ctx[index(-16)]);
            self.st.ctx[index(-15)] = (-100.0 - swing).to_bits();
        }
        self.scale_words(&sp, true);
    }

    /// Pushes one frame and returns the frame of the previous block at the same position.
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let out = self.output[self.pos];
        self.input[self.pos] = frame;
        self.pos += 1;
        if self.pos == 4 {
            self.pos = 0;
            let routine = spec(self.ty).map_or(Routine::Ens, |s| s.routine);
            let mut block: [f32; 8] = std::array::from_fn(|i| self.input[i / 2][i % 2]);
            process(routine, &mut self.st, &mut block);
            for (i, o) in self.output.iter_mut().enumerate() {
                *o = [block[2 * i], block[2 * i + 1]];
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: [u8; 3] = [0, 2, 3];

    fn run(ty: u8, rate: u32, amount: u32, fs: f32, n: usize) -> Vec<[f32; 2]> {
        let mut st = FwLawStage::new(ty, rate, amount, fs).unwrap();
        (0..n)
            .map(|i| st.process_frame([((i as f32) * 0.05).sin(), ((i as f32) * 0.031).cos()]))
            .collect()
    }

    fn word(st: &FwLawStage, off: i32) -> u32 {
        st.st.ctx[index(off)]
    }

    #[test]
    fn every_type_runs_finite_with_a_four_frame_latency() {
        for ty in TYPES {
            for (rate, amount) in [(0, 0), (64, 64), (127, 127), (30, 100)] {
                let out = run(ty, rate, amount, 48_000.0, 6000);
                assert!(
                    out.iter()
                        .all(|f| f.iter().all(|x| x.is_finite() && x.abs() < 16.0)),
                    "type {ty} rate {rate} amount {amount}"
                );
                assert!(out[..4].iter().all(|f| *f == [0.0, 0.0]), "latency");
                assert!(
                    out.iter().skip(4).any(|f| f[0] != 0.0),
                    "type {ty} rate {rate} amount {amount} is silent"
                );
            }
        }
        assert!(
            FwLawStage::new(4, 0, 0, 48_000.0).is_none(),
            "CHOR has its own stage"
        );
        for ty in [1, 8, 9, 10, 11] {
            assert!(
                FwLawStage::new(ty, 0, 0, 48_000.0).is_none(),
                "type {ty} is not wired"
            );
        }
    }

    #[test]
    fn the_laws_hit_the_captured_samples() {
        for law in d::AMT_0.iter().chain(d::RATE_2) {
            for (k, &v) in d::SAMPLES.iter().enumerate() {
                assert_eq!(
                    to_word(law, eval(law, v)),
                    law.words[k],
                    "off {} v {v}",
                    law.off
                );
            }
        }
        // PHAS 0 rate: the phase increment `0x27cf` at 0 and 1, `0x256145` at 127.
        let l = &d::RATE_0[0];
        assert_eq!(
            (eval(l, 0), eval(l, 127)),
            (0x27cf as f32, 0x25_6145 as f32)
        );
        // Between the samples the value lies between its neighbours.
        let mid = eval(l, 71);
        assert!(mid > eval(l, 63) && mid < eval(l, 79));
    }

    #[test]
    fn flang_center_plus_swing_is_constant() {
        // VERIFIED at the samples of both laws: c[-16] + c[-15] = -100 (organ / piano).
        for (a, b) in d::AMT_2
            .iter()
            .find(|l| l.off == -16)
            .zip(d::AMT_2.iter().find(|l| l.off == -15))
            .into_iter()
            .chain(
                d::RATE_2
                    .iter()
                    .find(|l| l.off == -16)
                    .zip(d::RATE_2.iter().find(|l| l.off == -15)),
            )
        {
            for k in 0..11 {
                let sum = f32::from_bits(a.words[k]) + f32::from_bits(b.words[k]);
                assert!((sum + 100.0).abs() < 1.0e-3, "sample {k}: {sum}");
            }
        }
        let fs = crate::organ::FIRMWARE_FRAME_RATE_HZ as f32;
        let st = FwLawStage::new(2, 20, 90, fs).unwrap();
        let (s, c) = (
            f32::from_bits(word(&st, -16)),
            f32::from_bits(word(&st, -15)),
        );
        assert!((s + c + 100.0).abs() < 1.0e-3, "swing {s} centre {c}");
    }

    #[test]
    fn the_type_switch_defaults_come_out_of_the_laws() {
        // Amount and rate 64 are the values of the type switch: PHAS 0 `c[-28]` (43.47) and the
        // FLANG swing (-14.285) are reproduced within the interpolation error.
        let fs = crate::organ::FIRMWARE_FRAME_RATE_HZ as f32;
        let p = FwLawStage::new(0, 64, 64, fs).unwrap();
        let c28 = f32::from_bits(word(&p, -28));
        assert!((c28 - f32::from_bits(0x422d_de16)).abs() < 0.6, "{c28}");
        let f = FwLawStage::new(2, 64, 64, fs).unwrap();
        let swing = f32::from_bits(word(&f, -16));
        assert!((swing - f32::from_bits(0xc164_907d)).abs() < 0.1, "{swing}");
        let inc = i64::from(word(&f, -20));
        assert!(
            (inc - 0x96da6).abs() < 0x96da6 / 100,
            "phase increment {inc:#x}"
        );
    }

    #[test]
    fn phase_increments_scale_with_the_rate_and_delays_with_the_inverse() {
        let at = |fs: f32| FwLawStage::new(2, 64, 64, fs).unwrap();
        let (lo, hi) = (at(43_402.34), at(86_804.69));
        let inc = |s: &FwLawStage| word(s, -20) as f64;
        assert!(
            (inc(&hi) / inc(&lo) - 0.5).abs() < 1.0e-3,
            "half the increment at twice the rate"
        );
        let dl = |s: &FwLawStage| f64::from(f32::from_bits(word(s, -16)));
        assert!(
            (dl(&hi) / dl(&lo) - 2.0).abs() < 1.0e-3,
            "twice the frames for the same duration"
        );
        let p = |fs: f32| FwLawStage::new(0, 64, 64, fs).unwrap();
        let (a, b) = (p(43_402.34), p(86_804.69));
        assert!(
            (word(&b, -32) as f64 / word(&a, -32) as f64 - 0.5).abs() < 1.0e-3,
            "PHAS 0"
        );
    }

    #[test]
    fn the_variation_types_keep_the_words_of_their_base_type() {
        let fs = 48_000.0;
        let f3 = FwLawStage::new(3, 64, 64, fs).unwrap();
        assert_eq!(word(&f3, -29), 0x3f80_0000, "c[-29] of the type 2 switch");
    }

    #[test]
    fn rate_and_amount_change_the_output() {
        for ty in TYPES {
            assert_ne!(
                run(ty, 20, 90, 48_000.0, 3000),
                run(ty, 100, 90, 48_000.0, 3000),
                "rate {ty}"
            );
            assert_ne!(
                run(ty, 60, 10, 48_000.0, 3000),
                run(ty, 60, 120, 48_000.0, 3000),
                "amount {ty}"
            );
        }
    }
}

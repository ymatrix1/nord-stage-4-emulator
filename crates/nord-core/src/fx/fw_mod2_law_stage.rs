//! Frame-by-frame adapter around the generated firmware Mod 2 routines that are driven by
//! tabulated laws: `PHAS` ([`super::fw_phas0`], `kFx2Type` 0, 1), `FLANG` (2, 3), `ENS` (8, 9) and `SPIN` (10, 11), behind `FirmwareFx::mod2` (opt-in, D22).
//!
//! The routines run once per pass on 2 blocks of two stereo frames, so the stage collects 4
//! frames and releases them one call later: **latency 4 frames (83 us at 48 kHz)**. The delay and
//! rate words are in DSP frames at `FIRMWARE_FRAME_RATE_HZ`: phase increments are scaled by
//! `F / fs`, the delay words listed per type by `fs / F` (a delay keeps its duration in seconds).
//!
//! Provenance (`docs/research/fx-mod2-types.md` s8, `docs/research/fx-mod2-validation.md`): the
//! routines are VERIFIED against the oracle; the instance row and the type words are derived
//! from the ARM capture; the `kFx2Amt` / `kFx2Rate` laws are the 44 knot words of
//! [`super::fw_mod2_dense`] (VERIFIED at every knot, EMPIRICAL between them: linear). Words that
//! both laws write: `PHAS 0` `c[-28]` is bilinear in the rate (joint law, VERIFIED on 3 rates x
//! 44 amounts to 1.3e-7), `FLANG` `c[-16]` is `A(amt) R(rate) / R(64)` (VERIFIED on the same
//! grid to 1e-7). `FLANG` `c[-15] = -100 - c[-16]` (VERIFIED, test
//! `flang_center_plus_swing_is_constant`).

use super::fw_chorus::{CTX_I1, RING_BASE};
use super::fw_mod1_data as boot;
use super::fw_mod2_data as d;
use super::fw_mod2_dense as dn;
use super::fw_mod2_dense::DenseLaw;
use super::fw_mod2_run::{process, Mod2State, Routine, RING2_BASE};
use super::fw_mod2_tables::PHAS_TABLE;

/// Offset of the `PHAS 0` table inside the ring array (`c[0x1A]` points there).
const PHAS_TABLE_AT: usize = 0x100;
const DEFAULT_PARAM: u32 = 64;

/// One wired type: its routine, rows, type words, laws and the words that scale.
struct Spec {
    routine: Routine,
    /// `I1` of the instance row of the piano unit when its words are the image under the boot
    /// words (self pointers are relocated); 0 for the image only rows.
    base: u32,
    row: &'static [(i32, u32)],
    words: &'static [(i32, u32)],
    amt: &'static [DenseLaw],
    rate: &'static [DenseLaw],
    /// Integer words that are phase increments per call (scaled by `F / fs`).
    phase: &'static [i32],
    /// Words that are delays in DSP frames (float; scaled by `fs / F`).
    delay: &'static [i32],
}

fn spec(ty: u8) -> Option<Spec> {
    let (routine, base, row, words, amt, rate, phase, delay): (_, _, _, _, _, _, &[i32], &[i32]) =
        match ty {
            0 => (
                Routine::Phas0,
                0,
                d::ROW_PHAS0,
                d::WORDS_0,
                dn::AMT_0,
                dn::RATE_0,
                &[-32],
                &[],
            ),
            2 => (
                Routine::Flang,
                0,
                d::ROW_FLANG,
                d::WORDS_2,
                dn::AMT_2,
                dn::RATE_2,
                &[-20],
                &[-16, -15],
            ),
            3 => (
                Routine::Flang,
                0,
                d::ROW_FLANG,
                d::WORDS_3,
                dn::AMT_3,
                dn::RATE_3,
                &[-20],
                &[-16, -15],
            ),
            1 => (
                Routine::Phas1,
                0xB0432,
                boot::ROW_PHAS1,
                d::WORDS_1,
                dn::AMT_1,
                dn::RATE_1,
                &[-32],
                &[],
            ),
            8 | 9 => (
                Routine::Ens,
                0xC129E,
                boot::ROW_ENS,
                if ty == 8 { d::WORDS_8 } else { d::WORDS_9 },
                if ty == 8 { dn::AMT_8 } else { dn::AMT_9 },
                if ty == 8 { dn::RATE_8 } else { dn::RATE_9 },
                &[-6],
                &[],
            ),
            10 | 11 => (
                Routine::Spin,
                0xC1174,
                boot::ROW_SPIN,
                if ty == 10 { d::WORDS_10 } else { d::WORDS_11 },
                if ty == 10 { dn::AMT_10 } else { dn::AMT_11 },
                if ty == 10 { dn::RATE_10 } else { dn::RATE_11 },
                &[],
                &[],
            ),
            _ => return None,
        };
    Some(Spec {
        routine,
        base,
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

/// Linear interpolation of `words` between the neighbouring knots at `v` (`0..=127`).
fn knot_value(words: &[u32], float: bool, v: u32) -> f32 {
    let v = v.min(127);
    let k = dn::KNOTS
        .iter()
        .rposition(|&s| s <= v)
        .unwrap_or(0)
        .min(dn::KNOTS.len() - 2);
    let (x0, x1) = (dn::KNOTS[k], dn::KNOTS[k + 1]);
    let t = (v - x0) as f32 / (x1 - x0) as f32;
    let (a, b) = (words[k], words[k + 1]);
    if float {
        let (a, b) = (f32::from_bits(a), f32::from_bits(b));
        a + (b - a) * t
    } else {
        let (a, b) = (a as i32 as f32, b as i32 as f32);
        a + (b - a) * t
    }
}

/// A law at parameter `v` (`0..=127`): linear between the neighbouring knots, exact at them.
fn eval(law: &DenseLaw, v: u32) -> f32 {
    knot_value(&law.words, law.float, v)
}

/// A joint law at `(rate, amount)`: linear in the rate between the words at rate 0 and 127.
fn eval_joint(law: &dn::JointLaw, rate: u32, amount: u32) -> f32 {
    let f0 = knot_value(&law.rate0, true, amount);
    let f1 = knot_value(&law.rate127, true, amount);
    f0 + (f1 - f0) * rate.min(127) as f32 / 127.0
}

fn to_word(law: &DenseLaw, x: f32) -> u32 {
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
    /// Stage for `kFx2Type` `ty` (0 to 3, 8 to 11), `kFx2Rate` and `kFx2Amt` in `0..=127`, at
    /// `fs`; `None` for the other types. ENS (8, 9) runs with `L4 = L7 = 0` like every routine
    /// (D114: its two rings sit `0x202` words apart and are read and written by pre-modify,
    /// no tap register is needed). `kFx2Amt` writes `c[-3]`, `c[2]`, `c[3]`, `c[4]` and `kFx2Rate` the phase increment `c[-6]` (44 knots, D100).
    pub(super) fn new(ty: u8, rate: u32, amount: u32, fs: f32) -> Option<Self> {
        Self::build(ty, rate, amount, fs)
    }

    /// [`Self::new`] with the boot register state (the experiment of the validation note).
    fn build(ty: u8, rate: u32, amount: u32, fs: f32) -> Option<Self> {
        let sp = spec(ty)?;
        let ratio = crate::organ::FIRMWARE_FRAME_RATE_HZ / f64::from(fs);
        let mut st = Mod2State::new();
        for &(o, w) in sp.row.iter().filter(|(o, _)| (-0x20..=0x2A).contains(o)) {
            // Pointer words of the image are placeholders (the ARM allocates the rings); a
            // pointer into the row itself (a boot word) is relocated to the context of the port.
            st.ctx[index(o)] = if (0xB_1000..0xB_2100).contains(&w) {
                if sp.routine == Routine::Spin && (27..=29).contains(&o) {
                    RING2_BASE
                } else {
                    RING_BASE
                }
            } else if sp.base != 0 && (sp.base - 0x20..=sp.base + 0x2A).contains(&w) {
                CTX_I1 + w - sp.base
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
            _ => {}
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
        let norm = |laws: &[DenseLaw], off: i32| {
            laws.iter()
                .find(|l| l.off == off)
                .map(|l| eval(l, DEFAULT_PARAM))
        };
        let joint: &[dn::JointLaw] = if self.ty == 0 { dn::JOINT_0 } else { &[] };
        for j in joint {
            // VERIFIED bilinear in the rate (every dense amount, 1.3e-7 of the range).
            self.st.ctx[index(j.off)] = eval_joint(j, rate, amount).to_bits();
        }
        for law in sp.amt.iter().chain(sp.rate) {
            if joint.iter().any(|j| j.off == law.off) {
                continue;
            }
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
    use super::super::tests_mod2_dense_golden as g;
    use super::*;

    const TYPES: [u8; 8] = [0, 1, 2, 3, 8, 9, 10, 11];

    fn run(ty: u8, rate: u32, amount: u32, fs: f32, n: usize) -> Vec<[f32; 2]> {
        let mut st = FwLawStage::new(ty, rate, amount, fs).unwrap();
        (0..n)
            .map(|i| st.process_frame([((i as f32) * 0.05).sin(), ((i as f32) * 0.031).cos()]))
            .collect()
    }

    fn word_of(st: &FwLawStage, off: i32) -> u32 {
        st.st.ctx[index(off)]
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
                    out.iter().skip(4).any(|f| f[0] != 0.0 || f[1] != 0.0),
                    "type {ty} rate {rate} amount {amount} is silent"
                );
            }
        }
        for ty in [4, 5, 6, 7] {
            assert!(
                FwLawStage::new(ty, 0, 0, 48_000.0).is_none(),
                "type {ty} has its own stage or is not wired"
            );
        }
    }

    #[test]
    fn the_laws_hit_the_captured_samples() {
        for law in dn::AMT_0.iter().chain(dn::RATE_2) {
            for (k, &v) in dn::KNOTS.iter().enumerate() {
                assert_eq!(
                    to_word(law, eval(law, v)),
                    law.words[k],
                    "off {} v {v}",
                    law.off
                );
            }
        }
        // PHAS 0 rate: the phase increment `0x27cf` at 0 and 1, `0x256145` at 127.
        let l = &dn::RATE_0[0];
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
        for (a, b) in dn::AMT_2
            .iter()
            .find(|l| l.off == -16)
            .zip(dn::AMT_2.iter().find(|l| l.off == -15))
            .into_iter()
            .chain(
                dn::RATE_2
                    .iter()
                    .find(|l| l.off == -16)
                    .zip(dn::RATE_2.iter().find(|l| l.off == -15)),
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
    fn the_ensemble_left_voice_runs_with_linear_tap_registers() {
        // Boot row, `L4 = L7 = 0` (the registers no code of the image writes, D114): the left
        // output runs because the left voice lives in a second ring `0x202` words after the
        // first and is reached by a pre-modify, which does not wrap. (Before D114 the model
        // wrapped the pre-modify, left stayed exactly 0 and circular taps were assumed.)
        let mut st = FwLawStage::new(8, 64, 100, 48_000.0).unwrap();
        let mut p = [0.0f32; 2];
        for i in 0..3000 {
            let out = st.process_frame([((i as f32) * 0.05).sin(), ((i as f32) * 0.031).cos()]);
            p[0] = p[0].max(out[0].abs());
            p[1] = p[1].max(out[1].abs());
        }
        assert!(p[0] > 0.1 && p[1] > 0.1, "{p:?}");
    }

    #[test]
    fn the_ens_laws_hit_every_knot() {
        for law in dn::AMT_8
            .iter()
            .chain(dn::RATE_8)
            .chain(dn::AMT_9)
            .chain(dn::RATE_9)
        {
            for (k, &v) in dn::KNOTS.iter().enumerate() {
                assert_eq!(
                    to_word(law, eval(law, v)),
                    law.words[k],
                    "off {} v {v}",
                    law.off
                );
            }
        }
        assert_eq!(dn::AMT_8.len(), 4);
        assert_eq!(dn::RATE_8.len(), 1);
    }

    #[test]
    fn ens_follows_the_arm_amount_and_rate_words_and_is_deterministic() {
        // Dense ARM words (`fx-mod2-dense.csv`, piano unit): rate `c[-6]` 0x3200 at 0 and
        // 0x13000 at 127 (DSP-frame phase increments), amount `c[-3]` 0.33 .. 0.65.
        let at = |ty, r, a| FwLawStage::new(ty, r, a, crate::organ::FIRMWARE_FRAME_RATE_HZ as f32);
        for ty in [8, 9] {
            let lo = at(ty, 0, 0).unwrap();
            let hi = at(ty, 127, 127).unwrap();
            assert_eq!(word(&lo, -6), 0x3200, "type {ty}");
            assert_eq!(word(&hi, -6), 0x1_3000, "type {ty}");
            assert!(f32::from_bits(word(&hi, -3)) > f32::from_bits(word(&lo, -3)));
            assert_eq!(f32::from_bits(word(&lo, 2)), 0.0);
            assert_eq!(
                run(ty, 64, 64, 44_100.0, 2000),
                run(ty, 64, 64, 44_100.0, 2000)
            );
            let st = FwLawStage::new(ty, 64, 64, 48_000.0).unwrap();
            assert_eq!(st.ty(), ty);
        }
    }

    #[test]
    fn the_variation_types_keep_the_words_of_their_base_type() {
        let fs = 48_000.0;
        let f3 = FwLawStage::new(3, 64, 64, fs).unwrap();
        assert_eq!(word(&f3, -29), 0x3f80_0000, "c[-29] of the type 2 switch");
    }

    #[test]
    fn the_boot_words_complete_ens_spin_and_phas1() {
        // The ARM sends the self pointers, the block counts and the all-pass gains at the boot
        // (`fx-mod1-rows.csv`, source `boot`); the image alone left ENS silent.
        let fs = 48_000.0;
        let ens = boot::ROW_ENS.iter().find(|w| w.0 == -5).unwrap();
        assert_eq!(ens.1, 0xc1296, "ENS phase pointer is the address of c[-8]");
        let spin = FwLawStage::new(10, 64, 64, fs).unwrap();
        assert_eq!(word(&spin, -8), CTX_I1 - 10, "SPIN phase pointer");
        assert_eq!(word(&spin, 27), RING2_BASE, "SPIN ring 2");
        let ph = FwLawStage::new(1, 64, 64, fs).unwrap();
        assert_eq!(word(&ph, -29), CTX_I1 - 31, "PHAS 1 phase pointer");
        assert_eq!(word(&ph, -26), 0x3ea0_0000, "all-pass gain 0.3125");
        assert_eq!(word(&ph, -17), 0xbf33_3333, "all-pass gain -0.7");
    }

    #[test]
    fn the_rotor_stays_bounded_over_long_runs() {
        for ty in [10, 11] {
            let out = run(ty, 100, 127, 48_000.0, 200_000);
            let peak = out.iter().flatten().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak.is_finite() && peak < 8.0, "type {ty}: peak {peak}");
        }
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

    #[test]
    fn the_shared_words_follow_the_joint_arm_words() {
        // PHAS 0 `c[-28]` (bilinear in the rate) and FLANG `c[-16]` (`A(amt) R(rate) / R(64)`)
        // at rate 0, 64, 127 against every dense amount (`tests_mod2_dense_golden`).
        for &(ty, off, pts) in g::ARM_JOINT.iter().filter(|w| w.0 == 0 || w.0 == 2) {
            for &(r, a, word) in pts {
                let st =
                    FwLawStage::new(ty, r, a, crate::organ::FIRMWARE_FRAME_RATE_HZ as f32).unwrap();
                let got = f64::from(f32::from_bits(word_of(&st, off)));
                let want = f64::from(f32::from_bits(word));
                // FLANG stores swings in engine frames (`fs / F`), here 1.0
                let tol = 5e-4 * want.abs().max(1.0);
                assert!(
                    (got - want).abs() < tol,
                    "type {ty} c[{off}] r {r} a {a}: {got} {want}"
                );
            }
        }
    }
}

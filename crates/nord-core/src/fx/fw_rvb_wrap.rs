//! Half-rate wrapper of the firmware reverb routines `0x12417E` / `0x12435C` (type 0 and 1).
//!
//! The routines wrap the reverb core `0x123F61` (`fw_rvb_core.rs`, called through `I8`) in a
//! sample-rate converter (`docs/research/fx-reverb-leslie.md` s5):
//!
//! * [`Wrap::process4`] (`0x12417E`, 4 stereo frames): three Butterworth stages at fs/4 low-pass
//!   the frames, frames 0 and 2 go to the core, its two returned frames are held (`w0 w0 w2 w2`),
//!   low-passed by a second cascade and added to the dry frames scaled by `c[0x15]`.
//! * [`mix2`] (`0x12435C`, 2 frames): the core runs on the frames directly, `out = in * g + wet`.
//!
//! VERIFIED (word for word against the `sharc-fx` oracle, `tools/fx_reverb_model.py check4` and
//! `check2`: 6000 of 6000 words over 5 seeds x 30 carried calls, and 96 of 96): the cascade, the
//! hold, the mix and the state layout, **given the core's outputs**; with the core ported
//! (`fw_rvb_core.rs`) `tests_fw_rvb_golden.rs` checks the whole routine against oracle output.
//! The arithmetic is generic over [`Fp`]: [`FwRvbWrap`] is the fast `f32` path, `Wrap<E40>` the
//! 40-bit register path that reproduces the oracle bit for bit. Allocation-free: all state lives
//! in the struct.

use super::biquad::FW_BUTTERWORTH_FS4_WORDS;
use super::fw_ext40::Fp;

const STAGES: usize = 3;
/// `[s1, s2]` per stage and channel.
type Cascade<F> = [[[F; 2]; 2]; STAGES];
/// One stereo frame.
type Frame = [f32; 2];

/// Firmware half-rate wrapper state: the two cascades (states at `I1 + 0x49` and `I1 + 0x55`).
#[derive(Debug, Clone, Default)]
pub struct Wrap<F: Fp> {
    bank1: Cascade<F>,
    bank2: Cascade<F>,
}

/// The fast single-precision wrapper.
pub type FwRvbWrap = Wrap<f32>;

/// ASSUMED naming only; VERIFIED words: PM `0x9437A` and `0x9437F` hold the same coefficients.
fn coef<F: Fp>() -> [F; 5] {
    FW_BUTTERWORTH_FS4_WORDS.map(F::from_word)
}

/// One cascade over 4 frames: each stage reads and writes the whole block, then the next stage.
///
/// The biquad step is the firmware's order (VERIFIED, `tools/fx_biquad_check.py biquad_step`):
/// `y = b0 x + s1`, `s1 = (b1 x + s2) - a1 y`, `s2 = b2 x - a2 y`; outputs and the final states are
/// stored (truncated) once per block.
fn cascade<F: Fp>(c: &[F; 5], st: &mut Cascade<F>, x: &mut [[F; 2]; 4]) {
    let [b0, b1, b2, a1, a2] = *c;
    for stage in st.iter_mut() {
        for (ch, z) in stage.iter_mut().enumerate() {
            for f in x.iter_mut() {
                let v = f[ch];
                let y = b0.mul(v).add(z[0]);
                let t = b1.mul(v).add(z[1]);
                z[0] = t.sub(a1.mul(y));
                z[1] = b2.mul(v).sub(a2.mul(y));
                f[ch] = y.store();
            }
            *z = [z[0].store(), z[1].store()];
        }
    }
}

fn to_f<F: Fp>(f: Frame) -> [F; 2] {
    f.map(F::from_f32)
}

/// `out = x * g + w` on one frame: the product is a register value, the sum is stored.
fn mix_frame<F: Fp>(x: Frame, g: f32, w: Frame) -> Frame {
    let (x, w) = (to_f::<F>(x), to_f::<F>(w));
    let g = F::from_f32(g);
    [0, 1].map(|i| x[i].mul(g).add(w[i]).to_f32())
}

/// `0x12435C`: two frames, `wet` is the core's `(R12, R14)`.
pub fn mix2(frames: &mut [Frame; 2], gain: f32, wet: (Frame, Frame)) {
    frames[0] = mix_frame::<f32>(frames[0], gain, wet.0);
    frames[1] = mix_frame::<f32>(frames[1], gain, wet.1);
}

impl<F: Fp> Wrap<F> {
    /// Zero states.
    pub fn new() -> Self {
        Self::default()
    }

    /// `0x12417E` on 4 frames in place. `core(frame0, frame2)` returns the reverb core's two
    /// output frames; `gain` is `c[0x15]` as the core leaves it (the core rewrites that word).
    pub fn process4(
        &mut self,
        frames: &mut [Frame; 4],
        gain: f32,
        mut core: impl FnMut(Frame, Frame) -> (Frame, Frame),
    ) {
        self.process4_late(frames, |a, b| {
            let (w0, w2) = core(a, b);
            (w0, w2, gain)
        });
    }

    /// [`Self::process4`] for a core that also reports `c[0x15]` after its call (the mix reads the
    /// word the core has just rewritten).
    pub fn process4_late(
        &mut self,
        frames: &mut [Frame; 4],
        mut core: impl FnMut(Frame, Frame) -> (Frame, Frame, f32),
    ) {
        let c = coef::<F>();
        let mut y = frames.map(to_f::<F>);
        cascade(&c, &mut self.bank1, &mut y);
        let (w0, w2, gain) = core(y[0].map(F::to_f32), y[2].map(F::to_f32));
        let (w0, w2) = (to_f::<F>(w0), to_f::<F>(w2));
        let mut z = [w0, w0, w2, w2];
        cascade(&c, &mut self.bank2, &mut z);
        for (f, zz) in frames.iter_mut().zip(z) {
            *f = mix_frame::<F>(*f, gain, zz.map(F::to_f32));
        }
    }

    /// The states as the context words `I1 + 0x49 ..` (`s1L s1R s2L s2R` per stage, bank 1 then 2).
    pub fn state_words(&self) -> [f32; 24] {
        let mut out = [0.0; 24];
        for (b, bank) in [&self.bank1, &self.bank2].into_iter().enumerate() {
            for (k, s) in bank.iter().enumerate() {
                let o = 12 * b + 4 * k;
                let v = [s[0][0], s[1][0], s[0][1], s[1][1]].map(F::to_f32);
                out[o..o + 4].copy_from_slice(&v);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_gain_and_no_wet_is_silent() {
        let mut w = FwRvbWrap::new();
        let mut f = [[0.5, -0.5]; 4];
        w.process4(&mut f, 0.0, |_, _| ([0.0; 2], [0.0; 2]));
        assert!(f.iter().flatten().all(|v| *v == 0.0), "gain 0 and no wet");
        let mut f = [[0.5, -0.5]; 4];
        w.process4(&mut f, 1.0, |_, _| ([0.0; 2], [0.0; 2]));
        assert!(
            f.iter().all(|fr| fr[0] >= 0.5 && fr[1] <= -0.5 + 1e-6),
            "dry passes at unity"
        );
    }

    #[test]
    fn the_core_sees_frames_0_and_2_of_the_low_passed_block_and_its_output_is_held() {
        let mut seen = None;
        let mut w = FwRvbWrap::new();
        let mut f = [[1.0, 0.0], [0.0; 2], [0.0; 2], [0.0; 2]];
        w.process4(&mut f, 0.0, |a, b| {
            seen = Some((a, b));
            ([0.0; 2], [0.0; 2])
        });
        let (a, b) = seen.expect("core called once");
        assert!(
            (a[0] - 0.292_893_2_f32.powi(3)).abs() < 1e-5,
            "b0^3: impulse through 3 stages"
        );
        assert!(b[0] > 0.0 && a[1] == 0.0);
        let mut w = FwRvbWrap::new();
        let mut f = [[0.0; 2]; 4];
        w.process4(&mut f, 0.0, |_, _| ([1.0, 1.0], [0.0; 2]));
        assert!(
            f[0][0] > 0.0 && f[1][0] > f[0][0],
            "held pair then low pass: rising"
        );
    }

    #[test]
    fn mix2_adds_the_wet_frames_after_the_gain() {
        let mut f = [[1.0, 2.0], [3.0, 4.0]];
        mix2(&mut f, 0.5, ([10.0, 20.0], [30.0, 40.0]));
        assert_eq!(f, [[10.5, 21.0], [31.5, 42.0]]);
    }

    #[test]
    fn state_words_follow_the_context_layout() {
        let mut w = FwRvbWrap::new();
        w.bank1[0] = [[1.0, 2.0], [3.0, 4.0]];
        w.bank2[2] = [[5.0, 6.0], [7.0, 8.0]];
        let s = w.state_words();
        assert_eq!(&s[0..4], &[1.0, 3.0, 2.0, 4.0]);
        assert_eq!(&s[20..24], &[5.0, 7.0, 6.0, 8.0]);
    }
}

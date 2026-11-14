//! The firmware reverb of types 0 and 1 (Spring, Spring 2): `0x12417E` around the core `0x123F61`.
//!
//! [`Reverb`] joins the half-rate wrapper ([`Wrap`]) and the ported core ([`Core`]) and runs them
//! on a stream of stereo frames at 48 kHz: every 4 input frames one call of the routine, so the
//! stage has a **latency of 4 frames** (83 us). The routine does the whole mix itself
//! (`out = in * c[0x15] + wet`, the core smooths the `kRvbAmt` words `c[0x12]`, `c[0x13]` into
//! `c[0x14]`, `c[0x15]`), so the engine's reverb send / dry is replaced, not added to.
//!
//! The type is data: the 39 type words of `kRvbType` 0 or 1 (`fw_rvb_words.rs`) set the two taps
//! (`-2658` and `-4068` words for type 0) and the diffusion constants. VERIFIED against the
//! `sharc-fx` oracle with the [`E40`] arithmetic (`tests_fw_rvb_golden.rs`); `f32` is the fast
//! path. All memory is allocated in [`Reverb::new`], none while processing.

use super::fw_ext40::{Fp, E40};
use super::fw_map;
use super::fw_rvb_core::Core;
use super::fw_rvb_wrap::Wrap;

/// One stereo frame.
pub type Frame = [f32; 2];

/// The firmware reverb on the register number format `F`.
#[derive(Clone, Debug)]
pub struct Reverb<F: Fp> {
    wrap: Wrap<F>,
    core: Core<F>,
    rvb_type: usize,
    last_ui: f32,
    inp: [Frame; 4],
    out: [Frame; 4],
    n: usize,
}

/// The bit-exact path: 40-bit registers as the DSP has them.
pub type FwReverb = Reverb<E40>;
/// The fast path: single precision registers.
pub type FwReverbF32 = Reverb<f32>;

impl<F: Fp> Reverb<F> {
    /// The reverb of `kRvbType` `rvb_type` (0 Spring, 1 Spring 2), amount 127, cleared buffers.
    pub fn new(rvb_type: usize) -> Self {
        Self {
            wrap: Wrap::new(),
            core: Core::new(rvb_type),
            rvb_type: rvb_type.min(1),
            last_ui: f32::NAN,
            inp: [[0.0; 2]; 4],
            out: [[0.0; 2]; 4],
            n: 0,
        }
    }

    /// The `kRvbType` this instance runs.
    pub fn rvb_type(&self) -> usize {
        self.rvb_type
    }

    /// Sets the `kRvbAmt` words from the UI value `0..=127` (`fw_map::reverb_send` / `reverb_dry`).
    /// The firmware smooths the change (pole 0.9913 per call); `settle` jumps to it instead.
    pub fn set_amount(&mut self, ui: f32, settle: bool) {
        if ui == self.last_ui && !settle {
            return;
        }
        self.last_ui = ui;
        let (send, dry) = (fw_map::reverb_send(ui), fw_map::reverb_dry(ui));
        self.set_amount_words(send, dry, settle);
    }

    /// Sets the send word `c[0x12]` and the dry word `c[0x13]` directly.
    pub fn set_amount_words(&mut self, send: f32, dry: f32, settle: bool) {
        if settle {
            self.core.set_amount_words(send, dry);
        } else {
            self.core.set_ctx(0x12, send.to_bits());
            self.core.set_ctx(0x13, dry.to_bits());
        }
    }

    /// One call of the routine `0x12417E` on 4 frames, in place (no latency beyond the block).
    pub fn process_block4(&mut self, frames: &mut [Frame; 4]) {
        let core = &mut self.core;
        self.wrap.process4_late(frames, |a, b| {
            let (w0, w2) = core.call(a, b);
            (w0, w2, f32::from_bits(core.ctx(0x15)))
        });
    }

    /// One frame in, one frame out, 4 frames late.
    pub fn process_frame(&mut self, x: Frame) -> Frame {
        let y = self.out[self.n];
        self.inp[self.n] = x;
        self.n += 1;
        if self.n == 4 {
            self.n = 0;
            let mut block = self.inp;
            self.process_block4(&mut block);
            self.out = block;
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_in_silence_out_and_four_frames_of_latency() {
        let mut r = FwReverbF32::new(0);
        for _ in 0..16 {
            assert_eq!(r.process_frame([0.0; 2]), [0.0; 2]);
        }
        let mut r = FwReverbF32::new(0);
        r.set_amount(0.0, true);
        let first: Vec<Frame> = (0..8)
            .map(|k| r.process_frame(if k == 0 { [0.5, -0.5] } else { [0.0; 2] }))
            .collect();
        assert!(
            first[..4].iter().all(|f| *f == [0.0; 2]),
            "nothing before the block ends"
        );
        assert!(
            first[4][0] > 0.1,
            "dry path: the impulse comes out 4 frames late"
        );
    }

    #[test]
    fn the_wet_path_starts_at_the_first_tap() {
        // `-2658` words = 1329 pairs: the first wet output is one tap after the impulse's call
        let mut r = FwReverb::new(0);
        let mut first_wet = None;
        for k in 0..6000usize {
            let mut f = [[0.0f32; 2]; 4];
            if k == 0 {
                f[0] = [1.0, 1.0];
            }
            r.process_block4(&mut f);
            if k > 8 && first_wet.is_none() && f.iter().flatten().any(|v| v.abs() > 1e-9) {
                first_wet = Some(k);
            }
        }
        let k = first_wet.expect("a tail");
        assert!(
            (1330..=1400).contains(&k),
            "first wet call {k} (oracle: 1335)"
        );
    }

    #[test]
    fn the_amount_words_follow_the_kr_amount_law() {
        let mut r = FwReverbF32::new(1);
        assert_eq!(r.rvb_type(), 1);
        r.set_amount(64.0, true);
        assert_eq!(f32::from_bits(r.core.ctx(0x12)), 0.375);
        assert!((f32::from_bits(r.core.ctx(0x13)) - 0.927_025).abs() < 1e-6);
        r.set_amount(127.0, false);
        assert_eq!(
            f32::from_bits(r.core.ctx(0x14)),
            0.375,
            "smoothed copy not yet moved"
        );
    }
}

//! The firmware multi-tap reverb of types 2..11 (module `0xC02BB44C`): wrapper `0x120FDB` around
//! the packed core `0x120D8A`, as an opt-in effect.
//!
//! [`ReverbMulti`] runs the generated instruction-level port (`fw_rvbm_steps_wrap.rs`,
//! `fw_rvbm_steps_core.rs`) on the register machine of `fw_leslie_core.rs`, on the real instance
//! of the synth unit L6 (core 2 row 0: DM `0xC1A28`, `fw_rvbm_words.rs`) with the ARM's type words
//! of `kRvbType`. The routine takes **4 stereo frames per call**, filters them with three fs/4
//! biquads, runs the two-ring packed multi-tap core (FPACK / FUNPACK 16:16 words, a 20,480 word
//! line in SDRAM and a 3,140 word ring in DM), filters the result again and mixes it itself
//! (`c[42]`, `c[43]`), so the stage has a **latency of 4 frames** and replaces the engine's reverb
//! send / dry. VERIFIED word for word against the `sharc-fx` oracle by `tests_fw_rvbm_golden.rs`
//! with the [`E40`] arithmetic; `f32` is the fast path. All memory is allocated in
//! [`ReverbMulti::new`]; none while processing. Organ unit rows (zero in the image) and type 6
//! (no captured block) are not covered: [`ReverbMulti::new`] returns `None` for them.

use super::biquad::FW_BUTTERWORTH_FS4_WORDS;
use super::fw_ext40::{Fp, E40};
use super::fw_leslie_core::Core;
use super::fw_map;
use super::fw_rvbm_words as words;

/// One stereo frame.
pub type Frame = [f32; 2];

/// VERIFIED (`tools/fx_rvbmulti.py`, oracle harness): the 4 frames of a call sit in place at `IN`.
const IN: u32 = 0xB1000;
/// First PM word of the stage coefficients (`0x9437A`, `0x9437F`) and the type list behind them.
const PM: (u32, u32) = (0x94370, 0x90);
/// VERIFIED (instance row, `c[-24] = 0xE0000`, `c[-23] = 3140`; ARM FIR words `0xE0C46 ..`).
const RING: (u32, u32) = (0xE0000, 0xC60);
/// VERIFIED (instance row `c[-15 .. -13]` = `0x08020000`, `0x5000`): the SDRAM line, 20,480 words.
const LINE: (u32, u32) = (0x0802_0000, 0x5000);
/// Stage coefficient addresses of the first and the second bank (the same Butterworth words).
const BANKS: [u32; 2] = [0x9437A, 0x9437F];

/// The firmware multi-tap reverb on the register number format `F`.
#[derive(Clone, Debug)]
pub struct ReverbMulti<F: Fp> {
    core: Core<F>,
    rvb_type: usize,
    last_ui: f32,
    inp: [Frame; 4],
    out: [Frame; 4],
    n: usize,
}

/// The bit-exact path: 40-bit registers as the DSP has them.
pub type FwReverbMulti = ReverbMulti<E40>;
/// The fast path: single precision registers.
pub type FwReverbMultiF32 = ReverbMulti<f32>;

impl<F: Fp> ReverbMulti<F> {
    /// The `kRvbType` values this port covers (2..=5 and 7..=11).
    pub const TYPES: [usize; 9] = words::TYPES;

    /// The reverb of `kRvbType` `rvb_type` at amount 127 (wet only), cleared buffers; `None` for
    /// a type without captured words (0 and 1 are `FwReverb`, 6 has no block).
    pub fn new(rvb_type: usize) -> Option<Self> {
        let k = words::TYPES.iter().position(|&t| t == rvb_type)?;
        let core = Core::<F>::blank(&[(words::ROW, words::ROW_WORDS), (IN, 8), PM, RING, LINE]);
        let mut r = Self {
            core,
            rvb_type,
            last_ui: f32::NAN,
            inp: [[0.0; 2]; 4],
            out: [[0.0; 2]; 4],
            n: 0,
        };
        r.load(k);
        Some(r)
    }

    /// Switches to `rvb_type` in place: clears every buffer and loads the instance and the type
    /// words, without allocating. Returns `false` (and keeps the state) for an uncovered type.
    pub fn set_type(&mut self, rvb_type: usize) -> bool {
        match words::TYPES.iter().position(|&t| t == rvb_type) {
            Some(k) => {
                self.rvb_type = rvb_type;
                self.load(k);
                true
            }
            None => false,
        }
    }

    /// Zeroes the memory, writes the instance row, the type words `k` and the two stage banks,
    /// sets the DAG registers the pass context provides and the amount to 127 (wet only).
    fn load(&mut self, k: usize) {
        let core = &mut self.core;
        core.clear_memory();
        for &(a, w) in &words::INSTANCE {
            core.wr(a, w);
        }
        for &(a, w) in &words::TYPE_WORDS[k] {
            core.wr(a, w);
        }
        for base in BANKS {
            for (j, w) in FW_BUTTERWORTH_FS4_WORDS.into_iter().enumerate() {
                core.wr(base + j as u32, w);
            }
        }
        core.i[0] = IN;
        core.i[1] = words::CTX;
        for (n, v) in [(0, 0), (1, 1), (2, 2), (9, 1), (10, 2)] {
            core.m[n] = v;
        }
        self.last_ui = f32::NAN;
        self.inp = [[0.0; 2]; 4];
        self.out = [[0.0; 2]; 4];
        self.n = 0;
        self.set_amount_words(1.0, 0.0, true);
    }

    /// The `kRvbType` this instance runs.
    pub fn rvb_type(&self) -> usize {
        self.rvb_type
    }

    /// Sets the `kRvbAmt` words from the UI value `0..=127` (`fw_map::reverb_send` / `reverb_dry`).
    pub fn set_amount(&mut self, ui: f32, settle: bool) {
        if ui == self.last_ui && !settle {
            return;
        }
        self.last_ui = ui;
        self.set_amount_words(fw_map::reverb_send(ui), fw_map::reverb_dry(ui), settle);
    }

    /// Sets the send word `c[-31]` and the dry word `c[-30]`; `settle` also jumps the smoothed
    /// copy `c[-29]` (pole 0.9913 per call) to the send word.
    pub fn set_amount_words(&mut self, send: f32, dry: f32, settle: bool) {
        self.core.wr(words::CTX - 31, send.to_bits());
        self.core.wr(words::CTX - 30, dry.to_bits());
        if settle {
            self.core.wr(words::CTX - 29, send.to_bits());
        }
    }

    /// One call of the routine `0x120FDB` on 4 frames, in place (no latency beyond the block).
    pub fn process_block4(&mut self, frames: &mut [Frame; 4]) {
        let c = &mut self.core;
        for (k, v) in frames.iter().flatten().enumerate() {
            c.wr(IN + k as u32, v.to_bits());
        }
        c.i[0] = IN;
        c.i[1] = words::CTX;
        c.rvbm_wrapper();
        for (k, v) in frames.iter_mut().flatten().enumerate() {
            *v = f32::from_bits(c.rd(IN + k as u32));
        }
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

    /// Instructions executed by the last call (tests).
    #[cfg(test)]
    pub(super) fn ticks(&self) -> usize {
        self.core.ticks()
    }

    /// Stores outside every window since construction (tests).
    #[cfg(test)]
    pub(super) fn stray(&self) -> u32 {
        self.core.stray()
    }
}

#[cfg(test)]
mod tests {
    use super::super::fw_rvbm_steps_core::STATIC_STEPS_CORE;
    use super::super::fw_rvbm_steps_wrap::STATIC_STEPS_WRAP;
    use super::*;

    #[test]
    fn covers_the_nine_captured_types_only() {
        for t in [2, 3, 4, 5, 7, 8, 9, 10, 11] {
            assert!(FwReverbMultiF32::new(t).is_some(), "type {t}");
        }
        for t in [0, 1, 6, 12] {
            assert!(FwReverbMultiF32::new(t).is_none(), "type {t}");
        }
    }

    #[test]
    fn silence_stays_silent_and_a_block_has_four_frames_of_latency() {
        let mut r = FwReverbMultiF32::new(2).unwrap();
        for _ in 0..16 {
            assert_eq!(r.process_frame([0.0; 2]), [0.0; 2]);
        }
        let mut r = FwReverbMultiF32::new(2).unwrap();
        let out: Vec<Frame> = (0..8)
            .map(|k| r.process_frame(if k == 0 { [0.5, -0.5] } else { [0.0; 2] }))
            .collect();
        assert!(
            out[..4].iter().all(|f| *f == [0.0; 2]),
            "nothing before the block ends"
        );
    }

    #[test]
    fn a_call_runs_the_same_instructions_every_time_and_stays_inside_its_memory() {
        let mut r = FwReverbMultiF32::new(11).unwrap();
        let mut f = [[0.1, -0.1]; 4];
        r.process_block4(&mut f);
        let first = r.ticks();
        let lines = STATIC_STEPS_CORE + STATIC_STEPS_WRAP;
        assert!(
            first > lines,
            "the loop repeats its body: {first} against {lines} lines"
        );
        r.process_block4(&mut f);
        assert_eq!(r.ticks(), 2 * first);
        assert_eq!(r.stray(), 0, "no store outside the windows");
    }

    #[test]
    fn the_amount_words_follow_the_kr_amount_law() {
        let mut r = FwReverbMultiF32::new(3).unwrap();
        r.set_amount(63.0, true);
        assert_eq!(
            r.core.rd(words::CTX - 31),
            0x3EBB_0410,
            "captured send word of kRvbAmt 63"
        );
        assert_eq!(
            r.core.rd(words::CTX - 30),
            0x3F6E_4FB0,
            "captured dry word of kRvbAmt 63"
        );
    }

    #[test]
    fn switching_the_type_in_place_equals_a_fresh_instance() {
        let mut a = FwReverbMultiF32::new(2).unwrap();
        let mut noise = [[0.3, -0.2]; 4];
        for _ in 0..50 {
            a.process_block4(&mut noise);
        }
        assert!(a.set_type(9) && a.rvb_type() == 9);
        assert!(!a.set_type(6), "type 6 has no captured block");
        assert_eq!(a.rvb_type(), 9);
        let mut b = FwReverbMultiF32::new(9).unwrap();
        for k in 0..400 {
            let mut x = [[if k == 0 { 1.0 } else { 0.0 }; 2]; 4];
            let mut y = x;
            a.process_block4(&mut x);
            b.process_block4(&mut y);
            assert_eq!(x, y, "call {k}");
        }
    }
}

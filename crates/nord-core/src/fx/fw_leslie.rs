//! The firmware Leslie processor (`0x1256AD`, PNO core 1) as an opt-in rotary.
//!
//! [`Leslie`] wraps the generated instruction-level port (`fw_leslie_core.rs`,
//! `fw_leslie_main_*.rs`, `fw_leslie_callee.rs`): rotor phase accumulators, 65-point rotor tables,
//! delay lines and the two cabinet filters, 2 frames per call, run on the boot state of OS v1.66
//! (`docs/research/fx-leslie.md`). The arithmetic is generic over [`Fp`]: [`FwLeslieE40`]
//! reproduces the oracle bit for bit, [`FwLeslie`] is the single precision fast path. Allocation
//! free after construction.

use super::fw_ext40::{Fp, E40};
use super::fw_leslie_core::Core;

/// Slow speed words as the ARM re-sends them (`0xB2F25`, `0xB2F26`, `0xB2FA1`).
pub const SLOW_SPEED_WORDS: [u32; 3] = [0x22b00, 0x26000, 0x15d00];

/// Leslie processor state and a one frame latency stereo interface.
#[derive(Clone, Debug)]
pub struct Leslie<F: Fp> {
    core: Core<F>,
    held: Option<[f32; 2]>,
    out: [[f32; 2]; 2],
}

/// Single precision path.
pub type FwLeslie = Leslie<f32>;
/// 40-bit register path (word for word with the oracle).
pub type FwLeslieE40 = Leslie<E40>;

impl<F: Fp> Default for Leslie<F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: Fp> Leslie<F> {
    /// The processor in the boot state, rotor running at the slow speed words.
    pub fn new() -> Self {
        Leslie {
            core: Core::new(),
            held: None,
            out: [[0.0; 2]; 2],
        }
    }

    /// Sets the rotor speed words (`0xB2F25`, `0xB2F26`, `0xB2FA1`).
    pub fn set_speed_words(&mut self, words: [u32; 3]) {
        self.core.set_speed_words(words);
    }

    /// One call: frames `[L0, R0, L1, R1]` processed in place by the firmware routine.
    pub fn process_block(&mut self, io: [f32; 4]) -> [f32; 4] {
        self.core.call(io)
    }

    /// One stereo frame in, one out, with a latency of one frame (the routine takes 2 frames):
    /// the first frame of a pair returns the second output of the previous call.
    pub fn process(&mut self, l: f32, r: f32) -> [f32; 2] {
        match self.held.take() {
            None => {
                self.held = Some([l, r]);
                self.out[1]
            }
            Some(h) => {
                let o = self.core.call([h[0], h[1], l, r]);
                self.out = [[o[0], o[1]], [o[2], o[3]]];
                self.out[0]
            }
        }
    }
}

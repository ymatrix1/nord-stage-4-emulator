//! Firmware rule that picks the SY1SY2 interpolation kernel of a voice.
//!
//! The DSP never writes a voice's kernel pointer; the ARM voice update function `0xC01E00AC`
//! does, from a compare chain on one voice-object field (listing in `tools/arm_sy_kernel_rule.py`,
//! evidence in `docs/research/synth-sample-kernel-rule.md`):
//!
//! | key (`[object + 8]`) | kernel | `fc` | DM base |
//! |---|---|---|---|
//! | `<= 47` | 0 | 1.100 | `0xE3400` |
//! | `48..=59` | 1 | 1.050 | `0xE4000` |
//! | `60..=71` | 2 | 1.000 | `0xE4C00` |
//! | `72..=83` | 3 | 0.960 | `0xE5800` |
//! | `84..=95` | 4 | 0.925 | `0xE6400` |
//! | `>= 96` | 5 | 0.895 | `0xE7000` |
//!
//! The choice depends on the played key, not on the playback ratio.

use super::{SincKernel, SincKernelSet};

/// Upper key (inclusive) of kernels 0..4; kernel 5 takes every higher key.
///
/// VERIFIED: the immediates of the five `cmp r3, #imm` of the chain at `0xC01E1434..0xC01E146C`
/// (`0x5F, 0x53, 0x47, 0x3B, 0x2F` tested with `gt`), reproduced for keys 0..=127 by running the
/// firmware instructions in `tools/arm_sy_kernel_rule.py`; golden test below.
pub const KEY_BOUNDS: [u8; 5] = [47, 59, 71, 83, 95];

/// DM word address of kernel `k`. VERIFIED: `docs/research/sy-dsp-map.md` section 2 and the six
/// channel records of the ARM static initialiser (`0xE3400`, `0xE4000`, `0xE4C00`, `0xE5800`,
/// `0xE6400`, `0xE7000`).
pub const fn kernel_dm_base(kernel: usize) -> u32 {
    0xE3400 + 0xC00 * kernel as u32
}

/// Kernel index the firmware selects for a voice key.
///
/// VERIFIED (all 128 keys, firmware instructions run in `tools/arm_sy_kernel_rule.py`). That the
/// compared field `[object + 8]` is the MIDI key (and not, say, key plus transposition) is
/// ASSUMED: the pitch code of the same function subtracts `0x3C` (60) from it.
pub const fn kernel_for_key(key: u8) -> usize {
    let mut k = 0;
    while k < KEY_BOUNDS.len() && key > KEY_BOUNDS[k] {
        k += 1;
    }
    k
}

impl SincKernelSet {
    /// Kernel the firmware uses for a voice key, see [`kernel_for_key`]. Allocation-free.
    pub fn for_key(&self, key: u8) -> &SincKernel {
        self.get(kernel_for_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinc::CUTOFFS;

    /// Output of `python tools/arm_sy_kernel_rule.py bank0_CODE.bin bank0_SRAM.bin` (OS v1.66):
    /// the first key of each run and its DM base. Derived numbers only.
    const FIRMWARE_RUNS: [(u8, u32); 6] = [
        (0, 0xE3400),
        (48, 0xE4000),
        (60, 0xE4C00),
        (72, 0xE5800),
        (84, 0xE6400),
        (96, 0xE7000),
    ];

    #[test]
    fn rule_matches_the_firmware_runs_for_every_key() {
        for key in 0..=127u8 {
            let run = FIRMWARE_RUNS
                .iter()
                .rev()
                .find(|(lo, _)| key >= *lo)
                .unwrap();
            assert_eq!(kernel_dm_base(kernel_for_key(key)), run.1, "key {key}");
        }
    }

    #[test]
    fn higher_keys_get_a_lower_cutoff_and_the_bounds_are_octave_spaced() {
        for w in KEY_BOUNDS.windows(2) {
            assert_eq!(w[1] - w[0], 12);
        }
        let fcs: Vec<f64> = (0..=127u8).map(|k| CUTOFFS[kernel_for_key(k)]).collect();
        assert!(fcs.windows(2).all(|w| w[1] <= w[0]));
        assert_eq!((fcs[0], fcs[127]), (1.10, 0.895));
    }

    #[test]
    fn for_key_returns_the_kernel_of_that_index() {
        let set = SincKernelSet::new();
        assert_eq!(set.for_key(60).cutoff_f64(), 1.0);
        assert_eq!(set.for_key(0).cutoff_f64(), 1.10);
        assert_eq!(set.for_key(127).cutoff_f64(), 0.895);
        assert_eq!(kernel_for_key(255), 5);
    }
}

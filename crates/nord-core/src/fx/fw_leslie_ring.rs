//! History ring pointer advance of the Leslie cabinet contexts.
//!
//! The cabinet row routine (`0x1237F8`) writes its input at `c[-10] + c[-5]` and reads the FIR taps
//! at `c[-10] + c[-4]` but never stores `c[-10]`. The firmware advances the pointer in the PNO
//! core 1 pass routine, once per pass (`0x12116C..0x1211A6`, `docs/research/fx-leslie.md` s10):
//! `c[-10]` and the second buffer pointer `c[-8]` of the contexts `0xE359E` (left) and `0xE31A6`
//! (right) move by `c[-6] = 4` words (one call of the processor: 2 frames, 2 lanes), circular in
//! the buffers `c[-27]` / `c[-28]` (`0xE3200`, `0x32A` words and `0xE2E08`) and `c[-20]` / `c[-19]`
//! (`0x40` words). A counter word `DM(0xC00DC)` (1 at boot, 8 after the re-base) re-bases both
//! pointers to the buffer starts every 8th pass in the oracle's booted state
//! ([`RingAdvance::PASS_RESET8`]); that sawtooth is about 25 dB too bright against the hardware
//! clip, the plain advance ([`RingAdvance::ADVANCE`]) fits it (`fx-leslie.md` s10).
//!
//! No allocation.

/// Pointer words `c[-10]` of the left and right context and their ring bases (VERIFIED: boot log
/// words, `tools/fx_leslie_ir.py words`).
const RINGS: [(u32, u32); 2] = [(0xE3594, 0xE3200), (0xE319C, 0xE2E08)];
/// Second buffer pointer words `c[-8]` and bases `c[-20]` (VERIFIED, same source).
const SECOND: [(u32, u32); 2] = [(0xE3596, 0xE359E), (0xE319E, 0xE31A6)];
/// Ring length `c[-28]` and second buffer length `c[-19]` (VERIFIED, same source).
const RING_LEN: u32 = 0x32A;
const SECOND_LEN: u32 = 0x40;

/// How the history pointers move between calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingAdvance {
    /// Words per call (`0` keeps the ring frozen, as the first oracle runs did).
    pub stride: u32,
    /// Re-base to the buffer start every `period` calls (`0`: never, wrap at the ring length).
    pub period: u32,
}

impl RingAdvance {
    /// The pointers never move (the state of the harness before `fx-leslie.md` s10).
    pub const FROZEN: Self = Self {
        stride: 0,
        period: 0,
    };
    /// The pass routine of the booted oracle: 4 words per call (`c[-6]`), re-based every 8 passes
    /// (`DM(0xC00DC)`). VERIFIED: `sharc-pass --watch 0xe3594:1` on PNO core 1 over the boot log
    /// shows the pointer at `0xE3200 + 4 (n mod 8)` in pass `n` (`0xE3200, 0xE3204, .. 0xE321C,
    /// 0xE3200`); the hardware clip contradicts it (`fx-leslie.md` s10), so the ARM probably
    /// changes `DM(0xC00DC)` at run time (not in the boot log).
    pub const PASS_RESET8: Self = Self {
        stride: 4,
        period: 8,
    };
    /// 4 words per call, wrapping at the ring length, never re-based. EMPIRICAL: the stride is the
    /// verified `c[-6]`; the absence of the re-base is the reading that fits the B3 clip
    /// (`organ_b3` score 3.13 against 7.55 of the engine rotary, `fx-leslie.md` s10).
    pub const ADVANCE: Self = Self {
        stride: 4,
        period: 0,
    };

    /// Words advanced before call `k`, before the wrap at a buffer length.
    const fn raw(self, k: u64) -> u64 {
        if self.period != 0 {
            self.stride as u64 * (k % self.period as u64)
        } else {
            self.stride as u64 * k
        }
    }

    /// The offset in words of call `k` from the start of the history ring.
    pub const fn offset(self, k: u64) -> u32 {
        (self.raw(k) % RING_LEN as u64) as u32
    }

    /// `(address, value)` of the four pointer words before call `k`.
    pub(super) fn pointers(self, k: u64) -> [(u32, u32); 4] {
        let (o, o2) = (self.offset(k), (self.raw(k) % SECOND_LEN as u64) as u32);
        [
            (RINGS[0].0, RINGS[0].1 + o),
            (RINGS[1].0, RINGS[1].1 + o),
            (SECOND[0].0, SECOND[0].1 + o2),
            (SECOND[1].0, SECOND[1].1 + o2),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pass_routine_pointer_is_a_sawtooth_of_period_eight() {
        let f = RingAdvance::PASS_RESET8;
        let seq: Vec<u32> = (0..10).map(|k| f.offset(k)).collect();
        assert_eq!(seq, [0, 4, 8, 12, 16, 20, 24, 28, 0, 4]);
        assert_eq!(f.pointers(1)[0], (0xE3594, 0xE3204));
        assert_eq!(f.pointers(9)[1], (0xE319C, 0xE2E0C));
        assert_eq!(f.pointers(7)[2], (0xE3596, 0xE359E + 28));
    }

    #[test]
    fn without_a_period_the_pointer_wraps_at_the_ring_length() {
        let m = RingAdvance::ADVANCE;
        assert_eq!(m.offset(0x32A), 0);
        assert_eq!(m.offset(1), 4);
        let k = u64::from(RING_LEN) / 2 + 1; // 4 k = RING_LEN + 4: wrapped
        assert_eq!(m.offset(k), 4);
        assert_eq!(m.pointers(k)[2].1, 0xE359E + 24, "4 * 406 mod 0x40");
    }

    #[test]
    fn frozen_never_moves() {
        assert_eq!(RingAdvance::FROZEN.offset(12345), 0);
    }
}

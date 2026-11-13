//! Context word arrays and arithmetic helpers shared by the firmware routine ports
//! (`fw_tone_stack`, `fw_allpass`, `fw_amp_drive`, `fw_delay`).
//!
//! A port keeps the routine's context as raw 32-bit words addressed by their offset from `I1`
//! (the firmware's own layout), so a context captured from the DSP image loads unchanged and the
//! written-back state can be compared word for word. Float words flush a zero exponent field to
//! zero like the DSP's input stage. Everything is allocation free and `Copy`.
//!
//! Provenance: the helpers are the arithmetic of `tools/fx_pe.py` (`VERIFIED` against the
//! `sharc-fx` oracle through the models of `docs/research/fx-routine-models.md`), evaluated in
//! f32 here instead of 40-bit registers with truncating stores.

/// `N` context words starting at offset `base` from `I1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Words<const N: usize> {
    words: [u32; N],
    base: i32,
}

/// Float32 word to value; a zero exponent field flushes to zero.
#[inline]
pub fn val(word: u32) -> f32 {
    if (word >> 23) & 0xFF == 0 {
        0.0
    } else {
        f32::from_bits(word)
    }
}

/// `Fn = CLIP Fx BY Fy`: `v` limited to `+-|bound|`.
#[inline]
pub fn clip(v: f32, bound: f32) -> f32 {
    let b = bound.abs();
    if v.abs() <= b {
        v
    } else {
        b.copysign(v)
    }
}

/// One-pole step toward `target`: `target + k (state - target)`.
#[inline]
pub fn smooth(target: f32, state: f32, k: f32) -> f32 {
    target + k * (state - target)
}

/// `Rn = FIX Fx BY scale`: round to nearest even; out of range gives all ones.
#[inline]
pub fn fix_by(x: f32, scale: i32) -> u32 {
    if x == 0.0 {
        return 0;
    }
    let r = (f64::from(x) * 2f64.powi(scale)).round_ties_even();
    if !(-2147483648.0..2147483648.0).contains(&r) {
        return 0xFFFF_FFFF;
    }
    (r as i64) as u32
}

impl<const N: usize> Words<N> {
    /// All-zero context whose first word is at offset `base`.
    pub const fn new(base: i32) -> Self {
        Self {
            words: [0; N],
            base,
        }
    }

    /// A context from raw words (`words[0]` is at offset `base`).
    pub const fn from_words(base: i32, words: [u32; N]) -> Self {
        Self { words, base }
    }

    /// The raw words, including the written-back state.
    pub const fn words(&self) -> &[u32; N] {
        &self.words
    }

    #[inline]
    fn index(&self, offset: i32) -> usize {
        usize::try_from(offset - self.base)
            .ok()
            .filter(|i| *i < N)
            .expect("context offset outside the routine's words")
    }

    /// The raw word at `offset`.
    #[inline]
    pub fn word(&self, offset: i32) -> u32 {
        self.words[self.index(offset)]
    }

    /// Sets the raw word at `offset`.
    #[inline]
    pub fn set_word(&mut self, offset: i32, word: u32) {
        let i = self.index(offset);
        self.words[i] = word;
    }

    /// The float word at `offset`.
    #[inline]
    pub fn f(&self, offset: i32) -> f32 {
        val(self.word(offset))
    }

    /// Sets the float word at `offset`.
    #[inline]
    pub fn set(&mut self, offset: i32, value: f32) {
        self.set_word(offset, value.to_bits());
    }

    /// Sets both words of a (left, right) pair field to `value`.
    #[inline]
    pub fn set_pair(&mut self, offset: i32, value: f32) {
        self.set(offset, value);
        self.set(offset + 1, value);
    }

    /// The signed integer word at `offset`.
    #[inline]
    pub fn int(&self, offset: i32) -> i32 {
        self.word(offset) as i32
    }
}

/// Whether two float32 words agree to a relative `tol` (floor `1e-3` for values near zero) -- the
/// check of the golden tests, where f32 arithmetic meets 40-bit registers with truncating stores.
#[cfg(test)]
pub(super) fn close(got: u32, want: u32, tol: f32) -> bool {
    if got == want {
        return true; // integer words (and NaN patterns) must match exactly
    }
    let (a, b) = (val(got), val(want));
    (a - b).abs() <= tol * a.abs().max(b.abs()).max(1.0e-3)
}

/// Asserts that every word of `got` is [`close`] to `want`; `what` names the vector in a failure.
#[cfg(test)]
pub(super) fn assert_close(got: &[u32], want: &[u32], tol: f32, what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: length");
    for (k, (&a, &b)) in got.iter().zip(want).enumerate() {
        assert!(
            close(a, b, tol),
            "{what}[{k}]: got {a:#010x} ({}), want {b:#010x} ({})",
            val(a),
            val(b)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_addressed_by_offset_from_i1() {
        let mut w = Words::<4>::new(-2);
        w.set(-2, 1.5);
        w.set_pair(-1, 2.0);
        w.set_word(1, 0xFFFF_FFFE);
        assert_eq!(w.f(-2), 1.5);
        assert_eq!((w.f(-1), w.f(0)), (2.0, 2.0));
        assert_eq!(w.int(1), -2);
        assert_eq!(w.words()[0], 1.5f32.to_bits());
        assert_eq!(Words::from_words(-2, *w.words()), w);
    }

    #[test]
    #[should_panic(expected = "outside")]
    fn an_offset_outside_the_words_is_a_programming_error() {
        let _ = Words::<4>::new(-2).word(2);
    }

    #[test]
    fn float_helpers_follow_the_dsp_semantics() {
        assert_eq!(val(0x0040_0000), 0.0, "zero exponent flushes");
        assert_eq!(val(0x3F80_0000), 1.0);
        assert_eq!(clip(3.0, -2.0), 2.0);
        assert_eq!(clip(-3.0, 2.0), -2.0);
        assert_eq!(clip(0.5, 2.0), 0.5);
        assert_eq!(smooth(1.0, 0.0, 0.25), 0.75);
        assert_eq!(fix_by(1.5, 0), 2, "ties to even");
        assert_eq!(fix_by(2.5, 0), 2);
        assert_eq!(fix_by(-1.5, 0), 0xFFFF_FFFE);
        assert_eq!(fix_by(0.25, 30), 1 << 28);
        assert_eq!(
            fix_by(1.0e30, 0),
            0xFFFF_FFFF,
            "out of range gives all ones"
        );
        assert_eq!(fix_by(0.0, 8), 0);
    }

    #[test]
    fn close_uses_a_relative_tolerance_with_a_floor() {
        let w = |v: f32| v.to_bits();
        assert!(close(w(1.0), w(1.0 + 1.0e-6), 1.0e-5));
        assert!(!close(w(1.0), w(1.001), 1.0e-5));
        assert!(
            close(w(1.0e-6), w(1.5e-6), 1.0e-3),
            "values below the floor agree"
        );
        assert_close(&[w(1.0), w(2.0)], &[w(1.0), w(2.0)], 1.0e-6, "same");
    }

    #[test]
    #[should_panic(expected = "v[1]")]
    fn assert_close_names_the_failing_word() {
        let w = |v: f32| v.to_bits();
        assert_close(&[w(1.0), w(2.0)], &[w(1.0), w(3.0)], 1.0e-3, "v");
    }
}

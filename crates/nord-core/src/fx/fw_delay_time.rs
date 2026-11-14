//! Firmware delay time law: `kDlyRateClkDiv` (14 bit UI value) to delay samples
//! (`docs/research/fx-delay-time.md`, `tools/fx_delay_time.py`).
//!
//! Two verified halves:
//!
//! * ARM: the handler (`0xC010C514`) turns the value `v` into a fractional index `k` by a bracket
//!   search in [`FW_DELAY_TABLE`] and a linear interpolation in `f64`; the delay unit writes
//!   `c[0x15] = min(61438, floor(K * k))` (a float holding an integer; samples at 48 kHz).
//! * DSP: routine `0x120AB5` reads `c[0x15]` (and `c[0x16]`, the previous time) as ring buffer
//!   tap offsets; an input echoes `trunc(c[0x15]) + 2` samples later (oracle, 7 times in
//!   `tools/fx_delay_time.py oracle`).
//!
//! Only the free-running time path is covered; the tempo sync path (`kDlyClkEnable`) goes through
//! the master clock object and is not decoded.

use super::fw_delay_table::{FW_DELAY_TABLE, FW_DELAY_TABLE_LEN};

/// Delay samples per table index. VERIFIED: `floor(K * k)` reproduces all 264 captured words of
/// `docs/research/data/fx-delay-time.csv` for `K` in `[43.402338, 43.402340]` (the rest of the
/// tight interval is the float32 rounding of the firmware constant); the exact firmware literal
/// is not decoded (the double arithmetic of the delay unit runs through soft-float calls).
pub const SAMPLES_PER_INDEX: f64 = 43.402_339;
/// Longest time word. VERIFIED: every `v <= 150` captured `61438` (1.28 s at 48 kHz).
pub const MAX_SAMPLES: f64 = 61_438.0;
/// Samples from an input to its first echo beyond the tap word. VERIFIED in the `sharc-fx` oracle
/// (impulse at tap words 2 to 2000, always `trunc(c[0x15]) + 2`).
pub const ECHO_LATENCY: f32 = 2.0;
/// Sample rate the time words are counted at (the DSP runs at 48 kHz).
pub const FW_SAMPLE_RATE: f32 = 48_000.0;
/// Highest 14-bit UI value.
pub const MAX_RATE: u16 = 16_383;

/// First UI value of the top plateau. VERIFIED: handler branch `0xC010C914` for `v >= 0x3F80`.
const V_TOP: u32 = 16_256;
/// Index at the top plateau (double literal `20.0` of the branch) and at `v <= 0` (`1500.0`).
const K_TOP: f64 = 20.0;
const K_BOTTOM: f64 = 1500.0;
/// Coarse search stride of the handler (`0x5C` bytes = 46 entries) and its floor entry.
const COARSE: usize = 46;
const COARSE_FLOOR: usize = 28;

/// Fractional table index `k` of the 14-bit UI value `v` (`0xC010C514`, `f64` arithmetic).
///
/// VERIFIED word for word against the ARM harness for 264 values (`tests` below).
pub fn delay_index(v: u16) -> f64 {
    let v = i32::from(v);
    if v >= V_TOP as i32 {
        return K_TOP;
    }
    if v <= 0 {
        return K_BOTTOM;
    }
    let t = &FW_DELAY_TABLE;
    let mut i = FW_DELAY_TABLE_LEN - 1;
    while i != COARSE_FLOOR && v > i32::from(t[i - COARSE]) {
        i -= COARSE;
    }
    while v > i32::from(t[i]) {
        i -= 1;
    }
    if v == i32::from(t[i]) {
        return i as f64;
    }
    i as f64 + f64::from(i32::from(t[i]) - v) / f64::from(i32::from(t[i]) - i32::from(t[i + 1]))
}

/// The tap word `c[0x15]` for `kDlyRateClkDiv = v`: delay samples at 48 kHz, an integer value.
pub fn delay_word(v: u16) -> f32 {
    (SAMPLES_PER_INDEX * delay_index(v))
        .floor()
        .min(MAX_SAMPLES) as f32
}

/// Samples from an input to its first echo at 48 kHz: `trunc(c[0x15]) + 2`.
pub fn echo_samples(v: u16) -> f32 {
    delay_word(v) + ECHO_LATENCY
}

/// [`echo_samples`] scaled to another engine sample rate (the law is a time, `samples / 48 kHz`).
pub fn echo_samples_at(v: u16, sample_rate: f32) -> f32 {
    echo_samples(v) * (sample_rate / FW_SAMPLE_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(v, c[0x15])` captured through the ARM harness (`docs/research/data/fx-delay-time.csv`).
    const SERIES: [(u16, u32); 34] = [
        (0, 61438),
        (1, 61438),
        (127, 61438),
        (128, 61438),
        (170, 61088),
        (175, 60958),
        (180, 60850),
        (199, 60437),
        (256, 59179),
        (640, 52082),
        (1024, 46498),
        (2048, 32551),
        (3072, 27126),
        (4096, 23460),
        (5120, 21883),
        (6144, 20505),
        (7168, 19290),
        (8192, 17836),
        (9088, 16275),
        (9216, 15879),
        (10240, 13287),
        (11264, 9865),
        (11520, 8574),
        (12288, 6510),
        (13312, 4718),
        (13952, 3255),
        (14336, 2505),
        (15360, 1480),
        (15616, 1302),
        (15744, 1206),
        (16000, 1048),
        (16128, 957),
        (16256, 868),
        (16383, 868),
    ]; // fmt: skip

    #[test]
    fn law_reproduces_the_captured_words() {
        for (v, word) in SERIES {
            assert_eq!(delay_word(v), word as f32, "v = {v}");
        }
    }

    #[test]
    fn index_hits_exact_entries_plateaus_and_fractions() {
        assert_eq!(delay_index(0), 1500.0);
        assert_eq!(delay_index(16_383), 20.0);
        assert_eq!(delay_index(16_256), 20.0);
        // an entry that differs from both neighbours maps back to its own index
        let k = (100..1400)
            .find(|&k| {
                FW_DELAY_TABLE[k] < FW_DELAY_TABLE[k - 1]
                    && FW_DELAY_TABLE[k] > FW_DELAY_TABLE[k + 1]
            })
            .unwrap();
        assert_eq!(delay_index(FW_DELAY_TABLE[k] as u16), k as f64);
        // halfway between two entries is a half index (entries 1499 = 2 and 1500 = 0)
        assert_eq!(delay_index(1), 1499.5);
    }

    #[test]
    fn longer_values_are_shorter_delays_and_the_range_is_18_to_1280_ms() {
        let mut prev = f32::MAX;
        for v in (0..=MAX_RATE).step_by(7) {
            let w = delay_word(v);
            assert!(w <= prev, "monotone at {v}");
            prev = w;
        }
        assert_eq!(delay_word(0), 61_438.0);
        assert_eq!(delay_word(MAX_RATE), 868.0);
        let ms = |v| echo_samples(v) / FW_SAMPLE_RATE * 1000.0;
        assert!((ms(0) - 1280.0).abs() < 0.1 && (ms(MAX_RATE) - 18.1).abs() < 0.1);
    }

    #[test]
    fn echo_adds_the_two_sample_latency_and_scales_with_the_sample_rate() {
        assert_eq!(echo_samples(1024), 46_500.0);
        assert_eq!(echo_samples_at(1024, 96_000.0), 93_000.0);
        assert_eq!(echo_samples_at(1024, 48_000.0), echo_samples(1024));
    }
}

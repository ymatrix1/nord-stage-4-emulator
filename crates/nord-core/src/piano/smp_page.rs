//! One `.npno` page as the SMP voice loop consumes it, and a driver that plays it through
//! [`SmpVoice`] models the way the ARM would set the records up.
//!
//! Spec: `docs/research/smp-voice.md`. The file page needs no re-layout: its payload words are the
//! LSB-first 16-bit units the loop reads (VERIFIED on the oracle, bit-exact), one per DM word.

use super::q31_kernel::SmpKernel;
use super::smp_unpack::FIXED_LPC_COEFFS;
use super::smp_voice::{Call, SmpVoice, RING_WORDS};

/// Bytes of one mono `.npno` page (2-byte header, 1020 payload bytes); a stereo page is twice that.
pub const NPNO_PAGE_BYTES: usize = 1022;

/// One `.npno` page (1022 bytes mono, 2044 stereo): header byte `b1` (order and width) and the
/// payload as 16-bit words. A stereo payload interleaves the residuals `L0 R0 L1 R1 ...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpnoPage {
    pub channels: usize,
    pub order: u8,
    pub nbits: u32,
    /// Payload words, big-endian in the file, each kept in the low 16 bits of a DM word.
    pub words: Vec<u32>,
}

impl NpnoPage {
    /// Parses a mono (1022 bytes) or stereo (2044 bytes) page. `None` for a wrong length or an
    /// invalid header byte.
    pub fn parse(page: &[u8]) -> Option<Self> {
        let channels = match page.len() {
            NPNO_PAGE_BYTES => 1,
            n if n == 2 * NPNO_PAGE_BYTES => 2,
            _ => return None,
        };
        let (order, nbits) = (page[1] >> 5, u32::from(page[1] & 0x0F));
        if order > 4 || nbits == 0 {
            return None;
        }
        let words = page[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u32::from(u16::from_be_bytes(*c)))
            .collect();
        Some(Self {
            channels,
            order,
            nbits,
            words,
        })
    }

    /// Residuals per channel the page holds (`floor(8160 / nbits)` mono).
    pub fn frames(&self) -> usize {
        self.words.len() * 16 / (self.nbits as usize * self.channels)
    }
}

/// Deterministic test page (no firmware data): small LCG residuals of `nbits` bits, packed as a
/// file page (LSB-first in 16-bit words, stereo interleaved `L0 R0 L1 R1 ...`).
pub fn synthetic_page(order: u8, nbits: u32, channels: usize) -> NpnoPage {
    use super::smp_unpack::pack_units;
    let words_n = if channels == 1 { 510 } else { 1021 };
    let n = words_n * 16 / nbits as usize;
    let amp = 1i32 << (nbits - 2).min(3);
    let mut seed = 0x1234_5678u32;
    let res: Vec<i32> = (0..n)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 16) as i32).rem_euclid(2 * amp) - amp
        })
        .collect();
    let mut words = pack_units(&res, nbits);
    words.resize(words_n, 0);
    NpnoPage {
        channels,
        order,
        nbits,
        words,
    }
}

/// FNV-1a over the little-endian bytes of `words` (compact fingerprint of a bus capture).
pub fn fnv64(words: &[i32]) -> u64 {
    words
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .fold(0xCBF2_9CE4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3)
        })
}

/// Record set-up of a page player (the values the ARM writes into the records).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayConfig {
    /// Source samples per output frame (`<= 1`: the loop decodes one sample per call).
    pub step: f64,
    /// Q31 gain of each voice on its own bus side.
    pub gain: i32,
    /// Record word `-24` and initial phase (`-29`).
    pub hist_off: u32,
    pub phase0: u32,
    /// Starts the read `K` stream words in (`base = -16 K`).
    pub base_words: u32,
}

impl Default for PlayConfig {
    fn default() -> Self {
        Self {
            step: 0.5,
            gain: i32::MAX,
            hist_off: 0,
            phase0: 0,
            base_words: 0,
        }
    }
}

/// Voices of a page: stride `channels * width`, right channel base `-width`; a mono page feeds
/// both bus sides, a stereo page one voice per side. `h0` holds the initial LPC history per channel.
pub fn page_voices(page: &NpnoPage, h0: &[[i32; 4]], c: &PlayConfig) -> Vec<SmpVoice> {
    let coefs = FIXED_LPC_COEFFS[usize::from(page.order)];
    let q = (c.step * 4_294_967_296.0).round() as u64;
    (0..page.channels)
        .zip(h0)
        .map(|(v, h)| {
            let g = match (page.channels, v) {
                (1, _) => [c.gain, c.gain],
                (_, 0) => [c.gain, 0],
                _ => [0, c.gain],
            };
            let mut s = SmpVoice::new(page.nbits, coefs, g);
            s.stride = page.nbits * page.channels as u32;
            s.base = (v as u32 * page.nbits)
                .wrapping_neg()
                .wrapping_sub(16 * c.base_words);
            (s.hist_off, s.phase) = (c.hist_off, c.phase0);
            (s.step_int, s.step_frac) = ((q >> 32) as u32, q as u32);
            for (w, slot) in page.words.iter().zip(s.ring.iter_mut()) {
                *slot = *w;
            }
            s.preload_history(*h);
            s
        })
        .collect()
}

/// One played frame: state after the calls of all voices.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub calls: Vec<Call>,
    pub bus: [i32; 2],
    /// Counter and phase of voice 0 after the call.
    pub counter: u32,
    pub phase: u32,
    /// Record word `+4` and the history rings of all voices.
    pub last: Vec<i32>,
    pub hist: Vec<i32>,
    /// Ring words stored before this frame (the DMA refill): `(slot, word)`.
    pub refill: Vec<(usize, u32)>,
}

/// Plays `frames` output frames. The ring starts with the first `RING_WORDS` stream words and is
/// refilled just ahead of the read position, as the external-port DMA does.
pub fn play_page(page: &NpnoPage, h0: &[[i32; 4]], c: &PlayConfig, frames: usize) -> Vec<Frame> {
    let mut vs = page_voices(page, h0, c);
    let k = SmpKernel::shared();
    let mut next_word = RING_WORDS;
    (0..frames)
        .map(|_| {
            let need = vs
                .iter()
                .map(|v| (v.bit_position() >> 4) as usize + 1)
                .max()
                .unwrap_or(0);
            let mut refill = Vec::new();
            while next_word <= need.min(page.words.len() - 1) {
                for v in &mut vs {
                    v.ring[next_word % RING_WORDS] = page.words[next_word];
                }
                refill.push((next_word % RING_WORDS, page.words[next_word]));
                next_word += 1;
            }
            let mut bus = [0; 2];
            let calls = vs.iter_mut().map(|v| v.call(k, &mut bus)).collect();
            Frame {
                calls,
                bus,
                counter: vs[0].counter,
                phase: vs[0].phase,
                last: vs.iter().map(|v| v.last).collect(),
                hist: vs.iter().flat_map(|v| v.hist).collect(),
                refill,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piano::smp_voice::HIST_WORDS;

    #[test]
    fn test_synthetic_page_shape_and_fnv() {
        for (ch, words) in [(1, 510), (2, 1021)] {
            let p = synthetic_page(1, 6, ch);
            assert_eq!((p.words.len(), p.channels), (words, ch));
            assert!(p.words.iter().all(|&w| w <= 0xFFFF));
            assert_eq!(p, synthetic_page(1, 6, ch), "deterministic");
        }
        assert_ne!(fnv64(&[1, 2]), fnv64(&[2, 1]));
        assert_eq!(fnv64(&[]), 0xCBF2_9CE4_8422_2325);
    }

    #[test]
    fn test_npno_page_parse() {
        let mut page = vec![0u8; NPNO_PAGE_BYTES];
        page[1] = (2 << 5) | 9;
        page[2..4].copy_from_slice(&[0x12, 0x34]);
        let p = NpnoPage::parse(&page).unwrap();
        assert_eq!(
            (p.order, p.nbits, p.words[0], p.words.len()),
            (2, 9, 0x1234, 510)
        );
        assert_eq!(p.frames(), 510 * 16 / 9);
        page[1] = 5 << 5 | 9;
        assert!(NpnoPage::parse(&page).is_none());
        page[1] = 2 << 5;
        assert!(NpnoPage::parse(&page).is_none());
        assert!(NpnoPage::parse(&page[..10]).is_none());
        let mut st = vec![0u8; 2 * NPNO_PAGE_BYTES];
        st[1] = 4 << 5 | 8;
        let p = NpnoPage::parse(&st).unwrap();
        assert_eq!(
            (p.channels, p.words.len(), p.frames()),
            (2, 1021, 1021 * 16 / 16)
        );
    }

    /// Bus words of a played synthetic page (stereo interleaved L R per frame).
    fn bus(page: &NpnoPage, step: f64, frames: usize) -> Vec<i32> {
        let c = PlayConfig {
            step,
            ..PlayConfig::default()
        };
        let h0 = vec![[0; 4]; page.channels];
        play_page(page, &h0, &c, frames)
            .iter()
            .flat_map(|f| f.bus)
            .collect()
    }

    /// Golden values from the SMP image on the oracle (`sharc-ref`, `smp_voice_ref synthetic:..`,
    /// `docs/research/smp-voice.md` section 5): the fingerprints are those of the DSP's own bus
    /// capture, so this test fails if the Rust model leaves the firmware's arithmetic.
    #[test]
    fn test_synthetic_pages_reproduce_the_oracle_bus_capture() {
        let b = bus(&synthetic_page(1, 6, 2), 0.6, 400);
        assert_eq!(
            (b.len(), b[9], b[12], b[13]),
            (800, 131_071, -393_216, -655_360)
        );
        assert_eq!(fnv64(&b), 0xF073_FC2A_5525_CA67);
        let b = bus(&synthetic_page(3, 5, 1), 0.8749, 500);
        assert_eq!(
            (b.len(), b[8], b[9], b[10]),
            (1000, -131_072, -131_072, -1_310_720)
        );
        assert_eq!(fnv64(&b), 0x28F2_0169_4929_4371);
    }

    #[test]
    fn test_refill_keeps_the_ring_ahead_of_the_read_position_across_wraps() {
        let page = synthetic_page(1, 6, 2);
        let c = PlayConfig {
            step: 1.0,
            ..PlayConfig::default()
        };
        let frames = play_page(&page, &[[0; 4]; 2], &c, 1000);
        let refilled: usize = frames.iter().map(|f| f.refill.len()).sum();
        // one store per stream word beyond the 256 preloaded ones, up to the last word needed
        let last_needed = (999 * 12 + 6) / 16 + 1;
        assert_eq!(refilled, last_needed - (RING_WORDS - 1));
        assert!(frames[0].refill.is_empty());
        assert_eq!(frames[999].counter, 1000);
        assert_eq!(frames[0].hist.len(), 2 * HIST_WORDS);
    }

    #[test]
    fn test_page_voices_set_stride_base_and_gain_per_channel() {
        let c = PlayConfig {
            hist_off: 3,
            base_words: 2,
            ..PlayConfig::default()
        };
        let v = page_voices(&synthetic_page(2, 7, 2), &[[1, 2, 3, 4]; 2], &c);
        assert_eq!((v[0].stride, v[1].stride, v[0].width), (14, 14, 7));
        assert_eq!(v[0].base, 0u32.wrapping_sub(32));
        assert_eq!(v[1].base, 0u32.wrapping_sub(7 + 32));
        assert_eq!((v[0].gain, v[1].gain), ([i32::MAX, 0], [0, i32::MAX]));
        let m = page_voices(&synthetic_page(2, 7, 1), &[[0; 4]], &PlayConfig::default());
        assert_eq!(m[0].gain, [i32::MAX; 2]);
    }
}

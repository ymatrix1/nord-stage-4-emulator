//! Words of the Amp type switch for the drive stage `0x1228CF` (organ unit L3, FX core 1) and the
//! static words of the stage's context.
//!
//! Source: `Edt_Effects_kAmpTypeNew` 1..=6 applied to the settled firmware (`tools/fx_clients.py`),
//! every word the type switch sends into `DM 0xC016A..0xC019F`; committed as
//! `docs/research/data/fx-amp-types.csv` (`tools/fx_amp_types.py extract`). **VERIFIED** as words;
//! `tests_amp_golden.rs` runs the stage with them against the `sharc-fx` oracle. The type value is
//! the amp mode of the program (`amp_type_new & 15`: 1 Twin, 2 Twin II, 3 JC, 4 JC II, 5 Small,
//! 6 Small II; 0 and 7..=9 send no drive stage words). The **cabinet** words of the same switch
//! (130 FIR taps and a 12 set biquad bank of module `0xC0442A94`) are not used here.

use super::fw_amp_drive::{ctx, CTX_WORDS};
use super::AmpCabinet;

/// Words of one amp type: `G1` scale `ctx[-0xD]`, `G2` slope `ctx[-0xC]`, `G2` offset `ctx[-0xB]`,
/// first section `ctx[7..=0xB]`, second section `ctx[0x10..=0x14]` (`b0 b1 b2 a1 a2` each).
pub(super) const TYPE_WORDS: [[u32; 13]; 6] = [
    // type 1: Twin
    [
        0x43000000, 0x41800000, 0x41200000, 0x4033d9b3, 0xc098543d, 0x40042d9e, 0xbfbba49a,
        0x3f14c491, 0x4030d78f, 0xc0aaf8ee, 0x402768a7, 0xbfaaf8ee, 0x3eb0806c,
    ],
    // type 2: Twin II
    [
        0x43000000, 0x41800000, 0x41200000, 0x3f800000, 0xbffc11ca, 0x3f784202, 0xbffc11ca,
        0x3f784202, 0x404031c8, 0xc0bca334, 0x403ab4a5, 0xbfd795f2, 0x3f310758,
    ],
    // type 3: JC
    [
        0x43000000, 0x41800000, 0x41200000, 0x4132e6c1, 0xc19d7a98, 0x410b3f4d, 0xbf900ce5,
        0x3eb1a779, 0x3f349e72, 0xbfb31c19, 0x3f319ca9, 0xbfffdf00, 0x3f7fc228,
    ],
    // type 4: JC II
    [
        0x43000000, 0xc0400000, 0x41200000, 0x3f7e4439, 0xbffc7195, 0x3f7ac870, 0xbffc7195,
        0x3f790ca9, 0x40517486, 0xc0cdc4e0, 0x404b29d1, 0xbfeb2a24, 0x3f579063,
    ],
    // type 5: Small
    [
        0x43000000, 0x41800000, 0x41200000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
    ],
    // type 6: Small II
    [
        0x43000000, 0x41800000, 0x41200000, 0x3fd0c965, 0xc050c965, 0x3fd0c965, 0xbff56ce6,
        0x3f6bacc3, 0x3d700b71, 0x3df00b71, 0x3d700b71, 0xbf9a0cba, 0x3ee038a1,
    ],
];

/// Static words of the stage's context that the type switch does not send (the image's DM
/// initialisation, read through `sharc-fx` after the boot): `(offset from I1, word)`. VERIFIED
/// (same dump as the tables). Roles: high-pass `p`, `q` (a DC blocker, `0.99822`, `-0.99644`), the
/// `FIX` scale 31 (Q31), the bound of the dry/wet mix and of the shaper input (`1.0`, `0.99999994`).
const STATIC_WORDS: [(i32, u32); 6] = [
    (ctx::HP[0], 0x3f7f_8b58),
    (ctx::HP[1], 0xbf7f_16b1),
    (ctx::FIX_SCALE, 31),
    (ctx::MIX_BOUND, 0x3f80_0000),
    (ctx::SHAPER_BOUND, 0x3f7f_fffe),
    (0x15, 2),
];

/// Amp type (`kAmpTypeNew`) of a cabinet and its variation flag; `None` where the type switch
/// installs no drive stage words (off, To Rotary, LP24, HP24).
pub(super) fn type_for(cabinet: AmpCabinet, variation: bool) -> Option<u8> {
    match (cabinet, variation) {
        (AmpCabinet::Twin, v) => Some(1 + u8::from(v)),
        (AmpCabinet::Jc, v) => Some(3 + u8::from(v)),
        (AmpCabinet::Small, v) => Some(5 + u8::from(v)),
        _ => None,
    }
}

/// The 54 context words of the drive stage for amp `kind` (`1..=6`) with the drive target and its
/// smoothed state at `drive_ui / 127`. `None` for other types.
pub(super) fn context_words(kind: u8, drive_ui: f32) -> Option<[u32; CTX_WORDS]> {
    let w = TYPE_WORDS.get(usize::from(kind).checked_sub(1)?)?;
    let at = |off: i32| (off + 0x20) as usize;
    let mut words = [0u32; CTX_WORDS];
    for (off, word) in STATIC_WORDS {
        words[at(off)] = word;
    }
    let target = (drive_ui / 127.0).to_bits();
    words[at(ctx::PHASE_TARGET)] = target;
    words[at(ctx::PHASE_TARGET + 1)] = target;
    words[at(ctx::G1_SCALE)] = w[0];
    words[at(ctx::G2_SLOPE)] = w[1];
    words[at(ctx::G2_OFFSET)] = w[2];
    for (k, (&o1, &o2)) in ctx::SECTION1.iter().zip(&ctx::SECTION2).enumerate() {
        words[at(o1)] = w[3 + k];
        words[at(o2)] = w[8 + k];
    }
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_words_are_the_captured_sections() {
        let twin = context_words(1, 0.0).unwrap();
        let at = |off: i32| f32::from_bits(twin[(off + 0x20) as usize]);
        assert!((at(7) - 2.810_162).abs() < 1e-6 && (at(0xB) - 0.581_124).abs() < 1e-6);
        assert!((at(0x10) - 2.763_157).abs() < 1e-6 && (at(0x14) - 0.344_73).abs() < 1e-5);
        assert_eq!((at(-0xD), at(-0xC), at(-0xB)), (128.0, 16.0, 10.0));
        // JC II is the only type with the negative G2 slope; Small is the identity pair.
        assert_eq!(f32::from_bits(TYPE_WORDS[3][1]), -3.0);
        assert_eq!(f32::from_bits(TYPE_WORDS[4][3]), 1.0);
        assert_eq!(f32::from_bits(TYPE_WORDS[4][8]), 1.0);
    }

    #[test]
    fn modes_map_to_types_and_the_rest_has_no_drive_stage() {
        assert_eq!(type_for(AmpCabinet::Twin, false), Some(1));
        assert_eq!(type_for(AmpCabinet::Twin, true), Some(2));
        assert_eq!(type_for(AmpCabinet::Jc, true), Some(4));
        assert_eq!(type_for(AmpCabinet::Small, true), Some(6));
        assert_eq!(type_for(AmpCabinet::ToRotary, false), None);
        assert_eq!(type_for(AmpCabinet::Off, false), None);
        assert!(context_words(0, 0.0).is_none() && context_words(7, 0.0).is_none());
    }

    #[test]
    fn the_drive_value_sets_target_and_state() {
        let w = context_words(1, 127.0).unwrap();
        assert_eq!(f32::from_bits(w[0]), 1.0);
        assert_eq!(f32::from_bits(w[1]), 1.0);
    }
}

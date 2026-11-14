//! Start state and sine-bank data of the Vox post stage, derived from the PNO image.
//!
//! Regenerate with `python tools/organ_vox_image.py rust SRAM/core1_sram.bin --exe ...`;
//! method and checks in `docs/research/organ-vox-start-state.md`. 84 start words, three
//! sine phase groups, the 36 ladder words and the polynomial constants: derived values,
//! no code or sample data.

/// VERIFIED: LFO start phase `DM(0xE21F0)` of the image (both lanes), `|s| = 1`: the triangle
/// starts at its top. No ARM message and no DSP instruction other than the per frame
/// `+= 0x905A3` writes it (`tools/organ_vox_image.py check`).
pub(super) const LFO_START: u32 = 0x80000000;

/// VERIFIED: note phases `DM(0xE2B14 + k)` of the image (the state the oracle chord starts
/// from); the oscillators free-run and nothing resets them at key-down, so on hardware the
/// relative phases of a chord are those of the free-running bank at that instant.
#[rustfmt::skip]
pub(super) const NOTE_START: [u32; 84] = [
    0xBF44DC4C, 0xBF25F5F0, 0xBF38F493, 0xBF150BD5, 0xBEF1E0FC, 0xBF154068,
    0xBF03D6E9, 0xBF01429B, 0xBEC54AB2, 0xBE9C460B, 0xBE8D85D3, 0xBEA1914A,
    0xBE7F567D, 0xBE890277, 0xBE5B3E60, 0xBE64DFF3, 0xBE373B4A, 0xBDCBACCE,
    0xBDA6AA5D, 0xBDD94F79, 0xBDDA21A4, 0xBDA32124, 0xBD2D93C3, 0xBD2CB1A1,
    0xBCF44ADF, 0xBC7DD6E1, 0xBC9FD1FA, 0xBC8F1599, 0xBC458170, 0xBBBC5A3F,
    0xBC03A81B, 0xBAF56FA6, 0xBACA4376, 0xBA64A622, 0xBB1DA8AB, 0xBA42F251,
    0xBAA3B972, 0xB938C458, 0xB928F92E, 0xB8E380E6, 0xB8620DBB, 0xB79DB520,
    0xB68EF5EA, 0xB77D9642, 0xB5E4132C, 0xB6801286, 0xB43DEE5C, 0xB46F73AF,
    0xB4778AFB, 0xB451288B, 0xB3F6B3FB, 0xB36221D8, 0xB28CE02C, 0xB16FBD72,
    0xB002ECE3, 0xAE3DFD37, 0xAC17B299, 0xA986267F, 0xABFE41FD, 0xA8C8766C,
    0xA50916CF, 0xA73CDE69, 0xA2A477FD, 0xA4B3CF08, 0x9F1B9DA5, 0xA0F528F7,
    0x9A30241F, 0x9BBECB21, 0x9D6521BB, 0x94C3133F, 0x95FF20FF, 0x974DFA87,
    0x8C585CF0, 0x8D144A23, 0x8DDB6332, 0x80000000, 0x80000000, 0x80000000,
    0x80000000, 0x80000000, 0x6C673ED8, 0x6B3CECC1, 0x6A00DF01, 0x68B20579,
];

/// VERIFIED: sine phase steps `DM(0xE2C8E + j)` (fraction of 2^31); `0x1AE89F99 2^(j/12)` for
/// j < 24 (max 1 count), folded down by 0.75 (j 24..31) and 0.63 (j 32..35) after that.
#[rustfmt::skip]
pub(super) const SINE_LADDER: [u32; 36] = [
    0x1AE89F99, 0x1C823E07, 0x1E3437E7, 0x20000000, 0x21E71F25, 0x23EB3587,
    0x260DFC14, 0x285145F3, 0x2AB70211, 0x2D413CCC, 0x2FF221AE, 0x32CBFD4A,
    0x35D13F32, 0x39047C0E, 0x3C686FCE, 0x40000000, 0x43CE3E4B, 0x47D66B0F,
    0x4C1BF828, 0x50A28BE6, 0x556E0423, 0x5A827999, 0x5FE4435D, 0x6597FA94,
    0x50B9DECC, 0x5586BA15, 0x5A9CA7B5, 0x60000000, 0x65B55D71, 0x6BC1A096,
    0x7229F43D, 0x78F3D1D9, 0x6BA27E65, 0x7208F81D, 0x78D0DF9C, 0x7FFFFFFF,
];

/// VERIFIED: sine start phases `DM(0xE2B98 + j)` per octave group (j 0..23, 24..31, 32..35).
pub(super) const SINE_START: [u32; 3] = [0xFD70A3D8, 0x8B851EB9, 0x970A3D71];

/// VERIFIED: sine gains `DM(0xE2BBC + j)` as f32 words per group (j 0..11, 12..23, 24..31, 32..35).
pub(super) const SINE_GAIN_WORDS: [u32; 4] = [0x3F800000, 0x3E2FE9F0, 0x3E29F113, 0x3DAC5155];

/// VERIFIED: sine base word `DM(0xE2BE0)` (fraction of 2^31, 0.917).
pub(super) const SINE_BASE: u32 = 0x7572967D;

/// VERIFIED: sine polynomial words `DM(0xE2CB8..)`: c7, c5, c3, c1 (`c1 = pi / 2`).
pub(super) const SINE_POLY_WORDS: [u32; 4] = [0xBB8DEBE6, 0x3DA2ACAA, 0xBF25591E, 0x3FC90FAC];

/// VERIFIED: bucket of each sine, `DM(0x95E22 + 2 (j >> 1)) + (j & 1)`.
pub(super) const SINE_BUCKET: [u8; 36] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3, 4, 5,
    6, 7, 0, 1, 2, 3,
];

#[test]
fn start_state_is_the_image_state() {
    assert!(NOTE_START.iter().all(|&w| w != 0));
    assert_eq!(LFO_START, 0x8000_0000);
    assert_eq!(SINE_BUCKET.iter().filter(|&&b| b == 0).count(), 4);
}

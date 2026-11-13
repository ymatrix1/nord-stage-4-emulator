//! Golden vectors for the firmware bank: x words of the Python reference model
//! (`tools/organ_bank_ref.py`, class `ToneBank`) on a synthetic closed-form table.
//!
//! Regenerate with `python tools/organ_bank_ref.py golden`. The vectors contain no firmware data
//! (the table is the closed form of `docs/research/organ-tonewheel-bank.md` section 1). The test
//! drives the Rust bank with the same increment words through `fracmul` at tune 0.5, odd and
//! negative words included, so the rounding and wrap rules are checked against the model that
//! reproduces the SHARC oracle word for word.

pub(super) const INCS: [u32; 6] = [
    0x02000000, 0x02468ACF, 0xE0000001, 0x0633E000, 0x00001235, 0x7FFFFFFF,
];
pub(super) const X_BITS: [[u32; 6]; 24] = [
    [
        0xBDE89314, 0xBDE229F2, 0xBEEEE172, 0xBD79EF9F, 0xBE0DA960, 0x3F8D8DE1,
    ],
    [
        0xBDB4E1D7, 0xBDA7D0E7, 0xBF2FD081, 0x3CB3857F, 0xBE0DA960, 0xBDCA197B,
    ],
    [
        0xBD8048D3, 0xBD58ABD4, 0xBF556543, 0x3DD9DBCC, 0xBE0DA960, 0xBF64E43E,
    ],
    [
        0xBD15A64A, 0xBCBF258B, 0xBF64E43E, 0x3E47821F, 0xBE0DA960, 0xBE0DA960,
    ],
    [
        0xBC2469B9, 0x3C09A4E4, 0xBF5B0266, 0x3E91A23B, 0xBE0DA960, 0x3F8D8DE1,
    ],
    [
        0x3C89ED34, 0x3D1F6E91, 0xBF36FE63, 0x3EBE8E9D, 0xBE0DA960, 0xBDCA197B,
    ],
    [
        0x3D346F19, 0x3D8F0D12, 0xBEEA3AE2, 0x3EEBB89B, 0xBE0DA960, 0xBF64E43E,
    ],
    [
        0x3D92978D, 0x3DCF1DEA, 0xBDCA197B, 0x3F0B5BD3, 0xBE0DA960, 0xBE0DA960,
    ],
    [
        0x3DCB8A47, 0x3E07E81B, 0x3E9DA349, 0x3F205D3E, 0xBE0DA960, 0x3F8D8DE1,
    ],
    [
        0x3E027EE0, 0x3E2A55FB, 0x3F330B83, 0x3F342C8D, 0xBE0DA960, 0xBDCA197B,
    ],
    [
        0x3E1F6F8D, 0x3E4B2A0A, 0x3F7E0457, 0x3F463833, 0xBE0DA960, 0xBF64E43E,
    ],
    [
        0x3E3C8D5D, 0x3E6C23D6, 0x3F8D8DE1, 0x3F571157, 0xBE0DA960, 0xBE0DA960,
    ],
    [
        0x3E59CE2D, 0x3E869A2E, 0x3F81D0BD, 0x3F66346B, 0xBE0DA960, 0x3F8D8DE1,
    ],
    [
        0x3E772792, 0x3E97262A, 0x3F3A3965, 0x3F7349C1, 0xBE0DA960, 0xBDCA197B,
    ],
    [
        0x3E8A4776, 0x3EA898F6, 0x3E98FCB9, 0x3F7EC9BD, 0xBE0DA960, 0xBF64E43E,
    ],
    [
        0x3E98FCB9, 0x3EB9143F, 0xBE0DA960, 0x3F8419EF, 0xBE0DA960, 0xBE0DA960,
    ],
    [
        0x3EA7AE20, 0x3EC97B8A, 0xBEEEE172, 0x3F87F167, 0xBE0DA960, 0x3F8D8DE1,
    ],
    [
        0x3EB65636, 0x3ED9C728, 0xBF2FD081, 0x3F8ACD29, 0xBE0DA960, 0xBDCA197B,
    ],
    [
        0x3EC4EF85, 0x3EE9EF83, 0xBF556543, 0x3F8CA37F, 0xBE0DA960, 0xBF64E43E,
    ],
    [
        0x3ED374A3, 0x3EFACF38, 0xBF64E43E, 0x3F8D852F, 0xBE0DA960, 0xBE0DA960,
    ],
    [
        0x3EE1E035, 0x3F054C03, 0xBF5B0266, 0x3F8D6406, 0xBE0DA960, 0x3F8D8DE1,
    ],
    [
        0x3EF02CF8, 0x3F0D13CB, 0xBF36FE63, 0x3F8C4751, 0xBE0DA960, 0xBDCA197B,
    ],
    [
        0x3EFE55C5, 0x3F14BB9D, 0xBEEA3AE2, 0x3F8A23E8, 0xBE0DA960, 0xBF64E43E,
    ],
    [
        0x3F062ACC, 0x3F1C4049, 0xBDCA197B, 0x3F871268, 0xBE0DA960, 0xBE0DA960,
    ],
];

#[cfg(test)]
mod tests {
    use super::super::firmware_bank::{fracmul, ToneBank};
    use super::super::types::NUM_TONEWHEELS;
    use super::*;

    #[test]
    fn rust_bank_matches_python_reference_model() {
        let mut bank = ToneBank::new();
        for (w, &inc) in INCS.iter().enumerate() {
            bank.set_step(0, w, fracmul(inc, 0x4000_0000));
        }
        let mut out = [0.0f32; NUM_TONEWHEELS];
        for (frame, row) in X_BITS.iter().enumerate() {
            bank.advance(&mut out);
            for (w, &bits) in row.iter().enumerate() {
                let want = f32::from_bits(bits);
                // Python builds the table in float64 numpy, Rust with std: allow one f32 ulp.
                assert!(
                    (out[w] - want).abs() <= 2.0 * f32::EPSILON * want.abs().max(1e-3),
                    "frame {frame} wheel {w}: {} vs {want}",
                    out[w]
                );
            }
        }
    }
}

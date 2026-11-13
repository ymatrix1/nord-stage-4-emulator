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
        0xBDE89255, 0xBDE22934, 0xBEEEE161, 0xBD79EE2F, 0xBE0DA900, 0x3F8D8DCE,
    ],
    [
        0xBDB4E11B, 0xBDA7D02C, 0xBF2FD07A, 0x3CB38812, 0xBE0DA900, 0xBDCA18BD,
    ],
    [
        0xBD80481A, 0xBD58AA66, 0xBF556538, 0x3DD9DC55, 0xBE0DA900, 0xBF64E465,
    ],
    [
        0xBD15A4E2, 0xBCBF22C8, 0xBF64E465, 0x3E478253, 0xBE0DA900, 0xBE0DA900,
    ],
    [
        0xBC24644C, 0x3C09AA2A, 0xBF5B029D, 0x3E91A24D, 0xBE0DA900, 0x3F8D8DCE,
    ],
    [
        0x3C89EFCE, 0x3D1F6FD0, 0xBF36FE5C, 0x3EBE8EA8, 0xBE0DA900, 0xBDCA18BD,
    ],
    [
        0x3D347055, 0x3D8F0DA7, 0xBEEA3A9B, 0x3EEBB8A1, 0xBE0DA900, 0xBF64E465,
    ],
    [
        0x3D929822, 0x3DCF1E75, 0xBDCA18BD, 0x3F0B5BD6, 0xBE0DA900, 0xBE0DA900,
    ],
    [
        0x3DCB8AD2, 0x3E07E85B, 0x3E9DA390, 0x3F205D41, 0xBE0DA900, 0x3F8D8DCE,
    ],
    [
        0x3E027F20, 0x3E2A5634, 0x3F330B8B, 0x3F342C93, 0xBE0DA900, 0xBDCA18BD,
    ],
    [
        0x3E1F6FC8, 0x3E4B2A3D, 0x3F7E0420, 0x3F46383C, 0xBE0DA900, 0xBF64E465,
    ],
    [
        0x3E3C8D93, 0x3E6C2403, 0x3F8D8DCE, 0x3F571163, 0xBE0DA900, 0xBE0DA900,
    ],
    [
        0x3E59CE5D, 0x3E869A42, 0x3F81D0C3, 0x3F663479, 0xBE0DA900, 0x3F8D8DCE,
    ],
    [
        0x3E7727BC, 0x3E97263A, 0x3F3A396C, 0x3F7349D0, 0xBE0DA900, 0xBDCA18BD,
    ],
    [
        0x3E8A4789, 0x3EA89904, 0x3E98FCC9, 0x3F7EC9CA, 0xBE0DA900, 0xBF64E465,
    ],
    [
        0x3E98FCC9, 0x3EB9144A, 0xBE0DA900, 0x3F8419F4, 0xBE0DA900, 0xBE0DA900,
    ],
    [
        0x3EA7AE2E, 0x3EC97B94, 0xBEEEE161, 0x3F87F167, 0xBE0DA900, 0x3F8D8DCE,
    ],
    [
        0x3EB65642, 0x3ED9C730, 0xBF2FD07A, 0x3F8ACD25, 0xBE0DA900, 0xBDCA18BD,
    ],
    [
        0x3EC4EF8F, 0x3EE9EF89, 0xBF556538, 0x3F8CA375, 0xBE0DA900, 0xBF64E465,
    ],
    [
        0x3ED374AC, 0x3EFACF3E, 0xBF64E465, 0x3F8D851F, 0xBE0DA900, 0xBE0DA900,
    ],
    [
        0x3EE1E03D, 0x3F054C06, 0xBF5B029D, 0x3F8D63F1, 0xBE0DA900, 0x3F8D8DCE,
    ],
    [
        0x3EF02CFF, 0x3F0D13CE, 0xBF36FE5C, 0x3F8C4737, 0xBE0DA900, 0xBDCA18BD,
    ],
    [
        0x3EFE55CB, 0x3F14BBA0, 0xBEEA3A9B, 0x3F8A23CB, 0xBE0DA900, 0xBF64E465,
    ],
    [
        0x3F062ACF, 0x3F1C404D, 0xBDCA18BD, 0x3F87124A, 0xBE0DA900, 0xBE0DA900,
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

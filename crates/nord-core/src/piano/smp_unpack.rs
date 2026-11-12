//! Instruction-level model of the SMP voice loop's residual unpacker and 4-tap predictor.
//!
//! Spec: `docs/specs/piano-engine.md` ("SMP sample interpolation"),
//! `docs/research/smp-kernel-fit.md` section "Unpacker". Source: ADI `elfdump` listing of SMP
//! `0x1C07D8..0x1C08E3` (`python tools/adi_dis.py smp.bin 0x380FB0 0x3811D0`, CCES 2.12.1).
//!
//! What the loop does per output sample (VERIFIED, instruction forms):
//!
//! | Step | Instructions | Model |
//! |---|---|---|
//! | Bit position | `r0 = r6*r7 (UUI) - r1` | `bit = width * index - base` |
//! | Unit and offset | `r3 = FEXT r0 BY 0:4`, `r0 = LSHIFT r0 BY -4`, `AND mask` | `unit = bit >> 4`, `off = bit & 15` |
//! | Fetch | `r5 = DM(I1,M1); r1 = DM(I1,M1); r5 = r5 OR LSHIFT r1 BY 16` | `units[unit] \| units[unit + 1] << 16` |
//! | Align | `r3 = -r3; r5 = LSHIFT r5 BY r3` | `>> off` |
//! | Field | `r5 = FEXT r5 BY (width << 6)`, `r5 = FDEP r5 BY (width << 6) (SE)` | low `width` bits, sign-extended |
//! | Predict | `MRF = 0; MRF += R0*R1 (SSI)` x4, `R0 = DM(I5,M2)`, `R1 = PM(I9,M9)` | `y = sum c_i h_i + residual` |
//!
//! Each 32-bit memory word carries 16 stream bits (low half): the `OR LSHIFT 16` would corrupt
//! them otherwise. The stream is LSB first. Widths 1..=16 fit the 32-bit window
//! (`off <= 15`, `off + width <= 31`), which is exactly the CNSP range (`nbits = field + 1`,
//! 1..=16) of `crate::cbin::sample`. A second copy of the loop (`0x1C0865`, `R13 = 0x20`, `mode1`
//! bits set) assembles three units and serves wider fields; it is not needed for CNSP.
//!
//! The coefficient vector lives in program memory per voice (`PM(I9)`), written by the ARM; the
//! binomial rows of [`FIXED_LPC_COEFFS`] are the only vectors that reproduce the fixed-LPC codec
//! (ASSUMED: the ARM loads exactly these, oldest history first). The ARM-side re-layout from the
//! file's big-endian MSB-first words to this LSB-first 16-bit-unit stream is not known
//! (OPEN: not decoded); only the DSP-side semantics are modelled here.

/// Smallest and largest residual width the unpacker handles in one 32-bit window.
pub const WIDTHS: std::ops::RangeInclusive<u32> = 1..=16;

/// Predictor coefficient rows for fixed-LPC orders 0..=4, oldest history sample first
/// (`pred = c . [h-4, h-3, h-2, h-1]`). ASSUMED (ARM-side data); equal to the FLAC fixed
/// polynomials used by `crate::cbin::sample::fixed_lpc_predict`.
pub const FIXED_LPC_COEFFS: [[i32; 4]; 5] = [
    [0, 0, 0, 0],
    [0, 0, 0, 1],
    [0, 0, -1, 2],
    [0, 1, -3, 3],
    [-1, 4, -6, 4],
];

/// Packs `residuals` into the DSP layout: LSB-first bit stream, 16 bits per 32-bit word.
pub fn pack_units(residuals: &[i32], width: u32) -> Vec<u32> {
    let mask = (1u32 << width) - 1;
    let mut units = vec![0u32; (residuals.len() * width as usize).div_ceil(16)];
    for (i, &r) in residuals.iter().enumerate() {
        let field = (r as u32) & mask;
        for b in 0..width as usize {
            if field >> b & 1 == 1 {
                let bit = i * width as usize + b;
                units[bit >> 4] |= 1 << (bit & 15);
            }
        }
    }
    units
}

/// Model of the fetch/align/`FEXT`/`FDEP (SE)` sequence: residual number `index` of a stream
/// whose first residual starts at bit 0. `None` if the stream is too short.
pub fn extract_residual(units: &[u32], width: u32, index: usize) -> Option<i32> {
    if !WIDTHS.contains(&width) {
        return None;
    }
    let bit = width as usize * index;
    let (unit, off) = (bit >> 4, (bit & 15) as u32);
    let lo = *units.get(unit)? & 0xFFFF;
    let hi = units.get(unit + 1).copied().unwrap_or(0) & 0xFFFF;
    let window = (lo | (hi << 16)) >> off;
    let field = window & ((1u32 << width) - 1);
    let shift = 32 - width;
    Some(((field << shift) as i32) >> shift)
}

/// Model of `MRF += R0*R1 (SSI)` x4: signed integer multiply-accumulate, low 32 bits kept.
pub fn predict(coefs: &[i32; 4], hist: &[i32; 4]) -> i32 {
    let acc: i64 = coefs
        .iter()
        .zip(hist)
        .map(|(&c, &h)| i64::from(c) * i64::from(h))
        .sum();
    acc as i32
}

/// Decodes `count` samples of one block: unpack, predict with row `order`, push into the history.
pub fn decode_block(
    units: &[u32],
    width: u32,
    order: usize,
    count: usize,
    hist: &mut [i32; 4],
) -> Option<Vec<i32>> {
    let coefs = FIXED_LPC_COEFFS.get(order)?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let y = predict(coefs, hist).wrapping_add(extract_residual(units, width, i)?);
        *hist = [hist[1], hist[2], hist[3], y];
        out.push(y);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbin::sample::{decode_nsmp_stk_payload, fixed_lpc_predict};

    /// Deterministic residuals that fit `width` signed bits.
    fn residuals(width: u32, n: usize, seed: u32) -> Vec<i32> {
        let (lo, span) = (-(1i64 << (width - 1)), 1u64 << width);
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (lo + ((u64::from(s >> 8) * span) >> 24) as i64) as i32
            })
            .collect()
    }

    #[test]
    fn test_unpack_roundtrip_all_widths_including_extremes() {
        for width in WIDTHS {
            let mut r = residuals(width, 61, width);
            r[0] = -(1 << (width - 1)); // most negative
            r[1] = (1 << (width - 1)) - 1; // most positive
            r[2] = -1;
            let units = pack_units(&r, width);
            assert!(
                units.iter().all(|&u| u <= 0xFFFF),
                "16-bit clean, width {width}"
            );
            for (i, &want) in r.iter().enumerate() {
                assert_eq!(
                    extract_residual(&units, width, i),
                    Some(want),
                    "w {width} i {i}"
                );
            }
            assert_eq!(extract_residual(&units, width, 61 + 16), None);
        }
        assert_eq!(extract_residual(&[0], 0, 0), None);
        assert_eq!(extract_residual(&[0, 0], 17, 0), None);
    }

    #[test]
    fn test_binomial_rows_equal_rust_fixed_lpc_predictor() {
        let hists: [[i64; 4]; 4] = [
            [0; 4],
            [100, -250, 3000, -7],
            [-32768, 32767, -1, 1],
            [5, 4, 3, 2],
        ];
        for h in hists {
            let hi = h.map(|v| v as i32);
            for order in 0..=4u8 {
                assert_eq!(
                    i64::from(predict(&FIXED_LPC_COEFFS[order as usize], &hi)),
                    fixed_lpc_predict(order, &h),
                    "order {order} hist {h:?}"
                );
            }
        }
    }

    /// Minimal mono `\0stk` payload with one frame (header layout of `cbin::sample`).
    fn mono_stk(samples: &[i32], order: u8, width: u32) -> Vec<u8> {
        let mut hist = [0i64; 4];
        let res: Vec<i32> = samples
            .iter()
            .map(|&s| {
                let r = i64::from(s) - fixed_lpc_predict(order, &hist);
                hist = [hist[1], hist[2], hist[3], i64::from(s)];
                r as i32
            })
            .collect();
        let mut payload = vec![0u8; 0x44 + 8];
        payload[0x05] = 60;
        payload[0x06..0x08].copy_from_slice(&44_100u16.to_be_bytes());
        payload[0x08] = 1;
        payload[0x29] = 0x11; // one-shot
        let ctrl = (samples.len() as u32 & 0x3FFF) | (u32::from(order) << 14) | ((width - 1) << 19);
        payload.extend_from_slice(&ctrl.to_be_bytes());
        // File layout: big-endian words, MSB first.
        let (mut acc, mut bits) = (0u64, 0u32);
        let mask = (1u64 << width) - 1;
        for r in res {
            acc = (acc << width) | (r as u32 as u64 & mask);
            bits += width;
            while bits >= 32 {
                bits -= 32;
                payload.extend_from_slice(&((acc >> bits) as u32).to_be_bytes());
                acc &= (1u64 << bits) - 1;
            }
        }
        if bits > 0 {
            payload.extend_from_slice(&((acc << (32 - bits)) as u32).to_be_bytes());
        }
        payload.extend_from_slice(&0x0080_0020u32.to_be_bytes());
        payload
    }

    #[test]
    fn test_smp_model_matches_rust_cnsp_decoder_for_all_widths_and_orders() {
        for nbits in WIDTHS {
            for (order, coefs) in FIXED_LPC_COEFFS.iter().enumerate() {
                // Residuals that fit `nbits` signed bits (amplitude capped so order 4 stays small).
                let res = residuals(nbits.min(6), 40, 7 * nbits + order as u32);
                let mut hist = [0i32; 4];
                let samples: Vec<i32> = res
                    .iter()
                    .map(|&r| {
                        let y = predict(coefs, &hist) + r;
                        hist = [hist[1], hist[2], hist[3], y];
                        y
                    })
                    .collect();
                // Existing decoder: big-endian MSB-first file layout.
                let dec = decode_nsmp_stk_payload(&mono_stk(&samples, order as u8, nbits))
                    .expect("decode");
                // SMP model: the same residuals in the LSB-first 16-bit-unit DSP layout.
                let units = pack_units(&res, nbits);
                let mut h4 = [0i32; 4];
                let smp = decode_block(&units, nbits, order, samples.len(), &mut h4).unwrap();
                assert_eq!(smp, samples, "SMP model, width {nbits} order {order}");
                for (i, &s) in samples.iter().enumerate() {
                    assert!(
                        (dec.left[i] - s as f32 / 8192.0).abs() < 1e-6,
                        "w {nbits} o {order}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_decode_block_rejects_bad_order_and_short_stream() {
        let mut h = [0i32; 4];
        assert!(decode_block(&[0], 4, 5, 1, &mut h).is_none());
        assert!(decode_block(&[0], 4, 0, 100, &mut h).is_none());
    }

    #[test]
    fn test_full_scale_16_bit_residuals_with_order_4_history() {
        // Worst case for the 32-bit MAC: order 4 with 16-bit samples never overflows i32.
        let hist = [32767, -32768, 32767, -32768];
        let p = predict(&FIXED_LPC_COEFFS[4], &hist);
        assert_eq!(i64::from(p), fixed_lpc_predict(4, &hist.map(i64::from)));
    }
}

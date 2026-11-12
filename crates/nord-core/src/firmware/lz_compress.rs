//! Clavia LZ compressor (literal runs plus greedy matches); inverse of `decompress_clavia_lz`.

fn push_lzo_literals_zero_state(out: &mut Vec<u8>, lits: &[u8]) {
    if lits.is_empty() {
        return;
    }
    if lits.len() <= 3 {
        // Should only be used as initial literal run or handled via M1/M2; fallback for initial <4
        out.push(17 + lits.len() as u8);
        out.extend_from_slice(lits);
        return;
    }
    let rem = lits.len() - 3;
    if rem <= 15 {
        out.push(rem as u8);
    } else {
        out.push(0);
        let mut extra = rem - 15;
        while extra >= 255 {
            out.push(0);
            extra -= 255;
        }
        out.push(extra as u8);
    }
    out.extend_from_slice(lits);
}

/// Compresses raw data into LZO1X format compatible with `decompress_clavia_lz`.
pub fn compress_clavia_lz(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(input.len() + 16);
    let mut in_pos = 0usize;
    let mut lit_start = 0usize;

    let flush_lits = |out: &mut Vec<u8>, start: usize, end: usize, first_emit: &mut bool| {
        if start >= end {
            return;
        }
        let lits = &input[start..end];
        if *first_emit {
            *first_emit = false;
            if lits.len() <= 238 {
                out.push(17 + lits.len() as u8);
                out.extend_from_slice(lits);
                return;
            }
        }
        if lits.len() >= 4 {
            push_lzo_literals_zero_state(out, lits);
        } else {
            // Emit short run using M2/initial-style isn't valid mid-stream with state=0 unless folded;
            // handled cleanly by emitting as individual initial-or-deferred bytes when possible.
            // To guarantee valid state=0 LZO1X without ambiguity, only split >=4 literals or encode 3+!
            unreachable!("short literal run should only flush when >= 4 or at end of stream");
        }
    };

    let mut first_emit = true;
    while in_pos < input.len() {
        let win_start = in_pos.saturating_sub(2048);
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        for start in win_start..in_pos {
            let dist = in_pos - start;
            let mut len = 0usize;
            while in_pos + len < input.len() && len < 8 && input[start + len] == input[in_pos + len]
            {
                len += 1;
            }
            if len >= 3 && len > best_len {
                best_len = len;
                best_dist = dist;
                if len == 8 {
                    break;
                }
            }
        }

        let lit_len = in_pos - lit_start;
        // Only emit a match if pending literals can be flushed validly (0, or >=4, or <=3 when first_emit)
        let can_flush_lits = lit_len == 0 || lit_len >= 4 || first_emit;
        if best_len >= 3 && can_flush_lits {
            if lit_len > 0 {
                flush_lits(&mut out, lit_start, in_pos, &mut first_emit);
            }
            first_emit = false;
            let d = best_dist - 1;
            let t = (((best_len - 1) as u8) << 5) | (((d & 7) as u8) << 2);
            let hi = (d >> 3) as u8;
            out.push(t);
            out.push(hi);
            in_pos += best_len;
            lit_start = in_pos;
        } else {
            in_pos += 1;
        }
    }

    let rem_lits = in_pos - lit_start;
    if rem_lits > 0 {
        let lits = &input[lit_start..in_pos];
        if first_emit && lits.len() <= 238 {
            out.push(17 + lits.len() as u8);
            out.extend_from_slice(lits);
        } else if lits.len() >= 4 {
            push_lzo_literals_zero_state(&mut out, lits);
        } else {
            // Rewind is unnecessary if we ensure any trailing <4 literals stay attached;
            // if a preceding match existed with state=0, append M1-free EOF?
            // Simple valid LZO1X tail for 1..3 trailing bytes: patch previous match's state bits!
            // Actually, if a match was emitted right before lit_start, out[len-2] holds M2 byte t with state=0:
            if out.len() >= 2 && (out[out.len() - 2] >= 64) {
                let t_idx = out.len() - 2;
                out[t_idx] |= rem_lits as u8;
                out.extend_from_slice(lits);
            } else {
                out.push(17 + lits.len() as u8);
                out.extend_from_slice(lits);
            }
        }
    }

    // LZO1X explicit end-of-stream marker (M4 with dist_lo == 0)
    out.extend_from_slice(&[0x11, 0x00, 0x00]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmware::decompress_clavia_lz;

    #[test]
    fn compressed_repeating_data_round_trips() {
        let data: Vec<u8> = (0..300u32).map(|i| (i % 7) as u8).collect();
        let packed = compress_clavia_lz(&data);
        assert_eq!(decompress_clavia_lz(&packed, data.len()).unwrap(), data);
    }
}

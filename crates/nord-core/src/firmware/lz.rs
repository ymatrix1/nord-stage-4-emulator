//! Clavia LZ (LZO-like) decompressor.

#[inline]
fn read_lzo_byte(input: &[u8], ip: &mut usize) -> Result<u8, String> {
    if *ip >= input.len() {
        return Err(format!(
            "unexpected end of LZO1X stream at input offset 0x{:X}",
            *ip
        ));
    }
    let b = input[*ip];
    *ip += 1;
    Ok(b)
}

#[inline]
fn read_lzo_ext_len(input: &[u8], ip: &mut usize, base: usize) -> Result<usize, String> {
    let mut len = base;
    while read_lzo_byte(input, ip)? == 0 {
        len = len
            .checked_add(255)
            .ok_or_else(|| "LZO1X extended length overflow".to_string())?;
    }
    let tail = input[*ip - 1] as usize;
    len.checked_add(tail)
        .ok_or_else(|| "LZO1X extended length overflow".to_string())
}

#[inline]
fn copy_lzo_literals(
    input: &[u8],
    ip: &mut usize,
    out: &mut Vec<u8>,
    count: usize,
    max_output_size: usize,
) -> Result<(), String> {
    if *ip + count > input.len() {
        return Err(format!(
            "LZO1X literal run ({count} B) exceeds input bounds at 0x{:X}",
            *ip
        ));
    }
    if out.len().saturating_add(count) > max_output_size {
        return Err(format!(
            "LZO1X literal run exceeds max_output_size {max_output_size}"
        ));
    }
    out.extend_from_slice(&input[*ip..*ip + count]);
    *ip += count;
    Ok(())
}

#[inline]
fn copy_lzo_match(
    out: &mut Vec<u8>,
    distance: usize,
    length: usize,
    max_output_size: usize,
) -> Result<(), String> {
    if distance == 0 {
        return Err("invalid LZO1X match distance 0".to_string());
    }
    if distance > out.len() {
        return Err(format!(
            "LZO1X distance {distance} exceeds current output buffer length {}",
            out.len()
        ));
    }
    if out.len().saturating_add(length) > max_output_size {
        return Err(format!(
            "LZO1X match exceeds max_output_size {max_output_size}"
        ));
    }
    for pos in (out.len() - distance..).take(length) {
        let byte = out[pos];
        out.push(byte);
    }
    Ok(())
}

/// Decompresses a byte slice encoded with Clavia's OS firmware compression (LZO1X).
///
/// Proved bit-exact against `Update Nord Stage 4 OS v1.66.exe` Image 0
/// (`5,499,504` B, checksum `0xDE902503`) and Image 1 (`798,912` B, checksum `0xFBB813F8`).
pub fn decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(max_output_size.min(1024 * 1024));
    let mut ip = 0usize;
    let mut skip_first_op = false;

    let first = read_lzo_byte(input, &mut ip)?;
    if first > 17 {
        let count = (first - 17) as usize;
        copy_lzo_literals(input, &mut ip, &mut out, count, max_output_size)?;
        if count < 4 {
            skip_first_op = true;
        }
    } else {
        ip = 0;
    }

    while ip < input.len() {
        let mut t = if !skip_first_op {
            let op = read_lzo_byte(input, &mut ip)?;
            if op < 16 {
                let lit_len = if op == 0 {
                    read_lzo_ext_len(input, &mut ip, 15)? + 3
                } else {
                    op as usize + 3
                };
                copy_lzo_literals(input, &mut ip, &mut out, lit_len, max_output_size)?;
                if ip >= input.len() {
                    break;
                }
                let next_op = read_lzo_byte(input, &mut ip)?;
                if next_op < 16 {
                    let lo = read_lzo_byte(input, &mut ip)? as usize;
                    let dist = 1 + 0x0800 + ((next_op as usize) >> 2) + (lo << 2);
                    copy_lzo_match(&mut out, dist, 3, max_output_size)?;
                    let state = (next_op & 3) as usize;
                    if state > 0 {
                        copy_lzo_literals(input, &mut ip, &mut out, state, max_output_size)?;
                        skip_first_op = true;
                    }
                    continue;
                }
                next_op
            } else {
                op
            }
        } else {
            skip_first_op = false;
            read_lzo_byte(input, &mut ip)?
        };

        loop {
            let state = if t >= 64 {
                let hi = read_lzo_byte(input, &mut ip)? as usize;
                let dist = 1 + (((t as usize) >> 2) & 7) + (hi << 3);
                let m_len = ((t as usize) >> 5) + 1;
                copy_lzo_match(&mut out, dist, m_len, max_output_size)?;
                (t & 3) as usize
            } else if t >= 32 {
                let base_len = (t & 31) as usize;
                let m_len = if base_len == 0 {
                    read_lzo_ext_len(input, &mut ip, 31)? + 2
                } else {
                    base_len + 2
                };
                let b0 = read_lzo_byte(input, &mut ip)?;
                let b1 = read_lzo_byte(input, &mut ip)?;
                let dist = 1 + ((b0 as usize) >> 2) + ((b1 as usize) << 6);
                copy_lzo_match(&mut out, dist, m_len, max_output_size)?;
                (b0 & 3) as usize
            } else if t >= 16 {
                let dist_hi = ((t as usize) & 8) << 11;
                let base_len = (t & 7) as usize;
                let m_len = if base_len == 0 {
                    read_lzo_ext_len(input, &mut ip, 7)? + 2
                } else {
                    base_len + 2
                };
                let b0 = read_lzo_byte(input, &mut ip)?;
                let b1 = read_lzo_byte(input, &mut ip)?;
                let dist_lo = ((b0 as usize) >> 2) + ((b1 as usize) << 6);
                let dist = dist_hi + dist_lo;
                if dist == 0 {
                    return Ok(out);
                }
                copy_lzo_match(&mut out, dist + 0x4000, m_len, max_output_size)?;
                (b0 & 3) as usize
            } else {
                let hi = read_lzo_byte(input, &mut ip)? as usize;
                let dist = 1 + ((t as usize) >> 2) + (hi << 2);
                copy_lzo_match(&mut out, dist, 2, max_output_size)?;
                (t & 3) as usize
            };

            if state > 0 {
                copy_lzo_literals(input, &mut ip, &mut out, state, max_output_size)?;
                t = read_lzo_byte(input, &mut ip)?;
            } else {
                break;
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_an_error_or_empty_never_a_panic() {
        let _ = decompress_clavia_lz(&[], 16);
    }
}

//! Helpers shared by the PNO Core1 oracle binaries (`sharc-bank`, `sharc-organ`): number and
//! register parsing, poke scripts, the documented start-up and the single-routine call.
//!
//! Spec: `docs/research/organ-tonewheel-bank.md` section 2 (start-up),
//! `docs/research/organ-oracle-audio.md`.
use crate::core::Core;

/// Parses `0x` hex or decimal.
pub fn parse_number(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Universal register code of `R3`, `I10`, `M2`, `L0`, `B7` (group in bits 4..7, index 0..15).
pub fn register_code(name: &str) -> Option<u8> {
    let group = match name.get(..1)? {
        "R" => 0,
        "I" => 1,
        "M" => 2,
        "L" => 3,
        "B" => 4,
        _ => return None,
    };
    let n: u8 = name.get(1..)?.parse().ok().filter(|n| *n < 16)?;
    Some((group << 4) | n)
}

/// Pokes of a script: `(call, address, value)` sorted by call. Lines are `CALL ADDR VALUE`.
pub fn parse_script(text: &str) -> Result<Vec<(u64, u32, u32)>, String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let [call, addr, value] = f.as_slice() else {
            return Err(format!("bad script line: {line}"));
        };
        out.push((
            parse_number(call).ok_or("bad call")?,
            parse_number(addr).ok_or("bad address")? as u32,
            parse_number(value).ok_or("bad value")? as u32,
        ));
    }
    out.sort_by_key(|p| p.0);
    Ok(out)
}

/// Boots a PNO Core1 flat SRAM image to its idle loop with the device presets of
/// `docs/research/pno-dsp-map.md` section 3, then ORs `mode1` into MODE1.
pub fn boot_pno(data: &[u8], entry: u32, boot_steps: u64, mode1: u32) -> Result<Core, String> {
    let mut core = Core::new(data);
    core.pc = entry;
    for (addr, value) in [
        (0x28ac_0380u32, 1u32),
        (0x3108_d008, 5),
        (0x3108_c06c, 0x400),
        (0x3108_c00c, 4),
    ] {
        core.preset_device(addr, value);
    }
    core.set_flag_input(1, true);
    if let Some(f) = core.run(boot_steps) {
        return Err(format!("boot fault: {f:?}"));
    }
    core.mode1 |= mode1;
    Ok(core)
}

/// Runs the routine at `call` until `pc == ret` (before executing it); returns the step count.
pub fn call_once(core: &mut Core, call: u32, ret: u32, limit: u64) -> Result<u64, String> {
    core.pc = call;
    let start = core.steps;
    while core.pc != ret {
        if core.steps - start > limit {
            return Err(format!(
                "no return after {limit} steps (pc 0x{:x})",
                core.pc
            ));
        }
        core.step()
            .map_err(|f| format!("fault at pc 0x{:x}: {f:?}", core.pc))?;
    }
    Ok(core.steps - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_parse_in_hex_and_decimal() {
        assert_eq!(parse_number("0x10"), Some(16));
        assert_eq!(parse_number("17"), Some(17));
        assert_eq!(parse_number("zz"), None);
    }

    #[test]
    fn script_lines_parse_and_sort() {
        let p = parse_script("5 0x10 0x20 # c\n\n1 0x11 7\n").unwrap();
        assert_eq!(p, vec![(1, 0x11, 7), (5, 0x10, 0x20)]);
        assert!(parse_script("1 2\n").is_err());
    }

    #[test]
    fn register_names_map_to_universal_codes() {
        assert_eq!(register_code("M10"), Some(0x2A));
        assert_eq!(register_code("I0"), Some(0x10));
        assert_eq!(register_code("X1"), None);
    }

    #[test]
    fn a_call_that_never_reaches_the_stop_pc_reports_it() {
        let image = vec![0u8; 0x24_0000 + 16];
        let mut core = Core::new(&image);
        let r = call_once(&mut core, 0x120000, 0x7FFFFF, 50);
        assert!(r.is_err(), "{r:?}");
    }
}

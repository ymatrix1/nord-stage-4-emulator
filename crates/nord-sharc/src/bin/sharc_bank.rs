//! `sharc-bank IMAGE [options]` runs the PNO Core1 wavetable oscillator bank (routine at pc
//! `0x122620`, see `docs/research/organ-tonewheel-bank.md`) in the SHARC oracle: boot the section to
//! its idle loop (the documented start-up), switch SIMD on, then call the routine N times with
//! scripted data pokes between the calls and write the two output arrays of every call.
//!
//! ```text
//! --calls N         number of calls (default 1)            --boot N   start-up steps (default 1500000)
//! --entry PC        section entry (default 0x121007)       --call PC  routine (default 0x122620)
//! --ret PC          stop pc of a call (default 0x12268C, the RTS (DB) of the routine)
//! --mode1 V         OR V into MODE1 after the start-up (default 0x200000 = PEYEN; 0 = scalar)
//! --set REG=V       preset R/I/M/L/B register after the start-up (repeatable)
//! --pre ADDR=V      data poke before the first call        --script FILE  lines `CALL ADDR VALUE`
//! --out FILE        write per call 2 * 368 little-endian u32 words: array x then array y
//! --range ADDR:COUNT with --out: write these data words per call instead (repeatable)
//! --dump ADDR:COUNT print data words after the last call
//! ```
//! Numbers accept `0x` hex or decimal; `ADDR` is a normal-word data address.
use nord_sharc::core::Core;
use std::process::ExitCode;

/// Words of one output array per call: two frames of `0xB8`.
const ARRAY_WORDS: u32 = 0x170;

fn parse_number(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

fn register_code(name: &str) -> Option<u8> {
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

/// Pokes of a script: `(call, address, value)` sorted by call.
fn parse_script(text: &str) -> Result<Vec<(u64, u32, u32)>, String> {
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

/// Runs the routine once; returns the number of steps or an error text.
fn call_once(core: &mut Core, call: u32, ret: u32, limit: u64) -> Result<u64, String> {
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

fn run(args: &[String]) -> Result<String, String> {
    let mut image = None;
    let (mut calls, mut boot, mut entry, mut routine, mut ret) =
        (1u64, 1_500_000u64, 0x121007u32, 0x122620u32, 0x12268Cu32);
    let mut mode1 = 0x20_0000u32;
    let mut registers = Vec::new();
    let mut pokes: Vec<(u64, u32, u32)> = Vec::new();
    let (mut out_path, mut dumps) = (None, Vec::new());
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |what: &str| it.next().cloned().ok_or(format!("{a} needs {what}"));
        match a.as_str() {
            "--calls" => calls = parse_number(&val("N")?).ok_or("bad --calls")?,
            "--boot" => boot = parse_number(&val("N")?).ok_or("bad --boot")?,
            "--entry" => entry = parse_number(&val("PC")?).ok_or("bad --entry")? as u32,
            "--call" => routine = parse_number(&val("PC")?).ok_or("bad --call")? as u32,
            "--ret" => ret = parse_number(&val("PC")?).ok_or("bad --ret")? as u32,
            "--mode1" => mode1 = parse_number(&val("V")?).ok_or("bad --mode1")? as u32,
            "--out" => out_path = Some(val("FILE")?),
            "--set" | "--pre" => {
                let spec = val("A=V")?;
                let (k, v) = spec.split_once('=').ok_or("needs NAME=VALUE")?;
                let v = parse_number(v).ok_or("bad value")? as u32;
                if a == "--set" {
                    registers.push((register_code(k).ok_or("bad register")?, v));
                } else {
                    pokes.push((0, parse_number(k).ok_or("bad address")? as u32, v));
                }
            }
            "--script" => {
                let path = val("FILE")?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                pokes.extend(parse_script(&text)?);
            }
            "--range" => {
                let spec = val("ADDR:COUNT")?;
                let (k, v) = spec.split_once(':').ok_or("needs ADDR:COUNT")?;
                ranges.push((
                    parse_number(k).ok_or("bad address")? as u32,
                    parse_number(v).ok_or("bad count")? as u32,
                ));
            }
            "--dump" => {
                let spec = val("ADDR:COUNT")?;
                let (k, v) = spec.split_once(':').ok_or("needs ADDR:COUNT")?;
                dumps.push((
                    parse_number(k).ok_or("bad address")? as u32,
                    parse_number(v).ok_or("bad count")? as u32,
                ));
            }
            _ => image = Some(a.clone()),
        }
    }
    let image = image.ok_or("usage: sharc-bank IMAGE [options]; see the module docs")?;
    let data = std::fs::read(&image).map_err(|e| format!("{image}: {e}"))?;
    let mut core = Core::new(&data);
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
    if let Some(f) = core.run(boot) {
        return Err(format!("boot fault: {f:?}"));
    }
    core.mode1 |= mode1;
    for (code, value) in registers {
        core.write_ureg(code, value);
    }
    pokes.sort_by_key(|p| p.0);
    let mut next = 0;
    let mut bytes: Vec<u8> = Vec::new();
    let mut steps = 0;
    for n in 0..calls {
        while next < pokes.len() && pokes[next].0 <= n {
            let (_, addr, value) = pokes[next];
            core.dm_write(addr, value)
                .map_err(|a| format!("bad poke 0x{a:x}"))?;
            next += 1;
        }
        steps = call_once(&mut core, routine, ret, 100_000)?;
        let default = [(0xC24E5u32, ARRAY_WORDS), (0xC2655, ARRAY_WORDS)];
        for &(base, count) in if ranges.is_empty() {
            &default[..]
        } else {
            &ranges[..]
        } {
            for k in 0..count {
                let w = core.dm_read(base + k).unwrap_or(0);
                bytes.extend_from_slice(&w.to_le_bytes());
            }
        }
    }
    if let Some(path) = out_path {
        std::fs::write(&path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    }
    let mut text = format!(
        "calls {calls}, steps per call {steps}, mode1 0x{:x}\n",
        core.mode1
    );
    for (addr, count) in dumps {
        let words: Vec<String> = (0..count)
            .map(|k| format!("{:x}", core.dm_read(addr + k).unwrap_or(0)))
            .collect();
        text.push_str(&format!("DM(0x{addr:x}..): {}\n", words.join(" ")));
    }
    Ok(text)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_lines_parse_and_sort() {
        let p = parse_script("5 0x10 0x20 # c\n\n1 0x11 7\n").unwrap();
        assert_eq!(p, vec![(1, 0x11, 7), (5, 0x10, 0x20)]);
        assert!(parse_script("1 2\n").is_err());
    }

    #[test]
    fn register_names_map_to_universal_codes() {
        assert_eq!(register_code("M10"), Some(0x2A));
        assert_eq!(register_code("X1"), None);
    }

    #[test]
    fn a_missing_image_is_an_error() {
        assert!(run(&["/nonexistent/image.bin".to_string()]).is_err());
    }

    #[test]
    fn a_call_that_never_reaches_the_stop_pc_reports_it() {
        let image = vec![0u8; 0x24_0000 + 16];
        let mut core = Core::new(&image);
        let r = call_once(&mut core, 0x120000, 0x7FFFFF, 50);
        assert!(r.is_err(), "{r:?}");
    }
}

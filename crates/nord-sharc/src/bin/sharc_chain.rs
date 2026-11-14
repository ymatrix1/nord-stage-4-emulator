//! `sharc-chain IMAGE [options]` runs any sequence of PNO Core1 routines in the SHARC oracle:
//! boot to the idle loop, then per call every `--step ENTRY:RET[:REG=V,...]` in order (registers
//! preset before the step, the routine runs until `pc == RET`), with scripted data pokes between
//! calls. Generalises `sharc-organ` (bank, mix, post stage) to the whole pass of a slot program.
//! Method and results: `docs/research/organ-post-stage.md`.
//!
//! ```text
//! --calls N            number of passes (default 1)       --boot N   start-up steps (default 1500000)
//! --step E:R[:REG=V,..] routine entry pc, stop pc, presets (I0, I1, M2, R3 ...; repeatable, in order)
//! --mode1 V            OR V into MODE1 after the start-up (default 0x200000 = PEYEN)
//! --pre ADDR=V         data poke before the first call    --script FILE  lines `CALL ADDR VALUE`
//! --out FILE           per call the words of the --range windows, little-endian u32
//! --range ADDR:COUNT   capture window (repeatable)        --dump ADDR:COUNT  print words at the end
//! --trace LO:HI FILE   write `pc R0..R15 S0..S15 I0..I15 M0..M15` (hex, after the instruction) for
//!                      every executed instruction whose pc is in [LO, HI), first call only
//! ```
use nord_sharc::core::Core;
use nord_sharc::harness::{boot_pno, call_once, parse_number, parse_script, register_code};
use std::process::ExitCode;

const STEP_LIMIT: u64 = 400_000;

/// One routine of the pass: entry pc, stop pc and register presets.
#[derive(Debug, PartialEq)]
struct Step {
    entry: u32,
    ret: u32,
    regs: Vec<(u8, u32)>,
}

fn parse_step(spec: &str) -> Result<Step, String> {
    let mut parts = spec.splitn(3, ':');
    let num = |s: Option<&str>| {
        s.and_then(parse_number)
            .map(|n| n as u32)
            .ok_or_else(|| format!("bad --step {spec}"))
    };
    let entry = num(parts.next())?;
    let ret = num(parts.next())?;
    let mut regs = Vec::new();
    for item in parts
        .next()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
    {
        let (k, v) = item.split_once('=').ok_or("preset needs REG=V")?;
        regs.push((
            register_code(k).ok_or("bad register")?,
            parse_number(v).ok_or("bad value")? as u32,
        ));
    }
    Ok(Step { entry, ret, regs })
}

struct Opts {
    image: String,
    calls: u64,
    boot: u64,
    mode1: u32,
    steps: Vec<Step>,
    pokes: Vec<(u64, u32, u32)>,
    out: Option<String>,
    ranges: Vec<(u32, u32)>,
    dumps: Vec<(u32, u32)>,
    trace: Option<((u32, u32), String)>,
}

fn pair(spec: &str, sep: char) -> Result<(u32, u32), String> {
    let (k, v) = spec.split_once(sep).ok_or("needs A<sep>B")?;
    Ok((
        parse_number(k).ok_or("bad number")? as u32,
        parse_number(v).ok_or("bad number")? as u32,
    ))
}

fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        image: String::new(),
        calls: 1,
        boot: 1_500_000,
        mode1: 0x20_0000,
        steps: Vec::new(),
        pokes: Vec::new(),
        out: None,
        ranges: Vec::new(),
        dumps: Vec::new(),
        trace: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |what: &str| it.next().cloned().ok_or(format!("{a} needs {what}"));
        match a.as_str() {
            "--calls" => o.calls = parse_number(&val("N")?).ok_or("bad --calls")?,
            "--boot" => o.boot = parse_number(&val("N")?).ok_or("bad --boot")?,
            "--mode1" => o.mode1 = parse_number(&val("V")?).ok_or("bad --mode1")? as u32,
            "--step" => o.steps.push(parse_step(&val("E:R[:REG=V,..]")?)?),
            "--out" => o.out = Some(val("FILE")?),
            "--pre" => {
                let (k, v) = pair(&val("A=V")?, '=')?;
                o.pokes.push((0, k, v));
            }
            "--script" => {
                let path = val("FILE")?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                o.pokes.extend(parse_script(&text)?);
            }
            "--range" => o.ranges.push(pair(&val("ADDR:COUNT")?, ':')?),
            "--dump" => o.dumps.push(pair(&val("ADDR:COUNT")?, ':')?),
            "--trace" => {
                let range = pair(&val("LO:HI")?, ':')?;
                o.trace = Some((range, val("FILE")?));
            }
            _ => o.image = a.clone(),
        }
    }
    if o.image.is_empty() {
        return Err("usage: sharc-chain IMAGE [options]; see the module docs".to_string());
    }
    o.pokes.sort_by_key(|p| p.0);
    Ok(o)
}

/// Like `call_once`, appending one register line per executed instruction with pc in `range`.
fn traced_call(
    core: &mut Core,
    s: &Step,
    range: (u32, u32),
    text: &mut String,
) -> Result<u64, String> {
    core.pc = s.entry;
    let start = core.steps;
    while core.pc != s.ret {
        if core.steps - start > STEP_LIMIT {
            return Err(format!("no return (pc 0x{:x})", core.pc));
        }
        let pc = core.pc;
        core.step()
            .map_err(|f| format!("fault at pc 0x{:x}: {f:?}", core.pc))?;
        if (range.0..range.1).contains(&pc) {
            text.push_str(&format!("{pc:x}"));
            for bank in [&core.r, &core.s, &core.i, &core.m] {
                for v in bank {
                    text.push_str(&format!(" {v:x}"));
                }
            }
            text.push('\n');
        }
    }
    Ok(core.steps - start)
}

fn run(args: &[String]) -> Result<String, String> {
    let o = parse_args(args)?;
    let data = std::fs::read(&o.image).map_err(|e| format!("{}: {e}", o.image))?;
    let mut core = boot_pno(&data, 0x121007, o.boot, o.mode1)?;
    let (mut next, mut bytes, mut steps) = (0, Vec::new(), 0);
    let mut trace_text = String::new();
    for n in 0..o.calls {
        while next < o.pokes.len() && o.pokes[next].0 <= n {
            let (_, addr, value) = o.pokes[next];
            core.dm_write(addr, value)
                .map_err(|a| format!("bad poke 0x{a:x}"))?;
            next += 1;
        }
        steps = 0;
        for s in &o.steps {
            for &(code, v) in &s.regs {
                core.write_ureg(code, v);
            }
            match (&o.trace, n) {
                (Some(((lo, hi), _)), 0) => {
                    steps += traced_call(&mut core, s, (*lo, *hi), &mut trace_text)?
                }
                _ => steps += call_once(&mut core, s.entry, s.ret, STEP_LIMIT)?,
            }
        }
        for &(base, count) in &o.ranges {
            for k in 0..count {
                let w = core.dm_read(base + k).unwrap_or(0);
                bytes.extend_from_slice(&w.to_le_bytes());
            }
        }
    }
    if let Some((_, path)) = &o.trace {
        std::fs::write(path, &trace_text).map_err(|e| format!("{path}: {e}"))?;
    }
    if let Some(path) = &o.out {
        std::fs::write(path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    }
    let mut text = format!("calls {}, steps per call {steps}\n", o.calls);
    for &(addr, count) in &o.dumps {
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

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn steps_parse_with_presets() {
        let s = parse_step("0x12272D:0x1229B2:I0=0xB0200,I1=7").unwrap();
        assert_eq!((s.entry, s.ret), (0x12272D, 0x1229B2));
        assert_eq!(s.regs, vec![(0x10, 0xB0200), (0x11, 7)]);
        assert!(parse_step("1:2").unwrap().regs.is_empty());
        assert!(parse_step("1").is_err());
        assert!(parse_step("1:2:Q9=1").is_err());
    }

    #[test]
    fn options_parse() {
        let o = parse_args(&strings(&[
            "img", "--calls", "3", "--step", "1:2", "--step", "3:4:I0=5", "--pre", "0x10=7",
            "--range", "0x20:4",
        ]))
        .unwrap();
        assert_eq!((o.calls, o.steps.len()), (3, 2));
        assert_eq!(o.pokes, vec![(0, 0x10, 7)]);
        assert_eq!(o.ranges, vec![(0x20, 4)]);
        assert!(parse_args(&strings(&["--calls", "1"])).is_err());
    }

    #[test]
    fn a_missing_image_is_an_error() {
        assert!(run(&strings(&["/nonexistent/image.bin"])).is_err());
    }
}

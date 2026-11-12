//! `sharc-fx IMAGE [options]` calls one routine of the FX DSP image in the SHARC oracle and prints
//! the data words it left behind. It is the routine-level harness for `docs/research/fx-dsp-map.md`.
//!
//! The FX sections do not produce audio on their own (every effect slot is a null stub until the
//! ARM installs a routine), so a routine is verified by calling it directly: boot the section,
//! optionally run one pass up to a point that has set `MODE1`, the modify registers and the frame
//! pointers (`--trigger` / `--stop-at`), preset registers and data, jump to the routine and run to
//! a stop address inside it (its `RTS`, or the instruction before a nested `CALL`).
//!
//! ```text
//! --entry PC          section entry (default 0x1211E3, FX Core1 and Core2)
//! --boot N            boot instructions (default 100000)
//! --dev ADDR=V        scripted device read       --flag N=L   flag input level
//! --trigger ADDR --stop-at PC   write 1 to ADDR, run until PC is reached (pass context)
//! --call PC --until PC          routine to call and the pc at which it is considered finished
//! --calls N           number of calls (default 1); registers and pokes are re-applied before each
//! --set REG=V         R0-15, S0-15, I0-15, M0-15, L0-15, B0-15 before every call
//! --poke [N:]ADDR=V   data word before call N (default 0); V may be a float as f0.5
//! --fill [N:]ADDR=V,V,...   consecutive data words before call N
//! --dump ADDR:COUNT   print `dump ADDR hex float` rows after every call
//! --max N             instruction limit per phase (default 1000000)
//! ```
use nord_sharc::core::{Core, Fault};
use std::fmt::Write as _;
use std::process::ExitCode;

fn num(s: &str) -> Result<u32, String> {
    let r = match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16),
        None => s.parse(),
    };
    r.map_err(|_| format!("bad number {s:?}"))
}

/// A data value: number, `0x` hex or `f<float>` (float bit pattern).
fn value(s: &str) -> Result<u32, String> {
    match s.strip_prefix('f') {
        Some(f) => f
            .parse::<f32>()
            .map(f32::to_bits)
            .map_err(|_| format!("bad float {s:?}")),
        None => num(s),
    }
}

/// `[N:]ADDR=REST` into (N, ADDR, REST).
fn split_target(s: &str) -> Result<(u32, u32, &str), String> {
    let bad = || format!("bad [N:]ADDR=V {s:?}");
    let (lhs, rest) = s.split_once('=').ok_or_else(bad)?;
    let (n, addr) = match lhs.split_once(':') {
        Some((n, a)) => (num(n)?, num(a)?),
        None => (0, num(lhs)?),
    };
    Ok((n, addr, rest))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Reg {
    R(usize),
    S(usize),
    I(usize),
    M(usize),
    L(usize),
    B(usize),
}

fn parse_reg(name: &str) -> Result<Reg, String> {
    let bad = || format!("bad register {name:?} (R S I M L B 0-15)");
    let n: usize = name
        .get(1..)
        .and_then(|t| t.parse().ok())
        .filter(|n| *n < 16)
        .ok_or_else(bad)?;
    Ok(match name.get(..1) {
        Some("R") => Reg::R(n),
        Some("S") => Reg::S(n),
        Some("I") => Reg::I(n),
        Some("M") => Reg::M(n),
        Some("L") => Reg::L(n),
        Some("B") => Reg::B(n),
        _ => return Err(bad()),
    })
}

fn apply(core: &mut Core, reg: Reg, v: u32) {
    match reg {
        Reg::R(n) => core.r[n] = v,
        Reg::S(n) => core.s[n] = v,
        Reg::I(n) => core.i[n] = v,
        Reg::M(n) => core.m[n] = v,
        Reg::L(n) => core.l[n] = v,
        Reg::B(n) => core.b[n] = v,
    }
}

#[derive(Default)]
struct Options {
    image: String,
    entry: u32,
    boot: u64,
    max: u64,
    calls: u32,
    call: Option<u32>,
    until: Option<u32>,
    trigger: Option<u32>,
    stop_at: Option<u32>,
    devs: Vec<(u32, u32)>,
    flags: Vec<(u8, bool)>,
    sets: Vec<(Reg, u32)>,
    /// (call index, address, value)
    pokes: Vec<(u32, u32, u32)>,
    dumps: Vec<(u32, u32)>,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        entry: 0x12_11E3,
        boot: 100_000,
        max: 1_000_000,
        calls: 1,
        ..Options::default()
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut need = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--entry" => o.entry = num(&need()?)?,
            "--boot" => o.boot = u64::from(num(&need()?)?),
            "--max" => o.max = u64::from(num(&need()?)?),
            "--calls" => o.calls = num(&need()?)?,
            "--call" => o.call = Some(num(&need()?)?),
            "--until" => o.until = Some(num(&need()?)?),
            "--trigger" => o.trigger = Some(num(&need()?)?),
            "--stop-at" => o.stop_at = Some(num(&need()?)?),
            "--dev" => {
                let s = need()?;
                let (_, a, v) = split_target(&s)?;
                o.devs.push((a, value(v)?));
            }
            "--flag" => {
                let s = need()?;
                let (n, l) = s.split_once('=').ok_or("--flag needs N=LEVEL")?;
                o.flags.push((num(n)? as u8, num(l)? != 0));
            }
            "--set" => {
                let s = need()?;
                let (n, v) = s.split_once('=').ok_or("--set needs REG=V")?;
                o.sets.push((parse_reg(n)?, value(v)?));
            }
            "--poke" => {
                let s = need()?;
                let (n, addr, v) = split_target(&s)?;
                o.pokes.push((n, addr, value(v)?));
            }
            "--fill" => {
                let s = need()?;
                let (n, addr, list) = split_target(&s)?;
                for (k, v) in list.split(',').enumerate() {
                    o.pokes.push((n, addr + k as u32, value(v)?));
                }
            }
            "--dump" => {
                let s = need()?;
                let (addr, n) = s.split_once(':').ok_or("--dump needs ADDR:COUNT")?;
                o.dumps.push((num(addr)?, num(n)?));
            }
            _ if o.image.is_empty() && !a.starts_with("--") => o.image = a.clone(),
            _ => return Err(format!("unknown argument {a:?}")),
        }
    }
    if o.image.is_empty() {
        return Err("usage: sharc-fx IMAGE [--call PC --until PC] [options]".into());
    }
    if o.trigger.is_some() != o.stop_at.is_some() {
        return Err("--trigger and --stop-at go together".into());
    }
    if o.call.is_some() != o.until.is_some() {
        return Err("--call and --until go together".into());
    }
    Ok(o)
}

fn fault_text(f: &Fault) -> String {
    match f {
        Fault::Unimplemented { pc, parcels } => {
            format!("unimplemented at 0x{pc:x} (parcels {parcels:04x?})")
        }
        Fault::BadAddress { pc, address } => format!("bad address 0x{address:x} at pc 0x{pc:x}"),
    }
}

/// Steps until `pc` equals `stop` (before executing it) or `max` steps passed.
fn run_to(core: &mut Core, stop: u32, max: u64) -> Result<u64, String> {
    let start = core.steps;
    while core.pc != stop {
        if core.steps - start >= max {
            return Err(format!(
                "step limit {max} before pc 0x{stop:x}, at 0x{:x}",
                core.pc
            ));
        }
        core.step().map_err(|f| fault_text(&f))?;
    }
    Ok(core.steps - start)
}

fn run(args: &[String]) -> Result<String, String> {
    let o = parse_args(args)?;
    let data = std::fs::read(&o.image).map_err(|e| format!("{}: {e}", o.image))?;
    let mut core = Core::new(&data);
    core.pc = o.entry;
    for &(n, high) in &o.flags {
        core.set_flag_input(n, high);
    }
    for &(a, v) in &o.devs {
        core.preset_device(a, v);
    }
    let mut out = String::new();
    if let Some(f) = core.run(o.boot) {
        return Err(format!("boot faulted: {}", fault_text(&f)));
    }
    let _ = writeln!(out, "booted {} steps, pc 0x{:x}", core.steps, core.pc);
    if let (Some(t), Some(stop)) = (o.trigger, o.stop_at) {
        core.dm_write(t, 1)
            .map_err(|a| format!("trigger address 0x{a:x}"))?;
        let n = run_to(&mut core, stop, o.max)?;
        let _ = writeln!(out, "pass context: {n} steps to pc 0x{stop:x}");
    }
    let (Some(call), Some(until)) = (o.call, o.until) else {
        return Ok(out);
    };
    for k in 0..o.calls {
        for &(n, a, v) in o.pokes.iter().filter(|p| p.0 == k) {
            let _ = n;
            core.dm_write(a, v)
                .map_err(|a| format!("poke address 0x{a:x}"))?;
        }
        for &(r, v) in &o.sets {
            apply(&mut core, r, v);
        }
        core.pc = call;
        let n = run_to(&mut core, until, o.max)?;
        let _ = writeln!(out, "call {k}: {n} steps, pc 0x{:x}", core.pc);
        for &(addr, count) in &o.dumps {
            for a in addr..addr + count {
                let w = core.dm_read(a).unwrap_or(0);
                let _ = writeln!(out, "dump {a:x} {w:08x} {:e}", f32::from_bits(w));
            }
        }
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(t) => {
            print!("{t}");
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

    /// Image with `DM(0xB0000 + k)` untouched and code at short-word 0x120000:
    /// `R0 = 5; R1 = R0 + R0; RTS` is not needed, the harness only needs two NOPs and a stop pc.
    fn tiny_image() -> String {
        let path = std::env::temp_dir().join("nord_sharc_fx_test.bin");
        let mut image = vec![0u8; 0x24_0000 + 64];
        for k in 0..8 {
            image[0x24_0000 + 2 * k..0x24_0000 + 2 * k + 2].copy_from_slice(&1u16.to_le_bytes());
        }
        std::fs::write(&path, &image).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn numbers_values_and_registers_parse() {
        assert_eq!(num("0x10"), Ok(16));
        assert_eq!(num("7"), Ok(7));
        assert!(num("zz").is_err());
        assert_eq!(value("f1.0"), Ok(0x3F80_0000));
        assert_eq!(value("0xff"), Ok(255));
        assert!(value("fxx").is_err());
        assert_eq!(parse_reg("I9"), Ok(Reg::I(9)));
        assert_eq!(parse_reg("S15"), Ok(Reg::S(15)));
        assert!(parse_reg("R16").is_err());
        assert!(parse_reg("Q1").is_err());
        assert_eq!(split_target("2:0x10=7").unwrap(), (2, 16, "7"));
        assert_eq!(split_target("0x10=7").unwrap(), (0, 16, "7"));
        assert!(split_target("nothing").is_err());
    }

    #[test]
    fn argument_rules_are_enforced() {
        assert!(parse_args(&strings(&[])).is_err());
        assert!(parse_args(&strings(&["img", "--call", "0x10"])).is_err());
        assert!(parse_args(&strings(&["img", "--trigger", "0x10"])).is_err());
        assert!(parse_args(&strings(&["img", "--bogus"])).is_err());
        let o = parse_args(&strings(&[
            "img",
            "--fill",
            "1:0x20=f1.0,f2.0",
            "--set",
            "R3=5",
            "--dump",
            "0x20:2",
        ]))
        .unwrap();
        assert_eq!(
            o.pokes,
            vec![(1, 0x20, 0x3F80_0000), (1, 0x21, 0x4000_0000)]
        );
        assert_eq!(o.sets, vec![(Reg::R(3), 5)]);
        assert_eq!(o.dumps, vec![(0x20, 2)]);
        let d = parse_args(&strings(&["img", "--dev", "0x3108d008=5", "--flag", "1=1"])).unwrap();
        assert_eq!(d.devs, vec![(0x3108_D008, 5)]);
        assert_eq!(d.flags, vec![(1, true)]);
    }

    #[test]
    fn a_routine_call_runs_to_its_stop_address_and_dumps_data() {
        let img = tiny_image();
        let text = run(&strings(&[
            &img,
            "--entry",
            "0x120000",
            "--boot",
            "1",
            "--call",
            "0x120000",
            "--until",
            "0x120004",
            "--poke",
            "0xb0000=f0.5",
            "--set",
            "R2=9",
            "--dump",
            "0xb0000:1",
        ]))
        .unwrap();
        assert!(text.contains("call 0: 4 steps, pc 0x120004"), "{text}");
        assert!(text.contains("dump b0000 3f000000"), "{text}");
        let err = run(&strings(&[
            &img, "--entry", "0x120000", "--boot", "1", "--call", "0x120000", "--until",
            "0x120006", "--max", "2",
        ]))
        .unwrap_err();
        assert!(err.contains("step limit"), "{err}");
    }

    #[test]
    fn a_pass_context_stops_at_the_requested_pc() {
        let img = tiny_image();
        let text = run(&strings(&[
            &img,
            "--entry",
            "0x120000",
            "--boot",
            "1",
            "--trigger",
            "0xb00e2",
            "--stop-at",
            "0x120005",
        ]))
        .unwrap();
        assert!(
            text.contains("pass context: 4 steps to pc 0x120005"),
            "{text}"
        );
    }
}

//! `sharc-run IMAGE ENTRY MAX_STEPS [--dev ADDR=VALUE]... [--poke STEP:ADDR=VALUE]... [--set REG=VALUE]... [--dump ADDR:COUNT]... [--watch ADDR] [--regs] [--lenient] [--trace N]`
//! runs a flat SRAM image from a short-word entry address, optionally scripting device registers,
//! writing data words at a given step (a host command) and printing the last N pcs.
//!
//! Prints the number of executed instructions, the final program counter and, when the run stops
//! early, the faulting instruction in the listing format of `sharc-disasm`. Use the images written
//! by `python tools/ns4_dsp_ldr.py IMG --sram DIR` and the section entry value (for example
//! `0x1211E3` for the FX image).
use nord_sharc::core::{Core, Fault};
use nord_sharc::visa::{classify, decode_text, Insn};
use std::process::ExitCode;

fn parse_number(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Universal register code of a name such as `R3`, `I8`, `M0`, `L5` or `B2`.
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

/// R0-R15, I0-I15 and the condition flags as text lines.
fn register_dump(core: &Core) -> String {
    let row = |name: &str, regs: &[u32]| {
        let words: Vec<String> = regs.iter().map(|v| format!("{v:x}")).collect();
        format!(
            "{name}: {}
",
            words.join(" ")
        )
    };
    let f = core.flags;
    format!(
        "{}{}{}flags AZ={} AN={} AV={} AC={} TF={}
",
        row("R", &core.r),
        row("I", &core.i),
        row("M", &core.m),
        u8::from(f.az),
        u8::from(f.an),
        u8::from(f.av),
        u8::from(f.ac),
        u8::from(f.tf)
    )
}

fn run(args: &[String]) -> Result<String, String> {
    let mut positional = Vec::new();
    let mut devices = Vec::new();
    let mut pokes: Vec<(u64, u32, u32)> = Vec::new();
    let mut registers: Vec<(u8, u32)> = Vec::new();
    let mut dumps: Vec<(u32, u32)> = Vec::new();
    let mut watch = u32::MAX;
    let mut show_regs = false;
    let mut trace = 0usize;
    let mut lenient = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dev" => {
                let spec = it.next().ok_or("--dev needs ADDR=VALUE")?;
                let (addr, value) = spec.split_once('=').ok_or("--dev needs ADDR=VALUE")?;
                devices.push((
                    parse_number(addr).ok_or("bad --dev address")? as u32,
                    parse_number(value).ok_or("bad --dev value")? as u32,
                ));
            }
            "--poke" => {
                let spec = it.next().ok_or("--poke needs STEP:ADDR=VALUE")?;
                let (step, rest) = spec.split_once(':').ok_or("--poke needs STEP:ADDR=VALUE")?;
                let (addr, value) = rest.split_once('=').ok_or("--poke needs STEP:ADDR=VALUE")?;
                pokes.push((
                    parse_number(step).ok_or("bad --poke step")?,
                    parse_number(addr).ok_or("bad --poke address")? as u32,
                    parse_number(value).ok_or("bad --poke value")? as u32,
                ));
            }
            "--lenient" => lenient = true,
            "--regs" => show_regs = true,
            "--watch" => {
                watch = parse_number(it.next().ok_or("--watch needs ADDR")?).ok_or("bad --watch")?
                    as u32;
            }
            "--dump" => {
                let spec = it.next().ok_or("--dump needs ADDR:COUNT")?;
                let (addr, count) = spec.split_once(':').ok_or("--dump needs ADDR:COUNT")?;
                dumps.push((
                    parse_number(addr).ok_or("bad --dump address")? as u32,
                    parse_number(count).ok_or("bad --dump count")? as u32,
                ));
            }
            "--set" => {
                let spec = it.next().ok_or("--set needs REG=VALUE")?;
                let (name, value) = spec.split_once('=').ok_or("--set needs REG=VALUE")?;
                registers.push((
                    register_code(name).ok_or("bad --set register (R I M L B 0-15)")?,
                    parse_number(value).ok_or("bad --set value")? as u32,
                ));
            }
            "--trace" => {
                trace = parse_number(it.next().ok_or("--trace needs N")?).ok_or("bad --trace")?
                    as usize;
            }
            _ => positional.push(a.clone()),
        }
    }
    let [image, entry, max_steps] = positional.as_slice() else {
        return Err(
            "usage: sharc-run IMAGE ENTRY MAX_STEPS [--dev A=V]... [--poke STEP:ADDR=VALUE]... [--set REG=VALUE]... [--dump ADDR:COUNT]... [--watch ADDR] [--regs] [--lenient] [--trace N]".to_string(),
        );
    };
    let data = std::fs::read(image).map_err(|e| format!("{image}: {e}"))?;
    let entry = parse_number(entry).ok_or("bad ENTRY")? as u32;
    let max_steps = parse_number(max_steps).ok_or("bad MAX_STEPS")?;
    let mut core = Core::new(&data);
    core.lenient = lenient;
    core.watch = watch;
    core.pc = entry;
    for (code, value) in registers {
        core.write_ureg(code, value);
    }
    for (addr, value) in devices {
        core.preset_device(addr, value);
    }
    pokes.sort_by_key(|p| p.0);
    let mut recent = std::collections::VecDeque::with_capacity(trace + 1);
    let mut fault = None;
    let mut next_poke = 0;
    while core.steps < max_steps && fault.is_none() {
        while next_poke < pokes.len() && pokes[next_poke].0 <= core.steps {
            let (_, addr, value) = pokes[next_poke];
            core.dm_write(addr, value)
                .map_err(|a| format!("bad poke address 0x{a:x}"))?;
            next_poke += 1;
        }
        if trace > 0 {
            recent.push_back(core.pc);
            if recent.len() > trace {
                recent.pop_front();
            }
        }
        // run in one go until the next poke step to keep the hot loop tight
        let until = pokes
            .get(next_poke)
            .map_or(max_steps, |p| p.0.min(max_steps));
        if trace == 0 {
            let n = until.saturating_sub(core.steps).max(1);
            fault = core.run(n);
        } else if let Err(f) = core.step() {
            fault = Some(f);
        }
    }
    let mut out = format!(
        "steps {} pc 0x{:x} device reads {} writes {}
",
        core.steps, core.pc, core.device_reads, core.device_writes
    );
    if core.skipped > 0 {
        out.push_str(&format!(
            "skipped {} unmodelled compute ops, first at 0x{:x}
",
            core.skipped,
            core.first_skipped.unwrap_or(0)
        ));
    }
    if show_regs {
        out.push_str(&register_dump(&core));
    }
    for (step, pc, value) in &core.watch_hits {
        out.push_str(&format!(
            "write DM(0x{:x}) = 0x{value:x} at pc 0x{pc:x}, step {step}
",
            core.watch
        ));
    }
    for (addr, count) in dumps {
        let words: Vec<String> = (0..count)
            .map(|k| format!("{:x}", core.dm_read(addr.wrapping_add(k)).unwrap_or(0)))
            .collect();
        out.push_str(&format!(
            "DM(0x{addr:x}..): {}
",
            words.join(" ")
        ));
    }
    if !core.return_addresses().is_empty() {
        let stack: Vec<String> = core
            .return_addresses()
            .iter()
            .map(|a| format!("{a:x}"))
            .collect();
        out.push_str(&format!(
            "call stack (outermost first): {}
",
            stack.join(" ")
        ));
    }
    if trace > 0 {
        let list: Vec<String> = recent.iter().map(|p| format!("{p:x}")).collect();
        out.push_str(&format!(
            "last pcs: {}
",
            list.join(" ")
        ));
    }
    match fault {
        None => out.push_str("stopped at the step limit"),
        Some(Fault::Unimplemented { pc, parcels }) => {
            let (kind, len) = classify(parcels[0], Some(parcels[1]));
            let insn = Insn { kind, parcels, len };
            out.push_str(&format!(
                "unimplemented at 0x{pc:x}: {} {}",
                kind.name(),
                decode_text(&insn)
            ));
        }
        Some(Fault::BadAddress { pc, address }) => {
            out.push_str(&format!("bad data address 0x{address:x} at pc 0x{pc:x}"));
        }
    }
    Ok(out)
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
    fn runs_a_tiny_image_and_reports_the_stop_reason() {
        let path = std::env::temp_dir().join("nord_sharc_run_test.bin");
        let mut image = vec![0u8; 0x24_0000 + 16];
        // at short-word 0x120000 (byte 0x240000): NOP, then an unimplemented Type 6a word
        for (k, p) in [0x0001u16, 0x8000, 0x0000, 0x0000].iter().enumerate() {
            image[0x24_0000 + 2 * k..0x24_0000 + 2 * k + 2].copy_from_slice(&p.to_le_bytes());
        }
        std::fs::write(&path, &image).unwrap();
        let p = path.to_string_lossy().to_string();
        let text = run(&[p.clone(), "0x120000".into(), "10".into()]).unwrap();
        assert!(text.contains("steps 1 pc 0x120001"), "{text}");
        assert!(text.contains("unimplemented at 0x120001"), "{text}");
        let limited = run(&[p.clone(), "0x120000".into(), "1".into()]).unwrap();
        assert!(limited.contains("stopped at the step limit"));
        assert!(run(&[p.clone(), "zz".into(), "1".into()]).is_err());
        assert!(run(std::slice::from_ref(&p)).is_err());
        let traced = run(&[
            p.clone(),
            "0x120000".into(),
            "10".into(),
            "--trace".into(),
            "3".into(),
        ])
        .unwrap();
        assert!(traced.contains("last pcs: 120000 120001"), "{traced}");
        let dev = run(&[
            p.clone(),
            "0x120000".into(),
            "1".into(),
            "--dev".into(),
            "0x3108d008=5".into(),
        ])
        .unwrap();
        assert!(dev.contains("device reads 0"));
        assert!(run(&[
            p.clone(),
            "0".into(),
            "1".into(),
            "--dev".into(),
            "oops".into()
        ])
        .is_err());
        assert!(run(&[p.clone(), "0".into(), "1".into(), "--trace".into()]).is_err());
        let poked = run(&[
            p.clone(),
            "0x120000".into(),
            "3".into(),
            "--poke".into(),
            "1:0xb0000=7".into(),
        ])
        .unwrap();
        assert!(poked.contains("steps 1 pc"), "{poked}");
        assert_eq!(register_code("I8"), Some(0x18));
        assert_eq!(register_code("R15"), Some(0x0F));
        assert_eq!(register_code("X1"), None);
        assert_eq!(register_code("R16"), None);
        let set = run(&[
            p.clone(),
            "0x120000".into(),
            "1".into(),
            "--set".into(),
            "R3=7".into(),
        ])
        .unwrap();
        assert!(set.contains("steps 1"), "{set}");
        assert!(run(&[
            p.clone(),
            "0".into(),
            "1".into(),
            "--set".into(),
            "Q1=2".into()
        ])
        .is_err());
        let len = run(&[p.clone(), "0x120000".into(), "1".into(), "--lenient".into()]).unwrap();
        assert!(len.contains("steps 1"), "{len}");
        assert!(run(&[p, "0".into(), "1".into(), "--poke".into(), "x".into()]).is_err());
        std::fs::remove_file(path).ok();
    }
}

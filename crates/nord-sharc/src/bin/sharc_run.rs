//! `sharc-run IMAGE ENTRY MAX_STEPS [--dev ADDR=VALUE]... [--poke STEP:ADDR=VALUE]... [--irq N@STEP]... [--flag N=LEVEL]... [--set REG=VALUE]... [--dump ADDR:COUNT]... [--watch ADDR] [--regs] [--lenient] [--skips] [--isa] [--enter STEP:PC]... [--trace N]`
//! runs a flat SRAM image from a short-word entry address, optionally scripting device registers,
//! writing data words at a given step (a host command) and printing the last N pcs.
//!
//! Prints the number of executed instructions, the final program counter and, when the run stops
//! early, the faulting instruction in the listing format of `sharc-disasm`. Use the images written
//! by `python tools/ns4_dsp_ldr.py IMG --sram DIR` and the section entry value (for example
//! `0x1211E3` for the FX image).
use nord_sharc::core::{vector_by_name, Core, Fault};
use nord_sharc::visa::{classify, classify_isa, decode_text, Insn};
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

/// The skipped operations of a lenient run, grouped by instruction text, most frequent first.
fn skip_histogram(core: &Core) -> String {
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for &pc in &core.skipped_pcs {
        let (kind, parcels, len) = match core.isa_word(pc) {
            Some(w) => {
                let parcels = [(w >> 32) as u16, (w >> 16) as u16, w as u16];
                (classify_isa(parcels[0]), parcels, 3)
            }
            None => {
                let (kind, len) = classify(core.parcel_at(pc), Some(core.parcel_at(pc + 1)));
                let mut parcels = [0u16; 3];
                for (k, p) in parcels.iter_mut().enumerate().take(len.min(3)) {
                    *p = core.parcel_at(pc + k as u32);
                }
                (kind, parcels, len)
            }
        };
        let text = decode_text(&Insn { kind, parcels, len });
        *counts.entry(format!("{} {text}", kind.name())).or_insert(0) += 1;
    }
    let mut rows: Vec<_> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut out = String::from(
        "skipped operations by text:
",
    );
    for (text, n) in rows.iter().take(40) {
        out.push_str(&format!(
            "{n:>7}  {text}
"
        ));
    }
    out
}

fn run(args: &[String]) -> Result<String, String> {
    let mut positional = Vec::new();
    let mut devices = Vec::new();
    let mut pokes: Vec<(u64, u32, u32)> = Vec::new();
    let mut irqs: Vec<(u64, u8)> = Vec::new();
    let mut flags: Vec<(u8, bool)> = Vec::new();
    let mut registers: Vec<(u8, u32)> = Vec::new();
    let mut dumps: Vec<(u32, u32)> = Vec::new();
    let mut watch = u32::MAX;
    let mut show_regs = false;
    let mut histogram = false;
    let mut trace = 0usize;
    let mut lenient = false;
    let mut isa = false;
    let mut enters: Vec<(u64, u32)> = Vec::new();
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
            "--irq" => {
                // N@STEP: raise interrupt vector N (number or name such as P4I) at core cycle STEP
                let spec = it.next().ok_or("--irq needs N@STEP")?;
                let (n, step) = spec.split_once('@').ok_or("--irq needs N@STEP")?;
                irqs.push((
                    parse_number(step).ok_or("bad --irq step")?,
                    vector_by_name(n).ok_or("bad --irq vector (0-41 or a name such as P4I)")?,
                ));
            }
            "--flag" => {
                // N=LEVEL: drive the FLAGn pin (an input flag reads this level)
                let spec = it.next().ok_or("--flag needs N=LEVEL")?;
                let (n, level) = spec.split_once('=').ok_or("--flag needs N=LEVEL")?;
                flags.push((
                    parse_number(n)
                        .filter(|n| *n < 16)
                        .ok_or("bad --flag number")? as u8,
                    parse_number(level).ok_or("bad --flag level")? != 0,
                ));
            }
            "--lenient" => lenient = true,
            "--isa" => isa = true,
            "--enter" => {
                // STEP:PC enters a handler body directly (no IRPTL/IMASK model); see `--irq`
                let spec = it.next().ok_or("--enter needs STEP:PC")?;
                let (step, pc) = spec.split_once(':').ok_or("--enter needs STEP:PC")?;
                enters.push((
                    parse_number(step).ok_or("bad --enter step")?,
                    parse_number(pc).ok_or("bad --enter pc")? as u32,
                ));
            }
            "--skips" => histogram = true,
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
            "usage: sharc-run IMAGE ENTRY MAX_STEPS [--dev A=V]... [--poke STEP:ADDR=VALUE]... [--irq N@STEP]... [--flag N=LEVEL]... [--set REG=VALUE]... [--dump ADDR:COUNT]... [--watch ADDR] [--regs] [--lenient] [--skips] [--isa] [--enter STEP:PC]... [--trace N]".to_string(),
        );
    };
    let data = std::fs::read(image).map_err(|e| format!("{image}: {e}"))?;
    let entry = parse_number(entry).ok_or("bad ENTRY")? as u32;
    let max_steps = parse_number(max_steps).ok_or("bad MAX_STEPS")?;
    let mut core = if isa {
        // raw ISA-mode image (SY1SY2/SMP): 48-bit words from 0x8C000, ENTRY is a word address
        let mut c = Core::new(&[]);
        c.load_isa(&data);
        c
    } else {
        Core::new(&data)
    };
    core.lenient = lenient;
    core.watch = watch;
    core.pc = entry;
    for (code, value) in registers {
        core.write_ureg(code, value);
    }
    for (addr, value) in devices {
        core.preset_device(addr, value);
    }
    for (n, high) in flags {
        core.set_flag_input(n, high);
    }
    for (step, vector) in irqs {
        core.schedule_interrupt(step, vector);
    }
    pokes.sort_by_key(|p| p.0);
    enters.sort_by_key(|p| p.0);
    let mut next_enter = 0;
    let mut recent = std::collections::VecDeque::with_capacity(trace + 1);
    let mut fault = None;
    let mut next_poke = 0;
    while core.steps < max_steps && fault.is_none() {
        if core.is_idle() && !core.can_wake() && next_poke >= pokes.len() {
            break; // IDLE with nothing left that could end it
        }
        while next_poke < pokes.len() && pokes[next_poke].0 <= core.steps {
            let (_, addr, value) = pokes[next_poke];
            core.dm_write(addr, value)
                .map_err(|a| format!("bad poke address 0x{a:x}"))?;
            next_poke += 1;
        }
        while next_enter < enters.len() && enters[next_enter].0 <= core.steps {
            core.enter_handler(enters[next_enter].1);
            next_enter += 1;
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
            .map_or(max_steps, |p| p.0.min(max_steps))
            .min(enters.get(next_enter).map_or(max_steps, |q| q.0));
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
    if core.interrupts_taken() > 0 || core.is_idle() {
        out.push_str(&format!(
            "interrupts taken {}, cycles {}{}
",
            core.interrupts_taken(),
            core.cycles(),
            if core.is_idle() {
                if core.can_wake() {
                    ", sleeping in IDLE"
                } else {
                    ", halted in IDLE (nothing can wake it)"
                }
            } else {
                ""
            }
        ));
    }
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
    if histogram {
        out.push_str(&skip_histogram(&core));
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
            let (kind, len) = if isa {
                (classify_isa(parcels[0]), 3)
            } else {
                classify(parcels[0], Some(parcels[1]))
            };
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
        // at short-word 0x120000 (byte 0x240000): NOP, then an unimplemented Type 10a word
        for (k, p) in [0x0001u16, 0xd70a, 0x3c23, 0xd70a].iter().enumerate() {
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
        // --enter goes into a handler body after the given step
        let irq = run(&[
            p.clone(),
            "0x120000".into(),
            "5".into(),
            "--enter".into(),
            "1:0x120000".into(),
        ])
        .unwrap();
        assert!(irq.contains("steps 2 pc 0x120001"), "{irq}");
        assert!(
            irq.contains("call stack (outermost first): 120001"),
            "{irq}"
        );
        assert!(run(&[
            p.clone(),
            "0".into(),
            "1".into(),
            "--enter".into(),
            "x".into()
        ])
        .is_err());
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
        std::fs::remove_file(&path).ok();
        // a lenient run lists the skipped operation: an MR3 transfer (compute 0x403000) does not exist
        let skip_path = std::env::temp_dir().join("nord_sharc_run_skip_test.bin");
        let mut image = vec![0u8; 0x24_0000 + 16];
        for (k, p) in [0x013eu16, 0x0040, 0x3000, 0x0001].iter().enumerate() {
            image[0x24_0000 + 2 * k..0x24_0000 + 2 * k + 2].copy_from_slice(&p.to_le_bytes());
        }
        std::fs::write(&skip_path, &image).unwrap();
        let sp = skip_path.to_string_lossy().to_string();
        let skipped = run(&[
            sp.clone(),
            "0x120000".into(),
            "1".into(),
            "--lenient".into(),
            "--skips".into(),
        ])
        .unwrap();
        assert!(skipped.contains("skipped operations by text:"), "{skipped}");
        assert!(skipped.contains("MULTI?op=03"), "{skipped}");
        std::fs::remove_file(skip_path).ok();
        // ISA mode: NOP, then a Type 10a word (jump + dual move, not executed) as 48-bit words
        let isa_path = std::env::temp_dir().join("nord_sharc_run_isa_test.bin");
        let mut image = Vec::new();
        for w in [[0u16; 3], [0xC13E, 0x0000, 0x0000]] {
            for p in [w[2], w[1], w[0]] {
                image.extend_from_slice(&p.to_le_bytes());
            }
        }
        std::fs::write(&isa_path, &image).unwrap();
        let ip = isa_path.to_string_lossy().to_string();
        let text = run(&[ip, "0x8c000".into(), "10".into(), "--isa".into()]).unwrap();
        assert!(text.contains("steps 1 pc 0x8c001"), "{text}");
        assert!(text.contains("unimplemented at 0x8c001: 10a"), "{text}");
        std::fs::remove_file(isa_path).ok();
    }

    #[test]
    fn irq_flag_and_idle_options() {
        let path = std::env::temp_dir().join("nord_sharc_run_idle_test.bin");
        let mut image = vec![0u8; 0x24_0000 + 0x400];
        // IDLE (16-bit 0x0091) at short-word 0x120100
        image[0x24_0200..0x24_0202].copy_from_slice(&0x0091u16.to_le_bytes());
        std::fs::write(&path, &image).unwrap();
        let p = path.to_string_lossy().to_string();
        let args = |extra: &[&str]| {
            let mut v = vec![p.clone(), "0x120100".into(), "5000".into()];
            v.extend(extra.iter().map(|s| s.to_string()));
            v
        };
        // an interrupt that is masked cannot end the IDLE: the run reports it and stops early
        let text = run(&args(&["--irq", "P0I@500", "--flag", "1=1"])).unwrap();
        assert!(text.contains("halted in IDLE"), "{text}");
        assert!(text.contains("interrupts taken 0"), "{text}");
        // while an interrupt is still scheduled the core sleeps and the run reaches the limit
        let mut short = args(&["--irq", "P0I@900000"]);
        short[2] = "2".into();
        let text = run(&short).unwrap();
        assert!(text.contains("sleeping in IDLE"), "{text}");
        assert!(run(&args(&["--irq", "99@5"])).is_err());
        assert!(run(&args(&["--irq", "P0I"])).is_err());
        assert!(run(&args(&["--flag", "16=1"])).is_err());
    }
}

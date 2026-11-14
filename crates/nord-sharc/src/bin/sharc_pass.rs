//! `sharc-pass IMAGE [options]` runs the complete PNO core 1 pass in the SHARC oracle: boot the
//! image to its poll loop (entry `0x121007`, board gates preset), apply ARM message scripts
//! (call table installs, parameter blocks, bank increments, weights), start every pass with the
//! P4I receive interrupt exactly like the hardware (the ISR stores the status in the mailbox
//! `DM(0xB00E1)`, the loop runs the 8 slot calls, the 18 module chain and the output stage), and
//! dump the output lanes. One pass renders 2 frames of a stereo pair, stored `[L0 R0 L1 R1]` per
//! pair (SIMD register pairs, `docs/research/pno-full-pass.md`).
//!
//! ```text
//! --boot N               start-up instructions (default 3000000)    --passes N   (default 1)
//! --script FILE[@PASS]   lines `PASS ADDR VALUE` (absolute DM words, `#` comments), PASS + offset
//! --msg [PASS:]ADDR=V    one message                                --poke PASS:ADDR=V  (V may be fFLOAT)
//! --lane SET:PAIR        output pair that goes to the WAV: pointer DM(0xB0159 + SET), pair 0..2
//! --wav FILE --rate R    stereo float WAV (2 frames per pass; default 48000 Hz)
//! --raw FILE             every pass: the 6 pointer sets x 3 pairs x 4 words as little-endian u32
//! --watch LO:LEN         list the store sites (pc, passes, address span) inside the window
//! --dump ADDR:COUNT      print data words after the last pass      --calls  list the call targets
//! --no-ping-pong         keep DM(0xB0158) (default: toggled before every pass like the P0I ISR)
//! --quiet                no per-message and per-pass lines
//! ```
//! Used with `python tools/arm_pass_script.py` scripts; see `docs/research/pno-full-pass.md`.
#[path = "sharc_pass/args.rs"]
mod args;
#[path = "sharc_pass/report.rs"]
mod report;

use args::{parse_args, Options, GATES, IDLE_PC, MAILBOX, OUT_POINTERS};
use nord_sharc::core::Core;
use nord_sharc::runner::audio::wav_bytes;
use nord_sharc::runner::msg::deliver;
use nord_sharc::runner::pass::{fault_text, run_pass, run_steps, store_sites, PassConfig, PassEnd};
use report::{lane_words, Stats, Writers};
use std::fmt::Write as _;
use std::process::ExitCode;

/// Pointer word the pass compares with `0x28AC01E0` to pick the buffer set (`pno-dsp-map.md` s3).
const PING_PONG: u32 = 0xB_0158;
const PING: u32 = 0x28AC_01E0;
const PONG: u32 = 0x28AC_0000;
/// P4I vector and the non-zero SEC status the ISR copies to the mailbox.
const P4I: u8 = 15;

fn boot_core(o: &Options) -> Result<Core, String> {
    let data = std::fs::read(&o.image).map_err(|e| format!("{}: {e}", o.image))?;
    let mut core = Core::new(&data);
    core.pc = args::ENTRY;
    for (a, v) in GATES {
        core.preset_device(a, v);
    }
    core.set_flag_input(1, true);
    if let Some((lo, len)) = o.watch {
        (core.watch, core.watch_len, core.watch_limit) = (lo, len, 1_000_000);
    }
    if let Some(f) = run_steps(&mut core, o.boot) {
        return Err(format!("boot faulted: {}", fault_text(&f)));
    }
    if core.pc != IDLE_PC && core.pc != IDLE_PC + 1 {
        return Err(format!(
            "boot ended at pc 0x{:x}, not in the poll loop",
            core.pc
        ));
    }
    core.watch_hits.clear();
    core.watch_addrs.clear();
    Ok(core)
}

/// Runs the script and returns the text report.
fn run(argv: &[String]) -> Result<String, String> {
    let o = parse_args(argv)?;
    let mut core = boot_core(&o)?;
    let cfg = PassConfig {
        trigger: MAILBOX,
        idle_pc: IDLE_PC,
        max_steps: o.max_steps,
        irq: Some((P4I, 1)),
    };
    let mut out = String::new();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut raw: Vec<u8> = Vec::new();
    let mut writers = Writers::default();
    let mut stats = Stats::default();
    let mut calls = std::collections::BTreeSet::new();
    for p in 0..o.passes {
        if !o.no_ping_pong {
            let v = if p % 2 == 0 { PING } else { PONG };
            core.dm_write(PING_PONG, v)
                .map_err(|a| format!("bad address 0x{a:x}"))?;
        }
        for &(_, a, v) in o.pokes.iter().filter(|q| q.0 == p) {
            core.dm_write(a, v)
                .map_err(|a| format!("bad --poke address 0x{a:x}"))?;
        }
        let mut n = 0;
        for m in o.msgs.iter().filter(|m| m.pass == p) {
            deliver(&mut core, 0, m)?;
            n += 1;
        }
        let r = run_pass(&mut core, &cfg);
        if !o.quiet {
            let end = match &r.end {
                PassEnd::Idle => "ok".to_string(),
                PassEnd::StepLimit => "step limit".to_string(),
                PassEnd::Fault(t) => format!("fault: {t}"),
            };
            let _ = writeln!(out, "pass {p}: {n} msgs, {} steps, {end}", r.steps);
        }
        if o.watch.is_some() {
            writers.add(p, &store_sites(&r.writes));
        }
        calls.extend(r.calls.iter().map(|c| c.0));
        for set in 0..6 {
            let w = lane_words(&mut core, OUT_POINTERS + set);
            if o.raw.is_some() {
                raw.extend(w.iter().flat_map(|x| x.to_le_bytes()));
            }
            if let Some(l) = o.lane.filter(|l| l.set == set) {
                let b = 4 * l.pair as usize;
                let f = |k: usize| f32::from_bits(w[b + k]);
                left.extend([f(0), f(2)]);
                right.extend([f(1), f(3)]);
            }
        }
        if !matches!(r.end, PassEnd::Idle) {
            let _ = writeln!(out, "stopped at pass {p}");
            break;
        }
    }
    if o.calls {
        let t: Vec<String> = calls.iter().map(|c| format!("{c:x}")).collect();
        let _ = writeln!(out, "call targets: {}", t.join(" "));
    }
    out.push_str(&writers.text());
    for (addr, n) in &o.dumps {
        let w: Vec<String> = (0..*n)
            .map(|k| format!("{:x}", core.dm_read(addr.wrapping_add(k)).unwrap_or(0)))
            .collect();
        let _ = writeln!(out, "DM(0x{addr:x}..): {}", w.join(" "));
    }
    if o.lane.is_some() {
        stats.add(&left);
        stats.add(&right);
        let _ = writeln!(out, "{}", stats.text(left.len()));
    }
    if let Some(f) = &o.wav {
        std::fs::write(f, wav_bytes(&[left, right], o.rate)).map_err(|e| format!("{f}: {e}"))?;
        let _ = writeln!(out, "wrote {f}");
    }
    if let Some(f) = &o.raw {
        std::fs::write(f, raw).map_err(|e| format!("{f}: {e}"))?;
        let _ = writeln!(out, "wrote {f}");
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            print!("{text}");
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

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_missing_image_and_a_bad_boot_are_errors() {
        assert!(run(&args(&["no_such_image.bin"])).is_err());
        // an all-zero image never reaches the poll loop
        let path = std::env::temp_dir().join("nord_sharc_pass_zero.bin");
        std::fs::write(&path, vec![0u8; 0x24_0000 + 64]).unwrap();
        let e = run(&args(&[&path.to_string_lossy(), "--boot", "10"]));
        assert!(e.is_err(), "{e:?}");
        std::fs::remove_file(path).ok();
    }

    /// Image of NOP parcels: from the entry the core walks to the poll-loop pc, so the boot check
    /// passes; a pass then wanders off through the NOPs and hits the step limit.
    fn nop_image(name: &str) -> String {
        let path = std::env::temp_dir().join(name);
        let mut image = vec![0u8; 0x24_0000 + 2 * 0x400];
        for k in 0..0x400 {
            image[0x24_0000 + 2 * k..0x24_0000 + 2 * k + 2].copy_from_slice(&1u16.to_le_bytes());
        }
        std::fs::write(&path, &image).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn a_synthetic_image_runs_a_pass_with_messages_pokes_lane_and_dumps() {
        let img = nop_image("nord_sharc_pass_nop.bin");
        let wav = std::env::temp_dir().join("nord_sharc_pass_nop.wav");
        let text = run(&args(&[
            &img,
            "--boot",
            "46",
            "--passes",
            "2",
            "--max-pass-steps",
            "20",
            "--msg",
            "0:0xB0010=7",
            "--poke",
            "0:0xB0011=f1.0",
            "--lane",
            "2:0",
            "--calls",
            "--watch",
            "0xB0000:0x100",
            "--dump",
            "0xB0010:2",
            "--wav",
            &wav.to_string_lossy(),
        ]))
        .unwrap();
        assert!(
            text.contains("pass 0: 1 msgs, 20 steps, step limit"),
            "{text}"
        );
        assert!(text.contains("stopped at pass 0"), "{text}");
        assert!(text.contains("DM(0xb0010..): 7 3f800000"), "{text}");
        assert!(text.contains("lane: 2 frames"), "{text}");
        assert!(std::fs::metadata(&wav).unwrap().len() > 44);
        std::fs::remove_file(img).ok();
        std::fs::remove_file(wav).ok();
    }

    #[test]
    fn bad_arguments_are_rejected() {
        assert!(run(&args(&[])).is_err());
        assert!(run(&args(&["x.bin", "--bogus"])).is_err());
    }
}

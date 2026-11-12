//! `sharc-org IMAGE [options]` drives the ORG Core2 section of the Nord Stage 4 DSP firmware in
//! the SHARC oracle: boot to the mailbox poll loop, then run N processing passes with scripted
//! data pokes between them, and dump the changed memory and the audio buffers.
//!
//! ```text
//! --entry PC          section entry (default 0x125C88, ORG Core2)
//! --boot N            boot instructions before the first pass (default 100000)
//! --trigger ADDR      mailbox word (default 0xB00E2)    --idle-pc PC  poll loop (default 0x125DDF)
//! --passes N          number of passes (default 1)      --max-pass-steps N  (default 200000)
//! --dev ADDR=V        scripted device read              --set REG=V   preset R/I/M/L/B register
//! --pre ADDR=V        data poke after boot, before pass 0
//! --poke PASS:ADDR=V  data poke right before pass PASS (0-based); V may be a float as f0.5
//! --lenient           skip unmodelled compute ops (exploration only, never for VERIFIED claims)
//! --diff [LO:HI]      list the data words each pass changed (optionally inside LO..HI)
//! --buf SPEC          capture channel `[@]ADDR:COUNT[:f32|q31[:STRIDE]]` after every pass
//! --wav FILE --rate R write the captured channels as a float WAV (default 48000 Hz)
//! --csv FILE          write pass,channel,index,address,raw,value rows
//! --watch LO:LEN[:LIMIT]  list the store sites (pc, count, address span) inside the window
//! --boot-stores      with --watch, also list the store sites of the boot phase
//! --coverage         list the program-counter ranges each pass executed
//! --dump ADDR:COUNT   print data words after the last pass
//! --msg [PASS:]W1=W2  ARM message before pass PASS: DM(msg-base + W1) = W2   (repeatable)
//! --msg-script FILE   messages from a file, lines `PASS W1 W2` (`#` comments)
//! --msg-base ADDR     base of word 1 (default 0xE0000, the channel strips)
//! --rx-irq VEC:ST     start every pass by raising VEC (P4I on Core1) with SEC source id ST
//! ```
//! Used with the images of `python tools/ns4_dsp_ldr.py IMG --sram DIR`; see
//! `docs/research/org-dsp-map.md`.
#[path = "sharc_org/args.rs"]
mod args;
#[path = "sharc_org/audio.rs"]
mod audio;
#[path = "sharc_org/msg.rs"]
mod msg;
#[path = "sharc_org/pass.rs"]
mod pass;

use args::parse_args;
use audio::{capture, wav_bytes};
use nord_sharc::core::Core;
use pass::{pc_ranges, run_pass, run_steps, runs, store_sites, PassEnd};
use std::fmt::Write as _;
use std::process::ExitCode;

/// Runs the script and returns the text report.
fn run(args: &[String]) -> Result<String, String> {
    let o = parse_args(args)?;
    let cfg = o.cfg.ok_or("no pass configuration")?;
    let data = std::fs::read(&o.image).map_err(|e| format!("{}: {e}", o.image))?;
    let mut core = Core::new(&data);
    core.lenient = o.lenient;
    core.pc = o.entry;
    for &(code, v) in &o.regs {
        core.write_ureg(code, v);
    }
    for &(n, high) in &o.flags {
        core.set_flag_input(n, high);
    }
    for &(a, v) in &o.devs {
        core.preset_device(a, v);
    }
    if let Some((lo, len, limit)) = o.watch {
        (core.watch, core.watch_len, core.watch_limit) = (lo, len, limit);
    }
    let mut out = String::new();
    if let Some(f) = run_steps(&mut core, o.boot) {
        return Err(format!("boot faulted: {}", pass::fault_text(&f)));
    }
    let _ = writeln!(out, "booted {} steps, pc 0x{:x}", core.steps, core.pc);
    if o.watch.is_some() && o.boot_stores {
        let boot: Vec<_> = core
            .watch_hits
            .iter()
            .zip(&core.watch_addrs)
            .map(|(&(_, pc, v), &a)| (pc, a, v))
            .collect();
        for (pc, n, lo, hi) in store_sites(&boot) {
            let _ = writeln!(
                out,
                "  boot store pc 0x{pc:x}: {n} stores, DM(0x{lo:x})..DM(0x{hi:x})"
            );
        }
    }
    for &(a, v) in &o.pre {
        core.dm_write(a, v)
            .map_err(|a| format!("bad --pre address 0x{a:x}"))?;
    }
    let mut channels: Vec<Vec<f32>> = vec![Vec::new(); o.bufs.len()];
    let mut csv = String::from("pass,channel,index,address,raw,value\n");
    for p in 0..o.passes {
        for &(_, a, v) in o.pokes.iter().filter(|q| q.0 == p) {
            core.dm_write(a, v)
                .map_err(|a| format!("bad --poke address 0x{a:x}"))?;
        }
        for m in o.msgs.iter().filter(|m| m.pass == p) {
            let a = msg::deliver(&mut core, o.msg_base, m)?;
            let _ = writeln!(
                out,
                "msg before pass {p}: 0x{:x}=0x{:x} -> DM(0x{a:x}) = 0x{:x}",
                m.word1, m.word2, m.word2
            );
        }
        let r = run_pass(&mut core, &cfg);
        let end = match &r.end {
            PassEnd::Idle => "back in the poll loop".to_string(),
            PassEnd::StepLimit => "step limit".to_string(),
            PassEnd::Fault(t) => format!("fault: {t}"),
        };
        let _ = writeln!(
            out,
            "pass {p}: {} steps, {end}, {} words changed, skipped ops {}",
            r.steps,
            r.changed.len(),
            core.skipped
        );
        if let Some((lo, hi)) = o.diff {
            let sel: Vec<_> = r
                .changed
                .iter()
                .copied()
                .filter(|c| c.0 >= lo && c.0 <= hi)
                .collect();
            for (addr, words) in runs(&sel) {
                let w: Vec<String> = words.iter().take(12).map(|v| format!("{v:x}")).collect();
                let _ = writeln!(out, "  DM(0x{addr:x}) +{}: {}", words.len(), w.join(" "));
            }
        }
        if o.coverage {
            let calls: Vec<String> = r
                .calls
                .iter()
                .map(|(t, ret)| format!("{t:x}<{ret:x}"))
                .collect();
            let _ = writeln!(out, "  calls (target<return): {}", calls.join(" "));
            let r: Vec<String> = pc_ranges(&r.visited)
                .iter()
                .map(|(a, b)| format!("{a:x}-{b:x}"))
                .collect();
            let _ = writeln!(out, "  executed pcs: {}", r.join(" "));
        }
        if o.watch.is_some() {
            for (pc, n, lo, hi) in store_sites(&r.writes) {
                let _ = writeln!(
                    out,
                    "  store pc 0x{pc:x}: {n} stores, DM(0x{lo:x})..DM(0x{hi:x})"
                );
            }
        }
        for (ch, spec) in o.bufs.iter().enumerate() {
            for (k, (addr, raw, v)) in capture(&mut core, spec).into_iter().enumerate() {
                channels[ch].push(v);
                let _ = writeln!(csv, "{p},{ch},{k},0x{addr:x},0x{raw:08x},{v}");
            }
        }
        if !matches!(r.end, PassEnd::Idle) {
            break;
        }
    }
    for (addr, n) in &o.dumps {
        let w: Vec<String> = (0..*n)
            .map(|k| format!("{:x}", core.dm_read(addr.wrapping_add(k)).unwrap_or(0)))
            .collect();
        let _ = writeln!(out, "DM(0x{addr:x}..): {}", w.join(" "));
    }
    if let Some(f) = &o.wav {
        std::fs::write(f, wav_bytes(&channels, o.rate)).map_err(|e| format!("{f}: {e}"))?;
        let _ = writeln!(out, "wrote {f}");
    }
    if let Some(f) = &o.csv {
        std::fs::write(f, csv).map_err(|e| format!("{f}: {e}"))?;
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

    /// Image whose code at 0x120000 is `DM(I0, M0) = 0` (Type 16b) followed by NOPs.
    fn tiny_image(name: &str) -> String {
        let path = std::env::temp_dir().join(name);
        let mut image = vec![0u8; 0x24_0000 + 32];
        for (k, p) in [0x9004u16, 0, 1, 1, 1].iter().enumerate() {
            image[0x24_0000 + 2 * k..0x24_0000 + 2 * k + 2].copy_from_slice(&p.to_le_bytes());
        }
        std::fs::write(&path, &image).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn a_scripted_run_pokes_diffs_and_writes_wav_and_csv() {
        let img = tiny_image("nord_sharc_org_test.bin");
        let wav = std::env::temp_dir().join("nord_sharc_org_test.wav");
        let csv = std::env::temp_dir().join("nord_sharc_org_test.csv");
        let text = run(&args(&[
            &img,
            "--entry",
            "0x120000",
            "--boot",
            "0",
            "--idle-pc",
            "0x120002",
            "--passes",
            "2",
            "--max-pass-steps",
            "10",
            "--set",
            "I0=0xB00E2",
            "--poke",
            "1:0xB0010=f1.0",
            "--diff",
            "--buf",
            "0xB0010:2",
            "--dump",
            "0xB0010:2",
            "--wav",
            &wav.to_string_lossy(),
            "--csv",
            &csv.to_string_lossy(),
        ]))
        .unwrap();
        assert!(
            text.contains("pass 0: 1 steps, back in the poll loop"),
            "{text}"
        );
        assert!(text.contains("pass 1:"), "{text}");
        assert!(text.contains("DM(0xb0010..): 3f800000 0"), "{text}");
        let w = std::fs::read(&wav).unwrap();
        assert_eq!(w.len(), 44 + 2 * 2 * 4); // two passes, two samples, one channel
        let rows = std::fs::read_to_string(&csv).unwrap();
        assert!(rows.contains("1,0,0,0xb0010,0x3f800000,1"), "{rows}");
        std::fs::remove_file(img).ok();
        std::fs::remove_file(wav).ok();
        std::fs::remove_file(csv).ok();
    }

    #[test]
    fn a_message_is_stored_before_its_pass_and_reported() {
        let img = tiny_image("nord_sharc_org_msg_test.bin");
        let text = run(&args(&[
            &img,
            "--entry",
            "0x120000",
            "--boot",
            "0",
            "--idle-pc",
            "0x120002",
            "--passes",
            "2",
            "--max-pass-steps",
            "10",
            "--set",
            "I0=0xB00E2",
            "--msg",
            "1:3=0x8012082f",
            "--msg-base",
            "0xB0010",
            "--dump",
            "0xB0010:5",
        ]))
        .unwrap();
        assert!(
            text.contains("msg before pass 1: 0x3=0x8012082f -> DM(0xb0013) = 0x8012082f"),
            "{text}"
        );
        assert!(text.contains("DM(0xb0010..): 0 0 0 8012082f 0"), "{text}");
        std::fs::remove_file(img).ok();
    }

    #[test]
    fn bad_arguments_are_rejected() {
        assert!(run(&args(&[])).is_err());
        assert!(run(&args(&["x.bin", "--bogus"])).is_err());
        assert!(run(&args(&["x.bin", "--passes"])).is_err());
        assert!(run(&args(&["no_such_file.bin"])).is_err());
    }
}

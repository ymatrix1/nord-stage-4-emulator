//! `sharc-sys IMAGE [options]`: the SHARC half of the minimal system run (D119). Boots ORG Core2,
//! delivers ARM message scripts between passes and reports the time of every component of a pass
//! interval, the steps per pass and a state comparison, all opt-in and interpreter only.
//!
//! ```text
//! --passes N / --warmup W   measured passes (default 300) after W warm-up passes (default 16)
//! --boot N                  boot instructions (default 100000)      --entry PC (0x125C88)
//! --pno                    PNO core 1 preset: entry 0x121007, poll loop 0x121091, mailbox 0xB00E1,
//!                           board gates, FLAG1 high, P4I:1 start, ping-pong toggle, boot 3000000
//! --trigger ADDR / --idle-pc PC   mailbox and poll loop (defaults 0xB00E2 / 0x125DDF)
//! --set In=V                address register preset (n = 0..15), for tiny test images
//! --msg [PASS:]W1=W2        message before PASS, W1 = DM word (--msg-base default 0)
//! --msg-script FILE         lines `PASS W1 W2`      --script-at OFFSET:FILE  same, PASS + OFFSET
//! --script-flat FILE        every message of FILE before pass 0, in file order
//! --proxy-voices K          poke proxy: stage-A slots 0..K get STAGE_SAW before pass 0
//! --irq VEC:STATUS          start each pass with that receive interrupt (P4I:0x41) instead of the mailbox
//! --flag1                   drive FLAG1 high (Core1 sections)
//! --dev ADDR=V              extra device read (0x282c0380=1 and 0x3108d008=5 are always set)
//! --probe PC                count the passes that execute PC (untimed second run of each pass)
//! --state-out FILE          write the final SRAM data words (little-endian u32, local only)
//! --state-cmp FILE          compare the final SRAM data words with a --state-out file
//! --pre ADDR=V              data poke after boot, before pass 0 (e.g. the EP DMA count stub)
//! --dump ADDR:COUNT         print data words after the last pass (same format as `sharc-org --dump`)
//! --frame-rate HZ           audio frame rate for the pass period (default 43402.34375)
//! --jit-diff                lockstep interpreter vs Cranelift JIT over the same passes and messages
//!                           (needs `--features jit`): full state + SRAM every --chunk steps
//!                           (default 1000), `--strict` adds the PEy/float/STKY/latch state
//! --epdma                   serve the DM(0x3E000..) range-transfer registers (replaces --pre 0x3e021=1)
//! --lane SET:PAIR --wav F   record output pointer set SET (0..5) pair PAIR (0..2) of every pass to a
//!                           float WAV (--rate R, default 48000; untimed extra run)
//! --sec-id ID               start Core1-style passes through the SEC model (CSID/END), source ID
//! --census                  list every register the boot and the passes touch outside SRAM
//! ```
//! Evidence: `docs/research/org-system-run.md`.
use nord_sharc::core::Core;
use nord_sharc::runner::audio::wav_bytes;
use nord_sharc::runner::msg::{deliver, parse_msg, parse_rx_irq, parse_script, Msg};
use nord_sharc::runner::pass::{fault_text, pc_ranges, run_steps, PassConfig, SRAM_WORDS};
use nord_sharc::runner::periph_run::{census_run, lane_run, PeriphOpts};
use nord_sharc::runner::system::{
    pass_period_s, probe_pass, region_hash, run_system, series, set_ping_pong, stats, PassTiming,
    FRAME_RATE, PNO_PING_PONG,
};
use nord_sharc::runner::{parse_number, parse_value};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::process::ExitCode;

/// STAGE_SAW routine pointer the poke proxy writes (`docs/research/sharc-jit-real-org-diff.md`).
const STAGE_SAW: u32 = 0x12_15FB;
/// First stage-A slot pointer and the slot stride.
const SLOT_BASE: u32 = 0xC_10B8;
const SLOT_STRIDE: u32 = 0x18;
/// Regions hashed per run: pass buffers, module state, channel strips.
const REGIONS: [(&str, u32, u32); 3] = [
    ("pass buffers 0xB0000..0xB2FFF", 0xB_0000, 0x3000),
    ("module state 0xC0000..0xC2FFF", 0xC_0000, 0x3000),
    ("strips 0xE0000..0xE02FF", 0xE_0000, 0x300),
];

#[derive(Default)]
struct Options {
    image: String,
    entry: u32,
    boot: u64,
    passes: u32,
    warmup: u32,
    msgs: Vec<Msg>,
    devs: Vec<(u32, u32)>,
    probe: Option<u32>,
    trigger: u32,
    idle_pc: u32,
    iregs: Vec<(usize, u32)>,
    state_out: Option<String>,
    state_cmp: Option<String>,
    frame_rate: f64,
    dumps: Vec<(u32, u32)>,
    pre: Vec<(u32, u32)>,
    irq: Option<(u8, u32)>,
    flag1: bool,
    ping_pong: Option<(u32, u32, u32)>,
    jit_diff: bool,
    chunk: u64,
    strict: bool,
    periph: PeriphOpts,
    lane: Option<(u32, u32)>,
    wav: Option<String>,
    rate: u32,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        entry: 0x12_5C88,
        boot: 100_000,
        passes: 300,
        warmup: 16,
        frame_rate: FRAME_RATE,
        chunk: 1_000,
        rate: 48_000,
        trigger: 0xB_00E2,
        idle_pc: 0x12_5DDF,
        devs: vec![(0x282c_0380, 1), (0x3108_d008, 5)],
        ..Options::default()
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or(format!("{a} needs a value"));
        let num = |s: String| parse_number(&s).ok_or(format!("bad value {s:?} for {a}"));
        match a.as_str() {
            "--passes" => o.passes = num(val()?)?,
            "--warmup" => o.warmup = num(val()?)?,
            "--boot" => o.boot = u64::from(num(val()?)?),
            "--entry" => o.entry = num(val()?)?,
            "--pno" => {
                o.entry = 0x12_1007;
                o.idle_pc = 0x12_1091;
                o.trigger = 0xB_00E1;
                o.boot = 3_000_000;
                o.irq = Some((15, 1));
                o.flag1 = true;
                o.ping_pong = Some(PNO_PING_PONG);
                o.devs = vec![
                    (0x28ac_0380, 1),
                    (0x3108_d008, 5),
                    (0x3108_c06c, 0x400),
                    (0x3108_c00c, 4),
                ];
            }
            "--irq" => o.irq = Some(parse_rx_irq(&val()?)?),
            "--flag1" => o.flag1 = true,
            "--jit-diff" => o.jit_diff = true,
            "--strict" => o.strict = true,
            "--chunk" => o.chunk = u64::from(num(val()?)?),
            "--trigger" => o.trigger = num(val()?)?,
            "--idle-pc" => o.idle_pc = num(val()?)?,
            "--set" => {
                let s = val()?;
                let (r, v) = s.split_once('=').ok_or("--set needs In=V")?;
                let n: usize = r
                    .strip_prefix('I')
                    .and_then(|n| n.parse().ok())
                    .filter(|n| *n < 16)
                    .ok_or("--set supports I0..I15")?;
                o.iregs.push((n, parse_value(v).ok_or("bad --set value")?));
            }
            "--probe" => o.probe = Some(num(val()?)?),
            "--msg" => o.msgs.push(parse_msg(&val()?)?),
            "--msg-script" => o.msgs.extend(read_script(&val()?, 0)?),
            "--script-flat" => {
                // every message of the file before pass 0, in file order (a finished load)
                let mut v = read_script(&val()?, 0)?;
                v.iter_mut().for_each(|m| m.pass = 0);
                o.msgs.extend(v);
            }
            "--script-at" => {
                let s = val()?;
                let (off, f) = s.split_once(':').ok_or("--script-at needs OFFSET:FILE")?;
                let off = parse_number(off).ok_or("bad --script-at offset")?;
                o.msgs.extend(read_script(f, off)?);
            }
            "--proxy-voices" => {
                let k = num(val()?)?.min(8);
                o.msgs.extend((0..k).map(|k| Msg {
                    pass: 0,
                    word1: SLOT_BASE + k * SLOT_STRIDE,
                    word2: STAGE_SAW,
                }));
            }
            "--dev" => {
                let s = val()?;
                let (x, v) = s.split_once('=').ok_or("--dev needs ADDR=V")?;
                o.devs.push((
                    parse_number(x).ok_or("bad --dev address")?,
                    parse_value(v).ok_or("bad --dev value")?,
                ));
            }
            "--pre" => {
                let s = val()?;
                let (x, v) = s.split_once('=').ok_or("--pre needs ADDR=V")?;
                o.pre.push((
                    parse_number(x).ok_or("bad --pre address")?,
                    parse_value(v).ok_or("bad --pre value")?,
                ));
            }
            "--dump" => {
                let s = val()?;
                let (x, n) = s.split_once(':').ok_or("--dump needs ADDR:COUNT")?;
                o.dumps.push((
                    parse_number(x).ok_or("bad --dump address")?,
                    parse_number(n).ok_or("bad --dump count")?,
                ));
            }
            "--sec-id" => o.periph.take_sec_id(&val()?)?,
            "--lane" => {
                let s = val()?;
                let bad = || format!("bad --lane {s:?} (want SET:PAIR)");
                let (a, b) = s.split_once(':').ok_or_else(bad)?;
                let (a, b) = (
                    parse_number(a).ok_or_else(bad)?,
                    parse_number(b).ok_or_else(bad)?,
                );
                if a > 5 || b > 2 {
                    return Err(bad());
                }
                o.lane = Some((a, b));
            }
            "--wav" => o.wav = Some(val()?),
            "--rate" => o.rate = num(val()?)?,
            "--state-out" => o.state_out = Some(val()?),
            "--state-cmp" => o.state_cmp = Some(val()?),
            "--frame-rate" => {
                o.frame_rate = val()?.parse().map_err(|_| "bad --frame-rate".to_string())?;
            }
            _ if o.periph.take_flag(a) => {}
            _ if o.image.is_empty() && !a.starts_with("--") => o.image = a.clone(),
            _ => return Err(format!("unknown argument {a:?}")),
        }
    }
    if o.image.is_empty() {
        return Err(
            "usage: sharc-sys IMAGE [--passes N] [--msg-script F]... (see the module docs)".into(),
        );
    }
    Ok(o)
}

fn read_script(file: &str, offset: u32) -> Result<Vec<Msg>, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let mut v = parse_script(&text)?;
    v.iter_mut().for_each(|m| m.pass += offset);
    Ok(v)
}

/// One timing line in microseconds.
fn line(out: &mut String, name: &str, secs: &[f64]) {
    let s = stats(secs);
    let _ = writeln!(
        out,
        "{name:<8} mean {:9.2} us  median {:9.2} us  worst {:9.2} us  (n {})",
        s.mean * 1e6,
        s.median * 1e6,
        s.worst * 1e6,
        s.n
    );
}

/// Untimed second look at every pass: coverage of `probe` and the pc ranges of the last pass.
fn probe_passes(o: &Options, cfg: &PassConfig, data: &[u8]) -> Result<String, String> {
    let mut core = boot(o, data)?;
    let total = o.warmup + o.passes;
    let (mut hit, mut last) = (0, BTreeSet::new());
    for p in 0..total {
        if let Some(pp) = o.ping_pong {
            set_ping_pong(&mut core, pp, p);
        }
        for m in o.msgs.iter().filter(|m| m.pass == p) {
            deliver(&mut core, 0, m)?;
        }
        let (_, _, visited) = probe_pass(&mut core, cfg);
        hit += u32::from(o.probe.is_some_and(|pc| visited.contains(&pc)));
        last = visited;
    }
    let ranges: Vec<String> = pc_ranges(&last)
        .iter()
        .map(|(a, b)| format!("{a:x}-{b:x}"))
        .collect();
    Ok(format!(
        "probe 0x{:x}: executed in {hit} of {total} passes
executed pcs (last pass, {} distinct): {}
",
        o.probe.unwrap_or(0),
        last.len(),
        ranges.join(" ")
    ))
}

fn boot(o: &Options, data: &[u8]) -> Result<Core, String> {
    let mut core = Core::new(data);
    o.periph.attach(&mut core);
    core.pc = o.entry;
    if o.flag1 {
        core.set_flag_input(1, true);
    }
    for &(n, v) in &o.iregs {
        core.i[n] = v;
    }
    for &(a, v) in &o.devs {
        core.preset_device(a, v);
    }
    if let Some(f) = run_steps(&mut core, o.boot) {
        return Err(format!("boot faulted: {}", fault_text(&f)));
    }
    for &(a, v) in &o.pre {
        core.dm_write(a, v)
            .map_err(|a| format!("bad --pre address 0x{a:x}"))?;
    }
    Ok(core)
}

/// Words that differ between the final SRAM and a `--state-out` file, by region.
fn compare_state(core: &mut Core, file: &str) -> Result<String, String> {
    let raw = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let mut diff = Vec::new();
    for (k, a) in SRAM_WORDS.enumerate() {
        let old = raw
            .get(4 * k..4 * k + 4)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if core.dm_read(a).unwrap_or(0) != old {
            diff.push(a);
        }
    }
    let mut out = format!("state vs {file}: {} words differ\n", diff.len());
    for (name, lo, len) in REGIONS {
        let n = diff.iter().filter(|a| (lo..lo + len).contains(a)).count();
        let _ = writeln!(out, "  {name}: {n} words differ");
    }
    let rest = diff
        .iter()
        .filter(|a| !REGIONS.iter().any(|r| (r.1..r.1 + r.2).contains(a)))
        .count();
    let _ = writeln!(out, "  elsewhere in SRAM: {rest} words differ");
    Ok(out)
}

/// Interpreter vs JIT lockstep over `warmup + passes` passes (`--jit-diff`).
#[cfg(feature = "jit")]
fn jit_diff(o: &Options, cfg: &PassConfig, data: &[u8]) -> Result<String, String> {
    use nord_sharc::runner::system_diff::{diff_system, range_len, SysRun};
    let (mut interp, mut jit) = (boot(o, data)?, boot(o, data)?);
    let total = o.warmup + o.passes;
    let run = SysRun {
        cfg,
        msgs: &o.msgs,
        total_passes: total,
        ping_pong: o.ping_pong,
        chunk: o.chunk,
        strict: o.strict,
    };
    let r = diff_system(&mut interp, &mut jit, &run)?;
    let mut out = String::new();
    let steps = &r.steps;
    let _ = writeln!(
        out,
        "jit-diff: {} of {total} passes matched (chunk {}, strict {}), {} messages",
        r.passes_ok,
        o.chunk,
        o.strict,
        o.msgs.len()
    );
    let _ = writeln!(
        out,
        "steps per pass: min {} max {} mean {:.1}",
        steps.iter().min().copied().unwrap_or(0),
        steps.iter().max().copied().unwrap_or(0),
        steps.iter().sum::<u64>() as f64 / steps.len().max(1) as f64
    );
    let ranges: Vec<String> = r
        .coverage
        .iter()
        .map(|(a, b)| format!("{a:x}-{b:x}"))
        .collect();
    let _ = writeln!(
        out,
        "coverage: {} distinct pcs in {} ranges: {}",
        range_len(&r.coverage),
        r.coverage.len(),
        ranges.join(" ")
    );
    match &r.first_divergence {
        Some((p, m)) => {
            let _ = writeln!(out, "FIRST DIVERGENCE in pass {p}: {m}");
        }
        None => {
            let _ = writeln!(out, "no divergence");
        }
    }
    Ok(out)
}

#[cfg(not(feature = "jit"))]
fn jit_diff(_: &Options, _: &PassConfig, _: &[u8]) -> Result<String, String> {
    Err("--jit-diff needs a build with --features jit".into())
}

fn run(args: &[String]) -> Result<String, String> {
    let o = parse_args(args)?;
    let data = std::fs::read(&o.image).map_err(|e| format!("{}: {e}", o.image))?;
    let cfg = PassConfig {
        trigger: o.trigger,
        idle_pc: o.idle_pc,
        max_steps: 200_000,
        irq: o.periph.start_status(o.irq),
    };
    if o.jit_diff {
        return jit_diff(&o, &cfg, &data);
    }
    let mut core = boot(&o, &data)?;
    let total = o.warmup + o.passes;
    let before: Vec<Vec<u32>> = REGIONS
        .iter()
        .map(|&(_, lo, len)| {
            (lo..lo + len)
                .map(|a| core.dm_read(a).unwrap_or(0))
                .collect()
        })
        .collect();
    let rows = run_system(&mut core, &cfg, &o.msgs, 0, total, o.ping_pong)?;
    let w = o.warmup as usize;
    let mut out = String::new();
    let steps: Vec<u64> = rows.iter().skip(w).map(|r| r.steps).collect();
    let _ = writeln!(
        out,
        "{} passes ({} warm-up), {} messages delivered, all idle: {}",
        rows.len(),
        o.warmup,
        rows.iter().map(|r| u64::from(r.msgs)).sum::<u64>(),
        rows.iter().all(|r| r.idle)
    );
    let _ = writeln!(
        out,
        "steps per pass: first {} min {} max {} mean {:.1}",
        rows.first().map_or(0, |r| r.steps),
        steps.iter().min().copied().unwrap_or(0),
        steps.iter().max().copied().unwrap_or(0),
        steps.iter().sum::<u64>() as f64 / steps.len().max(1) as f64
    );
    for (name, f) in [
        ("sharc", (|r: &PassTiming| r.sharc) as fn(&PassTiming) -> _),
        ("msg", |r| r.msg),
        ("stub", |r| r.stub),
        ("total", PassTiming::total),
    ] {
        line(&mut out, name, &series(&rows, w, f));
    }
    let period = pass_period_s(o.frame_rate);
    let mean_total = stats(&series(&rows, w, PassTiming::total)).mean;
    let worst_total = stats(&series(&rows, w, PassTiming::total)).worst;
    let _ = writeln!(
        out,
        "pass period {:.2} us; mean total / period {:.2}x, worst / period {:.2}x",
        period * 1e6,
        mean_total / period,
        worst_total / period
    );
    let _ = writeln!(
        out,
        "device accesses (unmodelled address space, whole run): {} reads, {} writes",
        core.device_reads, core.device_writes
    );
    for (k, (name, lo, len)) in REGIONS.into_iter().enumerate() {
        let changed = (lo..lo + len)
            .zip(&before[k])
            .filter(|&(a, &b)| core.dm_read(a).unwrap_or(0) != b)
            .count();
        let _ = writeln!(
            out,
            "hash {name}: {:016x}, {changed} words changed since boot",
            region_hash(&mut core, lo, len)
        );
    }
    for &(addr, n) in &o.dumps {
        let w: Vec<String> = (0..n)
            .map(|k| format!("{:x}", core.dm_read(addr.wrapping_add(k)).unwrap_or(0)))
            .collect();
        let _ = writeln!(out, "DM(0x{addr:x}..): {}", w.join(" "));
    }
    if let Some(f) = &o.state_cmp {
        out += &compare_state(&mut core, f)?;
    }
    if let Some(f) = &o.state_out {
        let bytes: Vec<u8> = SRAM_WORDS
            .flat_map(|a| core.dm_read(a).unwrap_or(0).to_le_bytes())
            .collect();
        std::fs::write(f, bytes).map_err(|e| format!("{f}: {e}"))?;
        let _ = writeln!(out, "wrote {f}");
    }
    if o.probe.is_some() {
        out += &probe_passes(&o, &cfg, &data)?;
    }
    if let Some(lane) = o.lane {
        let mut c = boot(&o, &data)?;
        let r = lane_run(&mut c, &cfg, &o.msgs, total, o.ping_pong, lane)?;
        let (peak, rms) = r.level();
        let _ = writeln!(
            out,
            "lane {}:{}: {} passes, {} frames, steps first {} last {}, peak {peak:.6}, rms {rms:.6}",
            lane.0,
            lane.1,
            r.passes,
            r.left.len(),
            r.first_steps,
            r.last_steps
        );
        if let Some(f) = &o.wav {
            std::fs::write(f, wav_bytes(&[r.left, r.right], o.rate))
                .map_err(|e| format!("{f}: {e}"))?;
            let _ = writeln!(out, "wrote {f}");
        }
    }
    if o.periph.census {
        let mut c = boot(&o, &data)?;
        out += &census_run(&mut c, &cfg, &o.msgs, total, o.ping_pong)?;
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
    fn options_parse_proxy_scripts_and_reject_bad_input() {
        let o = parse_args(&args(&[
            "img",
            "--proxy-voices",
            "3",
            "--msg",
            "2:0x10=5",
            "--probe",
            "0x1215fb",
        ]))
        .unwrap();
        assert_eq!(o.msgs.len(), 4);
        assert_eq!(o.msgs[1].word1, 0xC_10B8 + 0x18);
        assert_eq!((o.passes, o.warmup, o.probe), (300, 16, Some(0x1215FB)));
        for bad in [
            &["img", "--passes"][..],
            &["img", "--bogus"],
            &["--passes", "1"],
            &["img", "--script-at", "5"],
            &["img", "--dev", "x"],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_run_reports_steps_timing_hashes_and_state_round_trips() {
        let img = tiny_image("nord_sharc_sys_test.bin");
        let state = std::env::temp_dir().join("nord_sharc_sys_state.bin");
        let s = state.to_string_lossy().to_string();
        let common = |extra: &[&str]| {
            let mut v = vec![
                img.as_str(),
                "--entry",
                "0x120000",
                "--boot",
                "0",
                "--passes",
                "1",
                "--warmup",
                "0",
                "--idle-pc",
                "0x120002",
                "--set",
                "I0=0xB00E2",
            ];
            v.extend_from_slice(extra);
            run(&args(&v))
        };
        let text = common(&["--state-out", &s]).unwrap();
        assert!(
            text.contains("1 passes (0 warm-up), 0 messages delivered, all idle: true"),
            "{text}"
        );
        assert!(
            text.contains("steps per pass: first 1 min 1 max 1 mean 1.0"),
            "{text}"
        );
        assert!(text.contains("hash pass buffers"), "{text}");
        let same = common(&["--state-cmp", &s]).unwrap();
        assert!(same.contains("0 words differ"), "{same}");
        let text = common(&[
            "--state-cmp",
            &s,
            "--msg",
            "0:0xB0010=7",
            "--probe",
            "0x120000",
        ])
        .unwrap();
        assert!(text.contains("1 words differ"), "{text}");
        assert!(text.contains("executed in 1 of 1 passes"), "{text}");
        std::fs::remove_file(img).ok();
        std::fs::remove_file(state).ok();
    }

    #[test]
    fn irq_and_flag1_options_parse() {
        let o = parse_args(&args(&["img", "--irq", "P4I:0x41", "--flag1"])).unwrap();
        assert_eq!((o.irq, o.flag1), (Some((15, 0x41)), true));
        assert!(parse_args(&args(&["img", "--irq", "P4I"])).is_err());
    }

    #[test]
    fn pno_preset_sets_entry_irq_gates_and_ping_pong() {
        let o = parse_args(&args(&["img", "--pno"])).unwrap();
        assert_eq!(
            (o.entry, o.idle_pc, o.trigger),
            (0x12_1007, 0x12_1091, 0xB_00E1)
        );
        assert_eq!((o.irq, o.flag1, o.boot), (Some((15, 1)), true, 3_000_000));
        assert_eq!(o.ping_pong, Some(PNO_PING_PONG));
        assert_eq!(o.devs.len(), 4);
    }

    #[test]
    fn jit_diff_options_parse_and_the_tiny_image_matches() {
        let o = parse_args(&args(&["img", "--jit-diff", "--chunk", "8", "--strict"])).unwrap();
        assert_eq!((o.jit_diff, o.chunk, o.strict), (true, 8, true));
        let img = tiny_image("nord_sharc_sys_jitdiff.bin");
        let r = run(&args(&[
            &img,
            "--jit-diff",
            "--entry",
            "0x120000",
            "--boot",
            "0",
            "--passes",
            "1",
            "--warmup",
            "0",
            "--idle-pc",
            "0x120002",
            "--set",
            "I0=0xB00E2",
        ]));
        if cfg!(feature = "jit") {
            let text = r.unwrap();
            assert!(text.contains("1 of 1 passes matched"), "{text}");
            assert!(text.contains("no divergence"), "{text}");
        } else {
            assert!(r.unwrap_err().contains("--features jit"));
        }
        std::fs::remove_file(img).ok();
    }

    #[test]
    fn bad_runs_fail() {
        assert!(run(&args(&[])).is_err());
        assert!(run(&args(&["no_such_file.bin"])).is_err());
    }
}

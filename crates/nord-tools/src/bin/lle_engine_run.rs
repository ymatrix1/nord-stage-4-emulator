//! `lle_engine_run`: renders a short deterministic stretch with the unified LLE engine (ARM0 +
//! the DSP images, bean `ns4-lle5`, `docs/specs/lle-engine.md`) from local-only images and reports
//! what ran, wall time per subsystem and whether two runs are hash-identical. Offline, one thread,
//! slower than real time; it makes no claim about audio quality.
//!
//! ```text
//! lle_engine_run --dir DIR [--ticks N] [--runs N] [--note-on TICK:NOTE:VEL]... [--note-off TICK:NOTE]...
//!                [--npno FILE] [--no-arm] [--wav OUT] [--dump-msgs DIR]
//!   DIR      image directory, layout in `nord_lle::load` (arm/, PNO/, ORG/, SY/, SMP/, FX/)
//!   --ticks  ARM timer ticks to run after the boot (24,000 = 1 s of virtual time), default 2400
//!   --runs   how many times to run the whole thing for the determinism check (default 2)
//!   --no-arm DSP chips only, scripted events are dropped (no firmware needed)
//!   a fault in any run exits non-zero; data bytes (note, velocity) must be 0..=127
//! ```
use nord_core::lle_io::{link, LleEvent, LleInput};
use nord_lle::arm::{Arm0, ArmHost, FakeArm};
use nord_lle::engine::{LleEngine, LoggedMsg, Precision};
use nord_lle::load::{build_chips, default_routes, read_dir};
use nord_sharc::runner::audio::wav_bytes;
use nord_sharc::runner::system::FRAME_RATE;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

struct Args {
    dir: PathBuf,
    ticks: u64,
    runs: u32,
    events: Vec<(u64, LleEvent)>,
    npno: Option<PathBuf>,
    no_arm: bool,
    wav: Option<String>,
    dump_msgs: Option<PathBuf>,
}

fn parse_event(kind: &str, v: &str) -> Result<(u64, LleEvent), String> {
    let f: Vec<u64> = v
        .split(':')
        .map(|x| x.parse::<u64>().map_err(|e| format!("{kind} {v:?}: {e}")))
        .collect::<Result<_, _>>()?;
    let bad = || format!("bad {kind} {v:?} (data bytes are 0..=127)");
    if f.iter().skip(1).any(|x| *x > 127) {
        return Err(bad());
    }
    match (kind, f.as_slice()) {
        ("--note-on", [t, n, vel]) => Ok((
            *t,
            LleEvent::NoteOn {
                channel: 0,
                note: u8::try_from(*n).map_err(|_| bad())?,
                velocity: u8::try_from(*vel).map_err(|_| bad())?,
            },
        )),
        ("--note-off", [t, n]) => Ok((
            *t,
            LleEvent::NoteOff {
                channel: 0,
                note: u8::try_from(*n).map_err(|_| bad())?,
                velocity: 0,
            },
        )),
        _ => Err(bad()),
    }
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args {
        dir: PathBuf::new(),
        ticks: 2400,
        runs: 2,
        events: Vec::new(),
        npno: None,
        no_arm: false,
        wav: None,
        dump_msgs: None,
    };
    let mut it = args.iter();
    while let Some(k) = it.next() {
        let mut val = || it.next().cloned().ok_or(format!("{k} needs a value"));
        match k.as_str() {
            "--dir" => a.dir = val()?.into(),
            "--ticks" => a.ticks = val()?.parse().map_err(|e| format!("--ticks: {e}"))?,
            "--runs" => a.runs = val()?.parse().map_err(|e| format!("--runs: {e}"))?,
            "--note-on" | "--note-off" => a.events.push(parse_event(k, &val()?)?),
            "--npno" => a.npno = Some(val()?.into()),
            "--no-arm" => a.no_arm = true,
            "--wav" => a.wav = Some(val()?),
            "--dump-msgs" => a.dump_msgs = Some(val()?.into()),
            _ => return Err(format!("unknown argument {k:?}")),
        }
    }
    if a.dir.as_os_str().is_empty() || a.runs == 0 {
        return Err("usage: lle_engine_run --dir DIR [--ticks N] [--runs N] [--note-on T:N:V]... [--note-off T:N]... [--npno FILE] [--no-arm] [--wav OUT]".into());
    }
    a.events.sort_by_key(|e| e.0);
    Ok(a)
}

/// `peak / rms / first non-zero tick` of one signal point.
fn stage(l: &nord_sharc::runner::audio_bus::Level) -> String {
    let first = l
        .first_nonzero_tick
        .map_or("-".to_string(), |t| t.to_string());
    format!("{:.6}/{:.6}/{first}/nf{}", l.peak, l.rms(), l.non_finite)
}

/// `(peak, rms, nonzero frames)` of the finite samples.
fn level(l: &[f32], r: &[f32]) -> (f32, f64, usize) {
    let v = || l.iter().chain(r).copied().filter(|x| x.is_finite());
    let peak = v().fold(0f32, |m, x| m.max(x.abs()));
    let ss: f64 = v().map(|x| f64::from(x).powi(2)).sum();
    let n = v().count().max(1) as f64;
    let nz = l
        .iter()
        .zip(r)
        .filter(|(a, b)| **a != 0.0 || **b != 0.0)
        .count();
    (peak, (ss / n).sqrt(), nz)
}

struct Outcome {
    fault: Option<String>,
    digest: u64,
    report: String,
    left: Vec<f32>,
    right: Vec<f32>,
}

/// One `NAME.msgs` file per node: `--msg PASS:WORD=VALUE` tokens, the replay format of
/// `lle_sharc_bus` (`runner::msg::parse_msg_text`).
fn write_msgs(dir: &std::path::Path, names: &[String], log: &[LoggedMsg]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (i, n) in names.iter().enumerate() {
        let mut t = String::new();
        for m in log.iter().filter(|m| m.node == i) {
            let _ = writeln!(t, "--msg {}:{:#x}={:#x}", m.pass, m.word, m.value);
        }
        let f = dir.join(format!("{n}.msgs"));
        std::fs::write(&f, t).map_err(|e| format!("{}: {e}", f.display()))?;
    }
    Ok(())
}

fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

fn run_once(a: &Args, first: bool) -> Result<Outcome, String> {
    let mut out = String::new();
    let t = Instant::now();
    let images = read_dir(&a.dir)?;
    let _ = writeln!(
        out,
        "images read in {:.2}s; missing: {}",
        t.elapsed().as_secs_f64(),
        images.missing.len()
    );
    for m in &images.missing {
        let _ = writeln!(out, "  missing {m}");
    }
    let t = Instant::now();
    let chips = build_chips(&images)?;
    let names: Vec<String> = chips.iter().map(|c| c.name.clone()).collect();
    let _ = writeln!(
        out,
        "DSP booted in {:.2}s: {names:?}",
        t.elapsed().as_secs_f64()
    );
    let routes = default_routes(&chips);
    let arm: Box<dyn ArmHost> = match (&images.arm, a.no_arm) {
        (Some(im), false) => {
            let t = Instant::now();
            let arm = Arm0::boot(&im.code, &im.sram, im.eeprom.as_deref())?;
            let _ = writeln!(
                out,
                "ARM0 booted in {:.2}s ({} virtual insns)",
                t.elapsed().as_secs_f64(),
                arm.boot_insns
            );
            Box::new(arm)
        }
        (None, false) => return Err(
            "ARM images missing (arm/bank0_CODE.bin, bank0_SRAM.bin); use --no-arm for DSP only"
                .into(),
        ),
        (_, true) => Box::new(FakeArm::default()),
    };
    let mut e = LleEngine::new(arm, chips, routes, Precision::Exact)?;
    if let Some(p) = &a.npno {
        let len = std::fs::metadata(p)
            .map_err(|x| format!("{}: {x}", p.display()))?
            .len();
        if len > nord_sharc::runner::live_flash::MAX_BYTES {
            return Err(format!("{}: {len} bytes exceeds the cap", p.display()));
        }
        e.bind_smp(std::fs::read(p).map_err(|x| format!("{}: {x}", p.display()))?)?;
    }
    if a.dump_msgs.is_some() {
        e.record_messages();
    }
    let (mut host, mut eng) = link(256);
    let mut next = 0;
    let t = Instant::now();
    let mut fault = None;
    for tick in 0..a.ticks {
        while next < a.events.len() && a.events[next].0 <= tick {
            if !host.events.push(a.events[next].1) {
                return Err(format!(
                    "event ring full at tick {tick} ({} events due)",
                    a.events.len()
                ));
            }
            next += 1;
        }
        e.drain(&mut eng.events);
        if let Err(x) = e.step() {
            fault = Some(x);
            break;
        }
    }
    let wall = t.elapsed().as_secs_f64();
    let s = &e.stats;
    let virt = s.ticks as f64 / 24_000.0;
    let _ = writeln!(
        out,
        "ran {} ticks ({virt:.3} s virtual) in {wall:.2}s wall = {:.1}x slower than real time",
        s.ticks,
        wall / virt.max(1e-9)
    );
    let _ = writeln!(
        out,
        "passes {}, ARM messages {}, midi events {}, panel {}, unmodelled {}",
        s.passes, s.messages, s.events_midi, s.events_panel, s.events_unmodelled
    );
    let _ = writeln!(
        out,
        "wall: ARM {:.1} ms, routing {:.1} ms, DSP bus {:.1} ms",
        ms(s.arm_ns),
        ms(s.route_ns),
        ms(s.dsp_ns)
    );
    let _ = writeln!(out, "per-stage levels (peak / rms / first non-zero tick / non-finite count; in = frames the node took, out = frames it produced):");
    for (i, n) in e.node_names().iter().enumerate() {
        let l = &e.bus.node_levels[i];
        let _ = writeln!(
            out,
            "  {n:<5} {:>8.1} ms  short {:>3}  in {}  out {}",
            ms(e.bus.node_ns[i]),
            e.bus.underruns[i],
            stage(&l.input),
            stage(&l.output)
        );
    }
    let r = &e.router;
    let _ = writeln!(
        out,
        "routed {:?}; skipped {}, counted (no chip) {}, L2 forwarded {} dropped {}, bad SMP descriptors {}",
        r.sent, r.skipped, r.counted, r.l2_forwarded, r.l2_dropped, e.stats.bad_descriptors
    );
    if let Some(sr) = e.smp_refill() {
        let _ = writeln!(
            out,
            "SMP refill: descriptors {}, law ok {}, placed {}, rec11 {:?}",
            sr.descriptors, sr.law_ok, sr.placed, sr.rec11
        );
    }
    let (peak, rms, nz) = level(&e.host().left, &e.host().right);
    let _ = writeln!(out, "host: {} frames, nonzero {nz}, peak {peak:.6}, rms {rms:.6} (DSP frame rate {FRAME_RATE} Hz, not resampled)", e.host().left.len());
    let _ = writeln!(
        out,
        "precision: requested {:?}, effective {:?}",
        e.requested_precision(),
        e.effective_precision()
    );
    if let Some(x) = &fault {
        let _ = writeln!(out, "FAULT: {x}");
    }
    if first {
        let _ = writeln!(out, "ARM insns {}", e.arm.insns());
    }
    if let (Some(dir), Some(log)) = (&a.dump_msgs, &e.msg_log) {
        write_msgs(dir, e.node_names(), log)?;
        let _ = writeln!(out, "wrote {} messages to {}", log.len(), dir.display());
    }
    let digest = e.digest();
    let _ = writeln!(out, "digest {digest:016x}");
    Ok(Outcome {
        fault,
        digest,
        report: out,
        left: e.host().left.clone(),
        right: e.host().right.clone(),
    })
}

/// A fault in any run is an error (non-zero exit): the report is returned as the error text and
/// carries no determinism claim.
fn fault_exit(faults: &[String], text: &mut String) -> Result<(), String> {
    if faults.is_empty() {
        return Ok(());
    }
    let _ = writeln!(text, "FAULTED, no determinism claim: {faults:?}");
    Err(std::mem::take(text))
}

fn run(args: &[String]) -> Result<String, String> {
    let a = parse(args)?;
    let mut text = String::new();
    let mut digests = Vec::new();
    let mut first = None;
    let mut faults: Vec<String> = Vec::new();
    for i in 0..a.runs {
        let o = run_once(&a, i == 0)?;
        let _ = writeln!(text, "== run {} ==\n{}", i + 1, o.report);
        digests.push(o.digest);
        if let Some(f) = o.fault {
            faults.push(format!("run {}: {f}", i + 1));
        }
        first.get_or_insert((o.left, o.right));
    }
    fault_exit(&faults, &mut text)?;
    let same = digests.windows(2).all(|w| w[0] == w[1]);
    let _ = writeln!(
        text,
        "determinism over {} runs: {}",
        a.runs,
        if a.runs < 2 {
            "not checked (1 run)"
        } else if same {
            "IDENTICAL"
        } else {
            "DIFFERENT"
        }
    );
    if let (Some(f), Some((l, r))) = (&a.wav, first) {
        std::fs::write(f, wav_bytes(&[l, r], FRAME_RATE as u32))
            .map_err(|e| format!("{f}: {e}"))?;
        let _ = writeln!(text, "wrote {f}");
    }
    Ok(text)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(t) => print!("{t}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn arguments_and_events_parse() {
        let a = parse(&v(&[
            "--dir",
            "d",
            "--ticks",
            "5",
            "--runs",
            "3",
            "--note-on",
            "10:60:100",
            "--note-off",
            "4:60",
            "--no-arm",
        ]))
        .unwrap();
        assert_eq!((a.ticks, a.runs, a.no_arm), (5, 3, true));
        assert_eq!(a.events[0].0, 4, "events are sorted by tick");
        assert!(matches!(
            a.events[1].1,
            LleEvent::NoteOn {
                note: 60,
                velocity: 100,
                ..
            }
        ));
        for bad in [
            &["--ticks", "5"][..],
            &["--dir", "d", "--runs", "0"],
            &["--dir", "d", "--bogus"],
            &["--dir", "d", "--note-on", "1:2"],
            &["--dir", "d", "--note-on", "1:300:1"],
            &["--dir"],
        ] {
            assert!(parse(&v(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_stage_line_shows_peak_rms_and_the_first_non_zero_tick() {
        let mut l = nord_sharc::runner::audio_bus::Level::default();
        assert_eq!(stage(&l), "0.000000/0.000000/-/nf0");
        l.add(&[[0.0, 0.0], [0.5, -0.5], [f32::NAN, 0.0]], 7);
        assert_eq!(stage(&l), "0.500000/0.316228/7/nf1");
    }

    #[test]
    fn messages_are_written_one_replay_file_per_node() {
        let dir = std::env::temp_dir().join(format!("lle_msgs_{}", std::process::id()));
        let log = [LoggedMsg {
            node: 1,
            pass: 3,
            word: 0x10,
            value: 7,
        }];
        write_msgs(&dir, &["a".into(), "b".into()], &log).unwrap();
        let b = std::fs::read_to_string(dir.join("b.msgs")).unwrap();
        let a = std::fs::read_to_string(dir.join("a.msgs")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            (b.as_str(), a.as_str()),
            (
                "--msg 3:0x10=0x7
",
                ""
            )
        );
    }

    #[test]
    fn a_fault_makes_the_run_an_error_with_no_determinism_claim() {
        let mut t = String::from(
            "report
",
        );
        assert!(fault_exit(&[], &mut t).is_ok());
        let e = fault_exit(&["run 1: boom".into()], &mut t).unwrap_err();
        assert!(
            e.contains("FAULTED") && e.contains("boom") && e.contains("report"),
            "{e}"
        );
    }

    #[test]
    fn data_bytes_above_127_are_rejected() {
        assert!(parse_event("--note-on", "1:60:127").is_ok());
        assert!(parse_event("--note-on", "1:60:128").is_err());
        assert!(parse_event("--note-off", "1:200").is_err());
    }

    #[test]
    fn level_counts_nonzero_frames_and_ignores_non_finite() {
        let (p, _, nz) = level(&[0.0, 0.5, f32::NAN], &[0.0, 0.0, 0.0]);
        assert_eq!((p, nz), (0.5, 2));
        assert_eq!(level(&[], &[]).2, 0);
    }

    #[test]
    fn a_directory_with_no_images_runs_dsp_only_and_reports_everything_missing() {
        let dir = std::env::temp_dir().join(format!("lle_engine_run_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().into_owned();
        let e = run(&v(&["--dir", &d])).unwrap_err();
        assert!(e.contains("ARM images missing"), "{e}");
        let t = run(&v(&[
            "--dir", &d, "--no-arm", "--ticks", "10", "--runs", "2",
        ]))
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(t.contains("determinism over 2 runs: IDENTICAL"), "{t}");
        assert!(t.contains("missing"), "{t}");
        assert!(run(&v(&["--dir", "/no/such/dir"])).is_err());
    }
}

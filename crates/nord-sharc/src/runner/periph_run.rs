//! Opt-in DSP peripheral options of the system run (`sharc-sys`, `sharc-pass`): the range-transfer
//! registers of the ORG Core2 image (`core::periph::epdma`) and the per-pass census of every
//! access outside SRAM (`core::periph::access_log`). Everything is off unless a flag is given, so
//! the existing runs and goldens do not change (`docs/research/dsp-peripherals-audio.md`).
use super::msg::{deliver, Msg};
use super::pass::PassConfig;
use super::system::{lean_pass, set_ping_pong};
use crate::core::Core;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Which peripheral models a run switches on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PeriphOpts {
    /// `--epdma`: serve `DM(0x3E000..0x3E03F)` with the range-transfer rule (replaces `--pre 0x3E021=1`).
    pub epdma: bool,
    /// `--census`: list every register and device word the passes touch.
    pub census: bool,
    /// `--sec-id ID`: start Core1-style passes through the SEC model: source `ID` is raised, the
    /// ISR reads it from `CSID` and ends it with a write to `END` (replaces the preset status).
    pub sec_id: Option<u32>,
}

impl PeriphOpts {
    /// Takes `arg` when it is one of the peripheral flags; returns whether it was.
    pub fn take_flag(&mut self, arg: &str) -> bool {
        match arg {
            "--epdma" => self.epdma = true,
            "--census" => self.census = true,
            _ => return false,
        }
        true
    }

    /// Takes the value of `--sec-id`.
    pub fn take_sec_id(&mut self, value: &str) -> Result<(), String> {
        self.sec_id = Some(super::parse_number(value).ok_or(format!("bad --sec-id {value:?}"))?);
        Ok(())
    }

    /// Whether any model or census is on.
    pub fn any(&self) -> bool {
        self.epdma || self.census || self.sec_id.is_some()
    }

    /// Pass-start status for `irq`: the SEC source id when `--sec-id` is given.
    pub fn start_status(&self, irq: Option<(u8, u32)>) -> Option<(u8, u32)> {
        irq.map(|(v, st)| (v, self.sec_id.unwrap_or(st)))
    }

    /// Switches the selected models on (call before the boot so the start-up writes are seen).
    pub fn attach(&self, core: &mut Core) {
        if self.epdma {
            core.enable_epdma();
        }
        if self.census {
            core.enable_access_log();
        }
        if self.sec_id.is_some() {
            core.enable_sc57x(0);
        }
    }
}

/// Accesses of one address over the measured passes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Row {
    passes: u32,
    first: u32,
    reads: u64,
    writes: u64,
    min_r: u64,
    max_r: u64,
    min_w: u64,
    max_w: u64,
}

/// Runs `total` passes like `run_system` (untimed, with the same message and ping-pong order) and
/// returns the census text: the accesses of the boot phase and, per address, how many passes
/// touched it and the reads and writes per touching pass.
pub fn census_run(
    core: &mut Core,
    cfg: &PassConfig,
    msgs: &[Msg],
    total: u32,
    ping_pong: Option<(u32, u32, u32)>,
) -> Result<String, String> {
    let boot: Vec<(u32, u64, u64)> = core
        .access_log()
        .map(|l| l.since_mark())
        .unwrap_or_default();
    let mut due: Vec<&Msg> = msgs.iter().collect();
    due.sort_by_key(|m| m.pass);
    let (mut next, mut rows, mut ran) = (0, BTreeMap::<u32, Row>::new(), 0);
    for p in 0..total {
        if let Some(pp) = ping_pong {
            set_ping_pong(core, pp, p);
        }
        while next < due.len() && due[next].pass <= p {
            deliver(core, 0, due[next])?;
            next += 1;
        }
        if let Some(l) = core.access_log() {
            l.mark();
        }
        let (_, idle) = lean_pass(core, cfg);
        ran += 1;
        for (a, r, w) in core
            .access_log()
            .map(|l| l.since_mark())
            .unwrap_or_default()
        {
            let e = rows.entry(a).or_default();
            if e.passes == 0 {
                (e.first, e.min_r, e.min_w) = (p, u64::MAX, u64::MAX);
            }
            e.passes += 1;
            (e.reads, e.writes) = (e.reads + r, e.writes + w);
            (e.min_r, e.max_r) = (e.min_r.min(r), e.max_r.max(r));
            (e.min_w, e.max_w) = (e.min_w.min(w), e.max_w.max(w));
        }
        if !idle {
            break;
        }
    }
    let mut out = format!("census: {ran} passes; boot and set-up accesses outside SRAM:\n");
    for (a, r, w) in &boot {
        let _ = writeln!(out, "  boot 0x{a:08x} reads {r} writes {w}");
    }
    let _ = writeln!(
        out,
        "per address over the passes (passes touching, first pass, reads per touching pass min-max, writes per touching pass min-max):"
    );
    for (a, e) in &rows {
        let _ = writeln!(
            out,
            "  0x{a:08x} passes {:5} first {:4} reads {}-{} writes {}-{}",
            e.passes, e.first, e.min_r, e.max_r, e.min_w, e.max_w
        );
    }
    if let Some(l) = core.access_log() {
        let _ = write!(
            out,
            "totals and last values:
{}",
            l.table()
        );
    }
    if let Some(e) = core.epdma() {
        let _ = writeln!(out, "epdma ranges started: {:x?}", e.started);
    }
    Ok(out)
}

/// Output pointer words `DM(0xB0159 + k)`, `k` = 0..5 (VERIFIED, `docs/research/pno-full-pass.md`
/// section 1: one pass renders 2 frames per pointer set, 3 pairs of `[L0 R0 L1 R1]`).
pub const OUT_POINTERS: u32 = 0xB_0159;

/// The 12 words behind output pointer `set` (0..5): three pairs `[L0 R0 L1 R1]`.
pub fn lane_words(core: &mut Core, set: u32) -> [u32; 12] {
    let base = core.dm_read(OUT_POINTERS + set).unwrap_or(0);
    let mut w = [0u32; 12];
    for (k, slot) in w.iter_mut().enumerate() {
        *slot = core.dm_read(base.wrapping_add(k as u32)).unwrap_or(0);
    }
    w
}

/// Output of [`lane_run`]: the stereo lane and the pass statistics.
#[derive(Debug, Default, PartialEq)]
pub struct LaneRun {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// Passes that returned to the poll loop.
    pub passes: u32,
    /// Interpreter steps of the first and the last pass.
    pub first_steps: u64,
    pub last_steps: u64,
}

impl LaneRun {
    /// `(peak, rms)` over both channels, finite samples only.
    pub fn level(&self) -> (f32, f64) {
        let (mut peak, mut sum, mut n) = (0f32, 0f64, 0usize);
        for &x in self
            .left
            .iter()
            .chain(&self.right)
            .filter(|x| x.is_finite())
        {
            peak = peak.max(x.abs());
            sum += f64::from(x) * f64::from(x);
            n += 1;
        }
        (peak, if n == 0 { 0.0 } else { (sum / n as f64).sqrt() })
    }
}

/// Runs `total` passes (untimed, same message and ping-pong order as `run_system`) and records the
/// output pair `pair` of pointer set `set` after every pass, 2 frames per pass. This is the
/// SPORT transmit DMA of the hardware: it reads the output buffers between passes.
pub fn lane_run(
    core: &mut Core,
    cfg: &PassConfig,
    msgs: &[Msg],
    total: u32,
    ping_pong: Option<(u32, u32, u32)>,
    (set, pair): (u32, u32),
) -> Result<LaneRun, String> {
    let mut due: Vec<&Msg> = msgs.iter().collect();
    due.sort_by_key(|m| m.pass);
    let (mut next, mut out) = (0, LaneRun::default());
    for p in 0..total {
        if let Some(pp) = ping_pong {
            set_ping_pong(core, pp, p);
        }
        while next < due.len() && due[next].pass <= p {
            deliver(core, 0, due[next])?;
            next += 1;
        }
        let (steps, idle) = lean_pass(core, cfg);
        if !idle {
            break;
        }
        let (w, b) = (lane_words(core, set), 4 * pair as usize);
        let f = |k: usize| f32::from_bits(w[b + k]);
        out.left.extend([f(0), f(2)]);
        out.right.extend([f(1), f(3)]);
        out.passes += 1;
        if p == 0 {
            out.first_steps = steps;
        }
        out.last_steps = steps;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::pass::tests::clearing_core;
    use super::*;

    #[test]
    fn flags_parse_and_attach_the_selected_models() {
        let mut o = PeriphOpts::default();
        assert!(!o.any() && !o.take_flag("--bogus"));
        assert!(o.take_flag("--epdma") && o.take_flag("--census") && o.any());
        let mut c = clearing_core();
        o.attach(&mut c);
        assert!(c.epdma().is_some() && c.access_log().is_some());
    }

    #[test]
    fn sec_id_selects_the_start_status_and_the_sec_model() {
        let mut o = PeriphOpts::default();
        assert_eq!(o.start_status(Some((15, 1))), Some((15, 1)));
        assert!(o.take_sec_id("zz").is_err());
        o.take_sec_id("94").unwrap();
        assert_eq!(o.start_status(Some((15, 1))), Some((15, 94)));
        assert_eq!(o.start_status(None), None, "mailbox-started passes stay so");
        let mut c = clearing_core();
        o.attach(&mut c);
        assert!(c.sc57x().is_some());
    }

    #[test]
    fn lane_run_reads_the_pointer_set_after_every_pass() {
        let mut c = clearing_core();
        // pointer set 2 -> 0xB0200; the pair 1 words sit at +4
        c.dm_write(OUT_POINTERS + 2, 0xB_0200).unwrap();
        for (k, v) in [(4, 0.5f32), (5, -0.25), (6, 0.125), (7, 1.0)] {
            c.dm_write(0xB_0200 + k, v.to_bits()).unwrap();
        }
        let cfg = PassConfig {
            trigger: 0xB_00E2,
            idle_pc: 0x12_0002,
            max_steps: 10,
            irq: None,
        };
        let r = lane_run(&mut c, &cfg, &[], 3, None, (2, 1)).unwrap();
        // the tiny core idles once, then hits the step limit (see system.rs tests)
        assert_eq!(r.passes, 1);
        assert_eq!((r.left, r.right), (vec![0.5, 0.125], vec![-0.25, 1.0]));
        assert_eq!((r.first_steps, r.last_steps), (1, 1));
        let lr = LaneRun {
            left: vec![0.5, f32::NAN],
            right: vec![-1.0],
            ..LaneRun::default()
        };
        let (peak, rms) = lr.level();
        assert_eq!(peak, 1.0);
        assert!((rms - ((0.25 + 1.0) / 2.0f64).sqrt()).abs() < 1e-12);
        assert_eq!(LaneRun::default().level(), (0.0, 0.0));
    }

    #[test]
    fn census_counts_register_accesses_per_pass() {
        let mut c = clearing_core();
        PeriphOpts {
            epdma: true,
            census: true,
            sec_id: None,
        }
        .attach(&mut c);
        // a register write between passes is a boot-phase access of the census
        c.dm_write(0x3_E010, 5).unwrap();
        let cfg = PassConfig {
            trigger: 0xB_00E2,
            idle_pc: 0x12_0002,
            max_steps: 10,
            irq: None,
        };
        let text = census_run(&mut c, &cfg, &[], 1, None).unwrap();
        assert!(text.contains("boot 0x0003e010 reads 0 writes 1"), "{text}");
        assert!(text.contains("census: 1 passes"), "{text}");
    }
}

//! Lockstep differential runner comparing [`Core::run`] (interpreter oracle) against
//! [`Core::run_jit`] every `N` steps across full architectural state and SRAM, stopping at the
//! first divergence.

use super::pass::SRAM_WORDS;
use super::pass::{pc_ranges, PassConfig, SEC_CSID};
use crate::core::{Core, Fault};
use std::collections::BTreeSet;
use std::fmt;

/// First mismatch observed between interpreter and JIT execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffMismatch {
    pub step: u64,
    pub pc: u32,
    pub detail: String,
}

impl fmt::Display for DiffMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "divergence at step {} (pc 0x{:x}): {}",
            self.step, self.pc, self.detail
        )
    }
}

/// Compares full architectural state (`pc`, `steps`, `r`, `s`, `i`, `m`, `l`, `b`, `mr`, `flags`,
/// `astat`, `mode1`, `mode2`, and all `SRAM_WORDS`) between `interp` and `jit`.
pub fn compare_full_state(interp: &mut Core, jit: &mut Core) -> Option<DiffMismatch> {
    let step = interp.steps;
    let pc = interp.pc;
    let fail = |detail: String| DiffMismatch { step, pc, detail };

    if jit.pc != interp.pc {
        return Some(fail(format!(
            "pc: interp=0x{:x} jit=0x{:x}",
            interp.pc, jit.pc
        )));
    }
    if jit.steps != interp.steps {
        return Some(fail(format!(
            "steps: interp={} jit={}",
            interp.steps, jit.steps
        )));
    }
    if jit.r != interp.r {
        return Some(fail(format!("r: interp={:?} jit={:?}", interp.r, jit.r)));
    }
    if jit.s != interp.s {
        return Some(fail(format!("s: interp={:?} jit={:?}", interp.s, jit.s)));
    }
    if jit.i != interp.i {
        return Some(fail(format!("i: interp={:?} jit={:?}", interp.i, jit.i)));
    }
    if jit.m != interp.m {
        return Some(fail(format!("m: interp={:?} jit={:?}", interp.m, jit.m)));
    }
    if jit.l != interp.l {
        return Some(fail(format!("l: interp={:?} jit={:?}", interp.l, jit.l)));
    }
    if jit.b != interp.b {
        return Some(fail(format!("b: interp={:?} jit={:?}", interp.b, jit.b)));
    }
    if jit.mr != interp.mr {
        return Some(fail(format!("mr: interp={:?} jit={:?}", interp.mr, jit.mr)));
    }
    if jit.flags != interp.flags {
        return Some(fail(format!(
            "flags: interp={:?} jit={:?}",
            interp.flags, jit.flags
        )));
    }
    if jit.astat(0) != interp.astat(0) || jit.astat(1) != interp.astat(1) {
        return Some(fail(format!(
            "astat: interp=(0x{:x},0x{:x}) jit=(0x{:x},0x{:x})",
            interp.astat(0),
            interp.astat(1),
            jit.astat(0),
            jit.astat(1)
        )));
    }
    if jit.mode1 != interp.mode1 || jit.mode2 != interp.mode2 {
        return Some(fail(format!(
            "mode: interp=(0x{:x},0x{:x}) jit=(0x{:x},0x{:x})",
            interp.mode1, interp.mode2, jit.mode1, jit.mode2
        )));
    }
    for addr in SRAM_WORDS {
        let iv = interp.dm_read(addr).unwrap_or(0);
        let jv = jit.dm_read(addr).unwrap_or(0);
        if iv != jv {
            return Some(fail(format!(
                "sram[0x{addr:x}]: interp=0x{iv:08x} jit=0x{jv:08x}"
            )));
        }
    }
    None
}

/// [`compare_full_state`], plus with `strict` everything [`Core::state_mismatch`] checks (PEy
/// flags, float extensions, ASTAT extras, STKY, interrupt latches).
pub fn compare_state(interp: &mut Core, other: &mut Core, strict: bool) -> Option<DiffMismatch> {
    if let Some(m) = compare_full_state(interp, other) {
        return Some(m);
    }
    if strict {
        if let Some(detail) = interp.state_mismatch(other) {
            return Some(DiffMismatch {
                step: interp.steps,
                pc: interp.pc,
                detail,
            });
        }
    }
    None
}

/// Runs `interp` via [`Core::run`] and `other` via `run_other` for `steps` in lockstep
/// `chunk`-step increments, comparing state after every chunk (see [`compare_state`]) and
/// stopping at the first divergence.
pub fn diff_run_with(
    interp: &mut Core,
    other: &mut Core,
    steps: u64,
    chunk: u64,
    strict: bool,
    run_other: &mut dyn FnMut(&mut Core, u64) -> Option<Fault>,
) -> Result<(), DiffMismatch> {
    let chunk = chunk.max(1);
    let mut done = 0u64;
    while done < steps {
        let n = chunk.min(steps - done);
        let fi = interp.run(n);
        let fj = run_other(other, n);
        if fi != fj {
            return Err(DiffMismatch {
                step: interp.steps,
                pc: interp.pc,
                detail: format!("fault mismatch: interp={fi:?} other={fj:?}"),
            });
        }
        if let Some(m) = compare_state(interp, other, strict) {
            return Err(m);
        }
        if fi.is_some() {
            break;
        }
        done += n;
    }
    Ok(())
}

/// Runs `interp` via [`Core::run`] and `jit` via [`Core::run_jit`] for `steps` in lockstep
/// `chunk`-step increments, comparing full state after every chunk and stopping at the first
/// divergence.
#[cfg(feature = "jit")]
pub fn diff_run(
    interp: &mut Core,
    jit: &mut Core,
    steps: u64,
    chunk: u64,
) -> Result<(), DiffMismatch> {
    diff_run_with(interp, jit, steps, chunk, false, &mut |c, n| c.run_jit(n))
}

/// Summary of one lockstep differential audio pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassDiffReport {
    pub steps: u64,
    pub final_pc: u32,
    pub coverage: Vec<(u32, u32)>,
}

/// Starts one pass on both `interp` and `jit` (writing `cfg.trigger = 1` or raising `cfg.irq`),
/// then advances both in lockstep `chunk`-step increments until `cfg.idle_pc` is reached with
/// `cfg.trigger == 0` (or `cfg.max_steps` is hit), comparing full state + SRAM after every chunk
/// and stopping at the first divergence.
pub fn diff_pass_with(
    interp: &mut Core,
    jit: &mut Core,
    cfg: &PassConfig,
    chunk: u64,
    track_coverage: bool,
    strict: bool,
    run_other: &mut dyn FnMut(&mut Core, u64) -> Option<Fault>,
) -> Result<PassDiffReport, DiffMismatch> {
    for c in [&mut *interp, &mut *jit] {
        match cfg.irq {
            Some((vector, status)) => {
                c.preset_device(SEC_CSID, status);
                c.raise_interrupt(vector);
            }
            None => {
                if let Err(addr) = c.dm_write(cfg.trigger, 1) {
                    return Err(DiffMismatch {
                        step: c.steps,
                        pc: c.pc,
                        detail: format!("bad trigger address 0x{addr:x}"),
                    });
                }
            }
        }
    }

    let chunk = chunk.max(1);
    let mut visited = BTreeSet::new();
    let start_steps = interp.steps;
    let mut left_idle = false;

    while interp.steps - start_steps < cfg.max_steps {
        let mut chunk_steps = 0u64;
        while chunk_steps < chunk && interp.steps - start_steps < cfg.max_steps {
            if track_coverage {
                visited.insert(interp.pc);
            }
            if let Err(f) = interp.step() {
                return Err(DiffMismatch {
                    step: interp.steps,
                    pc: interp.pc,
                    detail: format!("interp faulted: {f:?}"),
                });
            }
            chunk_steps += 1;
            if interp.pc != cfg.idle_pc {
                left_idle = true;
            }
            if left_idle
                && interp.pc == cfg.idle_pc
                && interp.dm_read(cfg.trigger).unwrap_or(1) == 0
            {
                break;
            }
        }
        if let Some(f) = run_other(jit, chunk_steps) {
            return Err(DiffMismatch {
                step: jit.steps,
                pc: jit.pc,
                detail: format!("other core faulted: {f:?}"),
            });
        }
        if let Some(m) = compare_state(interp, jit, strict) {
            return Err(m);
        }
        if left_idle && interp.pc == cfg.idle_pc && interp.dm_read(cfg.trigger).unwrap_or(1) == 0 {
            break;
        }
    }

    Ok(PassDiffReport {
        steps: interp.steps - start_steps,
        final_pc: interp.pc,
        coverage: pc_ranges(&visited),
    })
}

/// [`diff_pass_with`] for the Cranelift JIT.
#[cfg(feature = "jit")]
pub fn diff_pass(
    interp: &mut Core,
    jit: &mut Core,
    cfg: &PassConfig,
    chunk: u64,
    track_coverage: bool,
) -> Result<PassDiffReport, DiffMismatch> {
    diff_pass_with(
        interp,
        jit,
        cfg,
        chunk,
        track_coverage,
        false,
        &mut |c, n| c.run_jit(n),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "jit")]
    fn load_program(c: &mut Core, program: &[u16]) {
        for (idx, chunk) in program.chunks(2).enumerate() {
            let lo = u32::from(chunk[0]);
            let hi = u32::from(chunk.get(1).copied().unwrap_or(0));
            c.dm_write(0x9_0000 + idx as u32, lo | (hi << 16)).unwrap();
        }
        c.pc = 0x12_0000;
    }

    #[test]
    fn compare_full_state_detects_register_and_sram_divergence() {
        let mut a = Core::new(&[]);
        let mut b = Core::new(&[]);
        assert_eq!(compare_full_state(&mut a, &mut b), None);

        b.r[3] = 42;
        let err = compare_full_state(&mut a, &mut b).unwrap();
        assert!(err.detail.contains("r:"), "{err}");

        b.r[3] = 0;
        b.dm_write(0x9_0123, 0xCAFE_BABE).unwrap();
        let err = compare_full_state(&mut a, &mut b).unwrap();
        assert!(err.detail.contains("0x90123"), "{err}");
    }

    #[cfg(feature = "jit")]
    #[test]
    fn diff_run_matches_and_stops_on_first_divergence() {
        // DO (+6) UNTIL LCE, LCNTR=R5; R0=DM(I2,M0); R1=R1+R0; F3=F1*F2
        let prog = [
            0x0d05u16, 0x0000, 0x0006, // DO (+6)
            0x9410, // R0 = DM(I2, M0)
            0x013e, 0x0000, 0x1110, // R1 = R1 + R0
            0x013e, 0x0013, 0x0312, // F3 = F1 * F2
        ];
        let mut interp = Core::new(&[]);
        let mut jit = Core::new(&[]);
        for c in [&mut interp, &mut jit] {
            load_program(c, &prog);
            c.r[5] = 10;
            c.r[1] = 1.5f32.to_bits();
            c.r[2] = 2.0f32.to_bits();
            (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0100, 1, 4, 0x9_0100);
            for k in 0..4u32 {
                c.dm_write(0x9_0100 + k, k + 1).unwrap();
            }
        }
        assert_eq!(diff_run(&mut interp, &mut jit, 1 + 10 * 3, 5), Ok(()));

        // Inject a 1-word SRAM divergence and verify diff_run stops immediately.
        jit.dm_write(0x9_0200, 1).unwrap();
        let err = diff_run(&mut interp, &mut jit, 5, 1).unwrap_err();
        assert!(err.detail.contains("0x90200"), "{err}");
    }

    #[cfg(feature = "jit")]
    #[test]
    fn diff_pass_tracks_coverage_and_matches_idle_loop() {
        let prog = [0x0001u16, 0x0001, 0x073e, 0x00ff, 0xfffe];
        let mut interp = Core::new(&[]);
        let mut jit = Core::new(&[]);
        for c in [&mut interp, &mut jit] {
            load_program(c, &prog);
        }
        let cfg = PassConfig {
            trigger: 0xB_00E2,
            idle_pc: 0x12_0000,
            max_steps: 9,
            irq: None,
        };
        let rep = diff_pass(&mut interp, &mut jit, &cfg, 4, true).unwrap();
        assert_eq!(rep.steps, 9);
        assert_eq!(rep.final_pc, 0x12_0000);
        assert!(!rep.coverage.is_empty());
    }

    #[cfg(feature = "jit")]
    #[test]
    #[ignore = "real ORG/firmware differential check: set NORD_SHARC_ORG_IMAGE=path/to/image.bin"]
    fn diff_real_org_image_from_env() {
        let path = std::env::var("NORD_SHARC_ORG_IMAGE")
            .expect("set NORD_SHARC_ORG_IMAGE to a raw SHARC firmware image");
        let image = std::fs::read(&path).expect("read NORD_SHARC_ORG_IMAGE");
        let mut interp = Core::new(&image);
        let mut jit = Core::new(&image);
        interp.lenient = true;
        jit.lenient = true;
        assert_eq!(diff_run(&mut interp, &mut jit, 200_000, 1_000), Ok(()));
    }

    #[cfg(feature = "jit")]
    #[test]
    #[ignore = "real ORG pass differential check (no --lenient, idle/1-voice/8-voice): set NORD_SHARC_ORG_IMAGE=path/to/core2_sram.bin"]
    fn diff_real_org_passes_from_env() {
        use crate::runner::msg;

        let path = std::env::var("NORD_SHARC_ORG_IMAGE")
            .expect("set NORD_SHARC_ORG_IMAGE to core2_sram.bin");
        let image = std::fs::read(&path).expect("read NORD_SHARC_ORG_IMAGE");
        let cfg = PassConfig {
            trigger: 0xB_00E2,
            idle_pc: 0x12_5DDF,
            max_steps: 400_000,
            irq: None,
        };
        let one_voice = [msg::parse_msg("0:0xc10b8=0x1215fb").unwrap()];
        let eight_voices: Vec<_> = (0..8)
            .map(|k| msg::parse_msg(&format!("0:0x{:x}=0x1215fb", 0xc10b8 + k * 0x18)).unwrap())
            .collect();

        let mut pass0_steps = Vec::new();
        for (label, expect_osc, msgs) in [
            ("idle", false, &[][..]),
            ("1-voice", true, &one_voice[..]),
            ("8-voices", true, &eight_voices[..]),
        ] {
            let mut interp = Core::new(&image);
            let mut jit = Core::new(&image);
            for c in [&mut interp, &mut jit] {
                c.pc = 0x12_5C88;
                c.preset_device(0x282c_0380, 1);
                c.preset_device(0x3108_d008, 5);
            }
            diff_run(&mut interp, &mut jit, 100_000, 1_000)
                .unwrap_or_else(|e| panic!("{label} boot diff failed: {e}"));

            let mut left_idle = false;
            let mut hit_osc = false;
            for p in 0..8u32 {
                for m in msgs.iter().filter(|m| m.pass == p) {
                    msg::deliver(&mut interp, 0, m).unwrap();
                    msg::deliver(&mut jit, 0, m).unwrap();
                }
                let rep = diff_pass(&mut interp, &mut jit, &cfg, 1_000, true)
                    .unwrap_or_else(|e| panic!("{label} pass {p} diff failed: {e}"));
                if p == 0 {
                    pass0_steps.push(rep.steps);
                }
                assert_eq!(rep.final_pc, 0x12_5DDF, "{label} pass {p} final_pc");
                if rep
                    .coverage
                    .iter()
                    .any(|&(s, e)| s < 0x12_5DDF || e > 0x12_5DE3)
                {
                    left_idle = true;
                }
                if rep
                    .coverage
                    .iter()
                    .any(|&(s, e)| (s..=e).contains(&0x12_15FB))
                {
                    hit_osc = true;
                }
            }
            assert!(left_idle, "{label}: never left idle poll loop");
            assert_eq!(hit_osc, expect_osc, "{label}: 0x1215fb coverage mismatch");
        }
        assert!(
            pass0_steps[0] < pass0_steps[1] && pass0_steps[1] < pass0_steps[2],
            "expected idle < 1-voice < 8-voices steps, got {pass0_steps:?}"
        );
    }
}

//! Differential equivalence tests and 8/32-voice polyphony benchmarks comparing
//! [`Core::run`] (interpreter oracle) against [`Core::run_jit`] on multi-stage DSP workloads.

use super::dag::MODE1_CBUFEN;
use super::support::{core_with, CODE};
use super::Core;
use std::time::Instant;

/// Asserts full architectural state equivalence between `interp` and `jit`
/// over the given `sram_watch` normal-word range.
fn assert_core_state_eq(interp: &Core, jit: &Core, sram_watch: std::ops::Range<u32>, step: u64) {
    assert_eq!(jit.pc, interp.pc, "pc mismatch at step {step}");
    assert_eq!(jit.steps, interp.steps, "steps mismatch at step {step}");
    assert_eq!(jit.r, interp.r, "r mismatch at step {step}");
    assert_eq!(jit.s, interp.s, "s mismatch at step {step}");
    assert_eq!(jit.i, interp.i, "i mismatch at step {step}");
    assert_eq!(jit.m, interp.m, "m mismatch at step {step}");
    assert_eq!(jit.l, interp.l, "l mismatch at step {step}");
    assert_eq!(jit.b, interp.b, "b mismatch at step {step}");
    assert_eq!(jit.mr, interp.mr, "mr mismatch at step {step}");
    assert_eq!(jit.flags, interp.flags, "flags mismatch at step {step}");
    assert_eq!(
        jit.astat(0),
        interp.astat(0),
        "astat(0) mismatch at step {step}"
    );
    assert_eq!(jit.mode1, interp.mode1, "mode1 mismatch at step {step}");
    assert_eq!(jit.mode2, interp.mode2, "mode2 mismatch at step {step}");
    for addr in sram_watch {
        let o = ((addr - 0x9_0000) as usize) * 4;
        assert_eq!(
            jit.sram[o..o + 4],
            interp.sram[o..o + 4],
            "sram[0x{addr:x}] mismatch at step {step}"
        );
    }
}

/// Builds a realistic nested-loop multi-voice DSP program (per-voice state load,
/// 64-tap circular-buffer DualMove MAC inner loop, float gain/mix, fixed ALU, and SRAM store)
/// plus a total step budget for `voices * frames` outer passes.
fn build_polyphony_program(voices: u32, frames: u32) -> (Vec<u16>, u64) {
    // Outer loop: DO (+end) UNTIL LCE, LCNTR=R5 (voices * frames passes).
    // Body per voice-frame:
    //   R1 = DM(I2, M0)           (T3c: 0x9411)  -- load voice state / phase
    //   R3 = R1 + R2              (T2a: 0x013e, 0x0000, 0x1312)
    //   F2 = F0 * F4              (T2a: 0x013e, 0x0013, 0x0204)
    //   F2 = F0 + F2              (T2a: 0x013e, 0x0008, 0x1202)
    //   DO (+3) UNTIL LCE, LCNTR=R6 (T12a: 0x0d06, 0x0000, 0x0003) -- 64-tap FIR inner loop
    //     MRF = MRF + r0*r4 (ssf), r0=dm(i6,m3), r4=pm(i10,m10) (T1a: 0x2cc0, 0x921b, 0xc004)
    //   DM(I3, M0) = R3           (T3c: 0x9633)  -- store output sample
    let body_without_outer_hdr: Vec<u16> = vec![
        0x9411, // R1 = DM(I2, M0)
        0x013e, 0x0000, 0x1312, // R3 = R1 + R2
        0x013e, 0x0013, 0x0204, // F2 = F0 * F4
        0x013e, 0x0008, 0x1202, // F2 = F0 + F2
        0x0d06, 0x0000, 0x0003, // DO (+3) UNTIL LCE, LCNTR=R6
        0x2cc0, 0x921b, 0xc004, // MRF=MRF+r0*r4(ssf), r0=dm(i6,m3), r4=pm(i10,m10)
        0x9633, // DM(I3, M0) = R3
    ];
    let outer_end_rel = (3 + body_without_outer_hdr.len() - 1) as u16;
    let mut program = vec![0x0d05u16, 0x0000, outer_end_rel];
    program.extend_from_slice(&body_without_outer_hdr);

    let outer_passes = u64::from(voices) * u64::from(frames);
    // 1 outer DO + outer_passes * (6 outer body insns + 64 inner MACs)
    let total_steps = 1 + outer_passes * (6 + 64);
    (program, total_steps)
}

fn init_polyphony_core(program: &[u16], voices: u32, frames: u32, fast_float: bool) -> Core {
    let mut c = core_with(program);
    c.mode1 = MODE1_CBUFEN;
    c.fast_float = fast_float;
    c.r[5] = voices * frames;
    c.r[6] = 64;
    c.r[2] = 0x0001_0000;
    c.r[0] = 0.5f32.to_bits();
    c.r[4] = 0.25f32.to_bits();
    // I2: circular voice-state table (64 words at 0x9_0200), I3: output buffer at 0x9_0400
    (c.i[2], c.m[0], c.l[2], c.b[2]) = (0x9_0200, 1, 64, 0x9_0200);
    (c.i[3], c.l[3], c.b[3]) = (0x9_0400, 128, 0x9_0400);
    // I6 & I10: 64-tap circular FIR buffers
    (c.i[6], c.m[3], c.l[6], c.b[6]) = (0x9_0100, 1, 64, 0x9_0100);
    (c.i[10], c.m[10], c.l[10], c.b[10]) = (0x9_0300, 1, 64, 0x9_0300);
    for k in 0..64u32 {
        c.dm_write(0x9_0100 + k, 0x1000_0000 + (k << 16)).unwrap();
        c.dm_write(0x9_0200 + k, k * 0x0010_0000).unwrap();
        c.dm_write(0x9_0300 + k, 0x0800_0000 + (k << 15)).unwrap();
    }
    c
}

#[test]
fn jit_differential_multi_voice_every_n_steps() {
    let (program, total_steps) = build_polyphony_program(4, 4);
    let mut interp = init_polyphony_core(&program, 4, 4, false);
    let mut jit = init_polyphony_core(&program, 4, 4, false);

    // Step in prime-sized chunks (N = 7) across the entire nested-loop run
    // to catch any mid-block, loop-boundary, or resume-PC state divergence.
    let chunk = 7u64;
    let mut done = 0u64;
    while done < total_steps {
        let n = chunk.min(total_steps - done);
        assert_eq!(interp.run(n), None);
        assert_eq!(jit.run_jit(n), None);
        done += n;
        assert_core_state_eq(&interp, &jit, 0x9_0400..0x9_0420, done);
    }
    assert_eq!(jit.pc, CODE + program.len() as u32);
    assert!(jit.loops.is_empty());
}

#[test]
#[ignore = "polyphony benchmark: run with cargo test --release -p nord-sharc --features jit --lib bench_jit_polyphony_8_and_32_voices -- --ignored --nocapture"]
fn bench_jit_polyphony_8_and_32_voices() {
    const FRAMES: u32 = 128; // 2.667 ms audio buffer @ 48 kHz
    const RT_BUDGET_MS: f64 = 128.0 / 48.0;

    for voices in [8u32, 32u32] {
        let (program, total_steps) = build_polyphony_program(voices, FRAMES);

        let mut interp = init_polyphony_core(&program, voices, FRAMES, false);
        let t0 = Instant::now();
        assert_eq!(interp.run(total_steps), None);
        let dt_interp = t0.elapsed();

        // Warm up JIT block compilation once, then measure a full 128-frame buffer.
        let mut jit_exact = init_polyphony_core(&program, voices, FRAMES, false);
        assert_eq!(jit_exact.run_jit(total_steps), None);
        assert_core_state_eq(&interp, &jit_exact, 0x9_0400..0x9_0480, total_steps);

        let mut jit_exact_timed = init_polyphony_core(&program, voices, FRAMES, false);
        let _ = jit_exact_timed.run_jit(10);
        let t1 = Instant::now();
        assert_eq!(jit_exact_timed.run_jit(total_steps - 10), None);
        let dt_jit = t1.elapsed();

        let mut jit_fast = init_polyphony_core(&program, voices, FRAMES, true);
        let _ = jit_fast.run_jit(10);
        let t2 = Instant::now();
        assert_eq!(jit_fast.run_jit(total_steps - 10), None);
        let dt_fast = t2.elapsed();

        let interp_ms = dt_interp.as_secs_f64() * 1e3;
        let jit_ms = dt_jit.as_secs_f64() * 1e3;
        let fast_ms = dt_fast.as_secs_f64() * 1e3;
        eprintln!(
            "polyphony {voices}v x {FRAMES}f ({total_steps} steps, budget {RT_BUDGET_MS:.2} ms): \
             interp {interp_ms:.2} ms ({:.2} Msteps/s) | \
             jit-exact {jit_ms:.2} ms ({:.2} Msteps/s, {:.2}x RT) | \
             jit-fast_float {fast_ms:.2} ms ({:.2} Msteps/s, {:.2}x RT)",
            total_steps as f64 / 1e6 / dt_interp.as_secs_f64(),
            total_steps as f64 / 1e6 / dt_jit.as_secs_f64(),
            RT_BUDGET_MS / jit_ms,
            total_steps as f64 / 1e6 / dt_fast.as_secs_f64(),
            RT_BUDGET_MS / fast_ms,
        );
    }
}

#[test]
#[ignore = "multicore polyphony benchmark: run with cargo test --release -p nord-sharc --features jit --lib bench_multicore_4x8_32_voices -- --ignored --nocapture"]
fn bench_multicore_4x8_32_voices() {
    const FRAMES: u32 = 128; // 2.667 ms @ 48 kHz
    const RT_BUDGET_MS: f64 = 128.0 / 48.0;
    let (program, per_core_steps) = build_polyphony_program(8, FRAMES);

    let cores: Vec<Core> = (0..4)
        .map(|_| {
            let mut c = init_polyphony_core(&program, 8, FRAMES, false);
            // Warm up JIT block compilation before timing parallel quantum execution.
            let _ = c.run_jit(10);
            c
        })
        .collect();
    let mut sched = crate::scheduler::MultiCoreScheduler::new(cores);
    let t0 = Instant::now();
    assert_eq!(sched.run_quantum(per_core_steps - 10, true), Ok(()));
    let dt = t0.elapsed();
    let ms = dt.as_secs_f64() * 1e3;
    let total_steps = per_core_steps * 4;
    eprintln!(
        "multicore 4x8=32v x {FRAMES}f ({total_steps} total steps, budget {RT_BUDGET_MS:.2} ms): \
         jit-exact {ms:.2} ms ({:.2} Msteps/s agg, {:.2}x RT)",
        total_steps as f64 / 1e6 / dt.as_secs_f64(),
        RT_BUDGET_MS / ms,
    );
}

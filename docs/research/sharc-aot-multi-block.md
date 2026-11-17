# Multi-block AOT of the ORG Core2 real pass: coverage and gain curve

Checked: 2026-11-16
Sources: `core2_sram.bin` (ORG Core2 image, local-only); `docs/research/data/org-system-load-core2.script` (912 ARM messages, flattened to pass 0) plus `--msg 0:0x3e021=1` (EP DMA stub); `crates/nord-sharc/src/core/{aot_lower,aot_emit,aot_hot,aot_glue,aot_state,aot_pe}.rs`, `runner/{aot_rank,aot_hot}.rs`, `sharc_jit_diff --aot-rank/--aot-top/--bench`, `sharc-aot --ranges`; baselines `sharc-aot-hot-block.md` (#467), `sharc-aot-native-float.md` (#469); D118 (whole-pass AOT question), `org-system-run.md` (#468 real pass).
Question: if the hot straight-line regions of the REAL pass (not the poke proxy) are lowered ahead of time, how many steps and how much interpreter time do they cover (N = 1..54 regions), is every N bit-exact against `Core::run`, and what does the whole pass gain? Scope: no whole-pass compiler, no scheduler, no ARM work, `pe` tier only, flag off = interpreter.

## Result in one paragraph

Coverage is high, the gain is none. 54 regions cover 98.5% of the pass steps (84% realized at run time, see 3) and every N is bit-exact over boot + 300 passes with full state + SRAM compared. The whole pass is not faster: x0.95-0.99 (slower) at every N. The reason is structural: of the 6 280 steps per pass only ~3.75% are ops the lowering can run natively (compute fields, immediate loads, register moves, shift immediates). The other ~95% inside the regions are memory moves (`Move::Other` 42%, `DirectMem` 12%, `DagMem` 10%, ...), which still run through `Core::step`. Without a native lowering of those the AOT removes nothing. The PRD budget (46.08 us/pass) stays ~20x away on this host (interpreter ~0.9 ms/pass).

## 1. Ranking of the real pass

`sharc_jit_diff ... --aot-rank 80:3` runs boot (100 000 steps), 16 warmup and 300 measured passes on the interpreter, counts and times every executed pc (a clock read around each step, so shares are meaningful and absolute times are inflated), cuts the executed code into straight-line chains (no jump lands inside, every step falls through) and chains into *runs* of instructions the lowering accepts.

- Pass: 6 279.6 steps, 1 101 us profiled (ASSUMED: ~1.1x clock overhead; the unprofiled pass is ~0.9 ms).
- 54 runs of >= 3 instructions; shorter runs: 106 steps/pass.
- Top runs (cumulative share of pass steps / profiled time): N=1 32.7% / 26.0%, N=2 61.7% / 57.7%, N=4 87.6% / 88.5%, N=8 92.5% / 92.7%, N=16 94.8% / 94.6%, N=32 95.9% / 96.4%, N=54 96.9% / 97.4%.

| # | Region | Insns | Entries/pass | Steps/pass | % steps | % time |
|---|---|---:|---:|---:|---:|---:|
| 1 | `0x12566e..0x1257af` | 135 | 15.2 | 2 055.6 | 32.7 | 26.0 |
| 2 | `0x125ead..0x127128` | 1 849 | 1.0 | 1 818.2 | 29.0 | 31.7 |
| 3 | `0x127cda..0x128695` | 957 | 1.0 | 957.0 | 15.2 | 19.1 |
| 4 | `0x1273c9..0x127a7f` | 673 | 1.0 | 673.0 | 10.7 | 11.8 |
| 5 | `0x1271b8..0x12730c` | 158 | 1.0 | 158.0 | 2.5 | 2.6 |
| 6 | `0x125de3..0x125e6b` | 57 | 1.0 | 57.0 | 0.9 | 0.7 |
| 7 | `0x120bf7..0x120bfc` | 3 | 16.0 | 48.0 | 0.8 | 0.5 |
| 8 | `0x12538c..0x12539a` | 7 | 6.0 | 42.0 | 0.7 | 0.4 |

The remaining 46 runs are <= 48 steps each (full list: command below). The hot block of #467 (`0x125021..=0x1250aa`) is not on this list: it belongs to the voice path the real pass does not run.

## 2. What the lowering does

A region becomes a list of items (`aot_lower.rs`):

- `Batch`: >= 3 consecutive native ops (unconditional compute fields, `Rn = imm`, `Rn = Rm [, compute]`, shift immediate, NOP; only when the register-resident kernels model them). The generated code runs the batch on PEx, then on PEy when SIMD is on (the scalar-mode case matters: part of the real pass runs with SIMD off, first attempt missed those entries), exactly like #467.
- `Interp`: every other straight-line instruction. The generated code calls `Core::aot_item`, which sets the pc, runs ONE `Core::step` (out of line, so a 1 849-instruction region stays small) and re-checks a signature (SIMD and IRPTEN bits of MODE1, IMASK, LIRPTL mask, interrupt `due`, window/idle, `code_gen`, loop and call stack depth, delayed branch, pc). If it changed, the region returns early with the exact interpreter state; the interpreter continues.
- Control flow (jumps, calls, returns, `DO`, stack ops, `IDLE`, ISA mode) ends a region and is never lowered; entry is by a start-pc bitmap tested with one compare (`aot_span == 0` while off), decided per region, not per step.

Guard at entry (`aot_ready`): no delayed branch, no trace, no interrupt window/idle, `steps + n <= irq.due`, no loop end inside the region, no enabled arithmetic-exception interrupt, code bytes equal to the generated snapshot. The step limit of the caller must cover the whole region.

Unsupported shapes, steps per pass (from the profile): ending a region: `Flow::JumpIndirect` 38.0, `Flow::Return` 38.0, `Flow::JumpDirect` 9.9, `Flow::DoUntilCounter` 1.0, `StackOp` ~0 (together 2.0%). Run as interpreter items (NOT lowered natively): `Move::Other` 2 659.3 (42.3%), `Move::DirectMem` 733.5 (11.7%), `Move::DagMem` 639.5 (10.2%), `Move::RegMove` (non-native forms) 294.4 (4.7%), `Move::RegSwap` 276.9 (4.4%), `ComputeBoth` (ops without a `pe` kernel) 187.9 (3.0%), `Move::LoadImm` (non-data registers) 187.0 (3.0%), `ShiftImm` 71.0 (1.1%), `ComputeCond` 24.6 (0.4%). Computed jumps, calls/returns, `DO` loops and interrupts are never inside a region; no interrupt was taken inside an enabled region in the measured passes (the `due` guard refuses entry, and a latch inside a region triggers the bail-out).

## 3. Exactness, N = 1..54 (strict, boot + 300 passes)

Command (below): boot 100 000 steps + 300 passes of the real pass, interpreter vs AOT in lockstep, full strict state (`state_mismatch`: registers of both PEs, flags, multiplier results, float extensions, ASTAT extras, STKY, MODE1/2, interrupt latches, stacks) plus SRAM compared every 20 000 steps, no `--lenient`. VERIFIED for these inputs only: every N printed `AOT DIFF OK` (1 854 395 steps matched 1:1); no divergence to report. The comparison chunk had to grow from 1 000 to 20 000 steps because a region longer than the chunk can never enter (limit check): the first divergence would be localized by re-running a failing N with a smaller chunk.

| N | Pass steps covered (run time) | Of them native | Calls | Bails | Misses |
|---:|---:|---:|---:|---:|---:|
| 1 | 31.39% | 2.09% | 4 312 | 0 | 0 |
| 2 | 60.61% | 2.90% | 4 606 | 1 | 0 |
| 4 | 74.76% | 2.90% | 5 206 | 601 | 0 |
| 8 | 79.44% | 3.29% | 12 406 | 1 201 | 0 |
| 16 | 82.07% | 3.75% | 14 874 | 1 501 | 0 |
| 32 | 82.96% | 3.75% | 19 674 | 2 401 | 0 |
| 54 | 83.96% | 3.75% | 25 671 | 2 401 | 0 |

Run-time coverage (84%) is below the ranked one (97%): a region bails when an interpreter item changes the interrupt controller's next event (`due`, e.g. an `IRPTL` write at `0x125ed9`) or other guarded state, and the rest of that region then runs on the interpreter (re-entry at an item boundary is not implemented). Misses were 0 only after the guard accepted scalar mode (SIMD off).

## 4. The curve (whole pass, same run, host shared)

Interleaved pass by pass in one process, 5 rounds, median (`target/aot/curve.py`, local scratch). Ratios only: absolute ms drift between runs (other sessions share the host; one round at N=2 and N=4 hit 0.69 and 0.78).

| N | Steps in regions (% pass) | Interp time in regions (profile, cum) | Whole-pass speedup (median, min-max) | Interp / AOT ms per pass |
|---:|---:|---:|---:|---:|
| 1 | 34.4 | 26.0% | x0.987 (0.982-1.018) | 0.879 / 0.892 |
| 2 | 63.8 | 57.7% | x0.962 (0.690-0.995) | 0.951 / 0.992 |
| 4 | 89.8 | 88.5% | x0.962 (0.782-0.989) | 0.967 / 0.993 |
| 8 | 94.7 | 92.7% | x0.966 (0.942-0.970) | 0.912 / 0.943 |
| 16 | 96.4 | 94.6% | x0.955 (0.930-0.972) | 0.892 / 0.924 |
| 32 | 97.5 | 96.4% | x0.971 (0.870-0.976) | 0.975 / 1.024 |
| 54 | 98.5 | 97.4% | x0.947 (0.935-0.969) | 1.055 / 1.089 |

("Steps in regions" is the block-timing sweep: all steps that start at an enabled region entry. It counts bail remainders, so it is above the 84% of section 3.) The region itself is ~1.08-1.13x faster than interpreting it (block sweep, e.g. N=54: 1 002 -> 888 us), yet the whole pass is ~3-5% slower: the AOT core pays the entry bitmap test on every step of the ~30 000-parcel window, the bail-out remainders and the out-of-line item call + signature check for each of ~5 000 items, while saving only the decode-cache lookup of the 3.75% native steps. A per-region I-cache cost of the 550 KB generated source is possible but not measured (ASSUMED).

Extrapolation to the budget, ASSUMED: if the native share rose from 3.75% to 100% of the steps in the regions at the `pe`-kernel speed seen in #467/#469 (block x3.9-9.3), the pass would approach the Amdahl bound of the covered share (up to x20-50 at 97% coverage in the best case, ~ 20-40 us/pass on this host, i.e. around the 46.08 us budget). Nothing here measures that: it needs native memory moves (DM/PM/DAG, ~64% of region steps) which are not implemented, and the real pass has bails and non-region steps (2.6% + 106 steps + gaps) that cap it.

## Implications

- Straight-line coverage is not the obstacle: 4-8 regions already hold 90-95% of the pass. The obstacle is the instruction mix: native lowering of the memory-move family (`Move::Other`, `DirectMem`, `DagMem`, `RegMove`/`RegSwap` incl. system registers) is the prerequisite for any gain, and it must model the interpreter's SIMD twin-write and DM/PM bank semantics bit-exactly.
- Hybrid interpreter items are a correctness tool (they let a region contain any straight-line op exactly), not a speed tool: an out-of-line `step` per item costs more than the interpreter's own loop.
- Exact-diff chunking has to be >= the longest region.
- Do not enable regions on code that changes mode or the interrupt controller inside the region without the bail-out; the signature is the cheap way.

## Follow-ups

- Native lowering of `DirectMem`/`DagMem` loads and stores (bank/PM-DM layout, SIMD twin addresses) and of `RegMove`/`RegSwap`; then re-measure this curve. Compare against whole-pass compilation (D118).
- Re-entry at an item boundary after a bail-out (coverage 84% -> ~97%), and entry from a jump target inside a region.
- Scalar-mode (SIMD off) and `DO` loops inside regions.
- Measure instruction-cache cost of large generated modules.

## Commands

```bash
IMG=<path to core2_sram.bin>                      # local-only, never committed
ROOT=$PWD
MSGS=$(awk '{printf "--msg 0:%s=%s ", $2, $3}' flat.script)   # flat.script = org-system-load-core2.script reduced to pass 0 (PASS W1 W2 -> 0 W1 W2)
RUN="--pc 0x125c88 --boot 100000 --passes 300 --warmup 16 --dev 0x282c0380=1 --dev 0x3108d008=5 --trigger 0xB00E2 --idle-pc 0x125DDF --msg-base 0 --msg 0:0x3e021=1 $MSGS"
cargo build --release -p nord-sharc --features jit                       # stub build
./target/release/sharc_jit_diff.exe $IMG $RUN --aot-rank 80:3 > rank.txt  # ranking; last line "ranges A:B,C:D,..."
RANGES=$(grep '^ranges ' rank.txt | cut -d' ' -f2)
./target/release/sharc-aot.exe $IMG --ranges "$RANGES" --tier pe --out "$ROOT/target/aot/multi_pe.rs"
NORD_SHARC_AOT_HOT="$ROOT/target/aot/multi_pe.rs" cargo build --release -p nord-sharc --features jit   # ~2 min
for N in 1 2 4 8 16 32 54; do ./target/release/sharc_jit_diff.exe $IMG $RUN --chunk 20000 --aot-top $N [--aot-detail]; done   # strict diff
./target/release/sharc_jit_diff.exe $IMG $RUN --aot-top 16 --bench        # whole-pass ratio (alternating passes)
```

The generated file and the image stay under `target/` / local paths and are never committed.

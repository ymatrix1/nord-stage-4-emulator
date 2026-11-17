# AOT prototype of the ORG Core2 hot block `0x125021..=0x1250aa` (measured)

Checked: 2026-11-16
Sources: `core2_sram.bin` (ORG Core2 image, local-only, extracted as in `sharc-jit-org-profile.md`); `crates/nord-sharc/src/core/{aot_lower,aot_hot,aot_fast,aot_pe}.rs`; `crates/nord-sharc/src/runner/aot_hot.rs`; `sharc_jit_diff --aot-hot`; budget numbers from `sharc-jit-org-profile.md` (one pass = 46.08 µs).
Question: how much real wall time does ahead-of-time compiled Rust for one hot straight-line block save inside the interpreter pass loop, with a strict 1:1 state match against `Core::run`? The numbers replace projections in the single-core ceiling study (#465).

## Method

- `sharc-aot IMAGE START LAST --hot --tier interp|fast|pe --out FILE` lowers the 70 instructions (VISA short words `0x125021..=0x1250aa`, straight line, exit pc `0x1250ad`) and writes a Rust module. The module embeds the block's code words; the runtime refuses to run it if the image differs (`aot_code_matches`).
- `crates/nord-sharc/build.rs` includes that file when `NORD_SHARC_AOT_HOT=<abs path>` is set at build time; without it a stub (`block() -> None`) is built. The generated file is derived from firmware and is never committed.
- `Core::enable_aot_hot(start, last)` arms the block; `Core::aot_try` runs it when `pc == start`, no interrupt is pending or enabled that the block could latch, the step limit allows 70 steps, and the code still matches. Otherwise the interpreter steps as before. Flag off (`--aot-hot` absent): no hook runs, `aot_hot` is `None`.
- Tiers (ablation):

| Tier | What the block does per call |
|---|---|
| `interp` | the interpreter's own compute implementation per op, PE-batched (no decode, no per-step PE swap) |
| `fast` | `interp` plus native-width `u64` float add/sub/mul/FLOAT kernels (`aot_fast.rs`, zero/denormal operands handled in cold paths) |
| `pe` | each PE's registers, float extensions, ALU/shifter flags, ASTAT bits and STKY bits held in a local `Pe`; one load, one store, interrupts latched once (`aot_pe.rs`) |

## Differential result

`sharc_jit_diff --aot-hot 0x125021:0x1250aa` runs boot (100 000 steps) and 300 passes (16 warmup) on two cores, one with `Core::run` and one with the block, and compares the full strict state every 1000 steps: registers, flags of both PEs, MR, effective float extensions, ASTAT extra, STKY, USTAT, IRPTL/LIRPTL, loop and call depth, plus SRAM. No `--lenient`. First divergence aborts.

| State | Tier `pe` result |
|---|---|
| idle | `AOT DIFF OK` boot + 300 passes (1 279 499 steps) matched 1:1, block calls 0 |
| 1 voice | `AOT DIFF OK` (1 342 799 steps) matched 1:1, 300 diff-run block calls |
| 8 voices | `AOT DIFF OK` (1 785 899 steps) matched 1:1, 4200 diff-run block calls |

Tiers `interp` and `fast` also matched 1:1 in all three states. `misses` in the diff run come from the 1000-step chunk boundaries (a chunk ending inside the block falls back to stepping); the benchmark has `misses=0`.

VERIFIED: the three results above by the commands below (status of the per-op unit test: `pe_ops_equal_interpreter_ops_on_random_simd_state`, randomized SIMD state, all ops of the block, equal to the interpreter's state).

## Timing (tier `pe`, release, warmed up, 300 passes)

Whole pass: interpreter and AOT run interleaved, pass by pass, same core state. Block time: separate sweep with a clock read around each block (interpreter: the 70 steps at `pc == start`; AOT: the generated call). Machine was shared with other sessions, so three repeats follow.

| State | Steps/pass | Block calls/pass | Block share of pass (steps) | Block share of pass (time) | Block interp → AOT (µs/pass) | Block speedup |
|---|---:|---:|---:|---:|---:|---:|
| idle | 4265 | 0 | 0% | 0% | – | – |
| 1 voice | 4476 | 2 (140 steps) | 3.1% | 2.9% | 10.5 → 2.2 | ×4.85 |
| 8 voices | 5953 | 16 (1120 steps) | 18.8% | 10.7% | 59.5 → 13.2 | ×4.49 |

Whole pass, mean (worst) ms and Msteps/s, per run of three (run 0 is the first table; runs 1–3 repeated):

| State | Run | Interpreter | AOT | Speedup |
|---|---|---|---|---:|
| idle | 0 | 0.355 (0.845) 12.01 | 0.371 (0.911) 11.49 | ×0.957 |
| idle | 1 / 2 / 3 | 0.603 / 0.345 / 0.417 | 0.614 / 0.364 / 0.438 | ×0.982 / 0.948 / 0.951 |
| 1 voice | 0 | 0.359 (1.084) 12.46 | 0.363 (1.190) 12.32 | ×0.989 |
| 1 voice | 1 / 2 / 3 | 0.471 / 0.428 / 0.747 | 0.477 / 0.431 / 0.814 | ×0.986 / 0.994 / 0.918 |
| 8 voices | 0 | 0.557 (1.518) 10.68 | 0.521 (1.644) 11.43 | ×1.070 |
| 8 voices | 1 / 2 / 3 | 0.515 / 0.458 / 0.674 | 0.474 / 0.423 / 0.631 | ×1.086 / 1.081 / 1.067 |

Reading: the block itself is ×3.7–4.9 faster per call (about 1.1 µs/call with 1 voice, 0.8 µs/call with 8 voices, against 5.2 and 3.7 µs interpreted), but it is only 2.9% / 10.7% of pass time, so the whole pass gains about 7–9% with 8 voices and nothing measurable with 1 voice. Idle loses 2–5%: the per-step `aot_try` check costs more than the block ever returns when it is never called. Run-to-run noise is large (interpreter mean differs by up to 2× between runs), so only the ratios inside one run are meaningful.

Tier ablation (same commands, tier changes the generated file only; one run each, noisy):

| Tier | Block speedup 1 voice / 8 voices | Whole-pass 8 voices |
|---|---:|---:|
| `interp` | ×2.17 / ×1.8–2.1 | ×1.056 |
| `fast` | ×2.01 / ×2.39 | ×1.088 |
| `pe` | ×3.0–4.9 / ×3.7–4.7 | ×1.067–1.086 |

Equivalent step rate of the block alone (70 steps per call): interpreter about 13 Msteps/s (1 voice) to 19 Msteps/s (8 voices); `pe` about 63 and 85 Msteps/s. ASSUMED: the real-time need is 92.6 / 97.1 / 129.2 Msteps/s (idle / 1 voice / 8 voices, `sharc-jit-org-profile.md`); even this best case does not reach it for the block alone.

## Commands

```bash
IMG=<path to core2_sram.bin>
cargo build --release -p nord-sharc --features jit          # builds sharc-aot, sharc_jit_diff (stub block)
./target/release/sharc-aot.exe $IMG 0x125021 0x1250aa --hot --tier pe --out "$PWD/target/aot/org_hot_125021_pe.rs"
NORD_SHARC_AOT_HOT="$PWD/target/aot/org_hot_125021_pe.rs" cargo build --release -p nord-sharc --features jit
COMMON="--pc 0x125c88 --boot 100000 --passes 300 --warmup 16 --dev 0x282c0380=1 --dev 0x3108d008=5 --trigger 0xB00E2 --idle-pc 0x125DDF --chunk 1000 --msg-base 0 --aot-hot 0x125021:0x1250aa"
# idle:
./target/release/sharc_jit_diff.exe $IMG $COMMON --bench
# 1 voice:
./target/release/sharc_jit_diff.exe $IMG $COMMON --msg 0:0xc10b8=0x1215fb --bench
# 8 voices (k = 0..7, 0xc10b8 + k*0x18):
./target/release/sharc_jit_diff.exe $IMG $COMMON $(for k in 0 1 2 3 4 5 6 7; do printf -- '--msg 0:0x%x=0x1215fb ' $((0xc10b8 + k*0x18)); done) --bench
```

Without `--bench` only the strict diff runs. Without `--aot-hot` the output is that of main.

## Implications

- Per-block AOT works and is exact (VERIFIED above), and the register-resident tier is ×3.7–4.9 on the block. The gain is bounded by how much of the pass the block is: at 8 voices 10.7% of time, so even an infinitely fast block caps the pass at about ×1.12 (Amdahl, computed from the table).
- A pass is 4–6 k steps of mostly different code. To approach the 46 µs budget the interpreter's remaining ~90% (the measured 0.4–0.6 ms pass) needs the same treatment, i.e. whole-pass AOT or a JIT that reaches the `pe` tier's cost (about 12–19 ns/step → about 0.8–1.2 µs per 70 steps). ASSUMED: the `pe` per-step cost transfers to other straight-line float blocks; not measured here.
- Blocks need an entry guard, interrupt rules and chunk-boundary handling; those are cheap here (`misses=0` in the benchmark) but cost 2–5% when the block is never called, so a block hook must be installed only when its block is hot.

## Follow-ups

- Run the same lowering on the other top blocks of the `--profile-jit` ranking and measure how the total adds up.
- Remove the per-step hook cost when idle (check `pc == start` only at jump targets or install a breakpoint-style hook).
- Split `aot_pe.rs`, `aot_fast.rs`, `aot_hot.rs` and `aot_lower.rs` (each is 400–470 lines including tests) once the design settles.
- Benchmark on a quiet machine; numbers above are from a shared host.

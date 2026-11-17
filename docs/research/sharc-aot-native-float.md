# Host-float tiers for the ORG Core2 hot block `0x125021..=0x1250aa` (exact-guarded and approximate)

Checked: 2026-11-16
Sources: `core2_sram.bin` (ORG Core2 image, local-only); `crates/nord-sharc/src/core/{aot_native,aot_pe,aot_hot,aot_lower}.rs`; `crates/nord-sharc/src/runner/{aot_hot,aot_audio}.rs`; `sharc_jit_diff --aot-hot [--aot-audio] [--bench]`; baseline `sharc-aot-hot-block.md` (#467); the guarded JIT path `jit.rs` `emit_exact_float_op` (#457).
Question: (1) does computing the block's float ops on the host FPU, only where the 40-bit result is provably exact, beat the bit-exact `pe` tier? (2) how far does an approximate `f32` tier (extension byte dropped, denormals flushed) move the audio, and how much faster is it? (3) what does the per-step hook cost, and can it be made free? The exact path stays the oracle; the approximate tier is opt-in, a measurement only, never the default.

## Tiers added

| Tier (`sharc-aot --tier`) | Float kernels | Exactness |
|---|---|---|
| `pe` (#467) | `u64` integer kernels (`aot_fast.rs`) | bit-exact |
| `guarded` | host `f64` add/sub/mul/`FLOAT` when the result is provably exact (normal operands, zero extension byte, no TRUNC/RND32, exponent gap <= 28, result tail zero, exponent in range), else the `pe` kernel | bit-exact |
| `approx` | host `f32` add/sub/mul/`FLOAT` with the extension byte dropped, denormals flushed in and out; flags AZ/AN/AV/AU/MN/MV/MU derived from the result; other ops (CLIP, COPYSIGN, RECIPS, fixed point) unchanged | NOT bit-exact |

Hook: `Core::aot_pc` holds the block entry (`u32::MAX` when off); `aot_try` is one inlined compare per step and the rest is out of line (`aot_enter`).

## 1. Exact-guarded native float (bit-exact)

VERIFIED bit-exact: `sharc_jit_diff --aot-hot 0x125021:0x1250aa` with the `guarded` block, 100 000 boot steps + 300 passes (16 warmup), full strict state + SRAM every 1000 steps, no `--lenient`, in all three states: idle `AOT DIFF OK` (1 279 499 steps), 1 voice `AOT DIFF OK` (1 342 799), 8 voices `AOT DIFF OK` (1 785 899). Per-kernel equality with `fast_*` on 60 000 randomized operand pairs: `guarded_kernels_equal_fast_kernels_and_usually_apply`, `guarded_from_int_equals_fast_from_int`.

Speed against `pe` (alternating runs, median of 5, release, shared machine; ratios only, absolute µs drift between runs):

| State | Tier | Whole-pass speedup (median, min-max) | Block interp → AOT (µs/pass) | Block speedup |
|---|---|---:|---:|---:|
| 1 voice | `pe` | x1.025 (0.994-1.029) | 13.0 → 3.3 | x3.94 |
| 1 voice | `guarded` | x1.013 (0.990-1.018) | 13.1 → 5.0 | x2.62 |
| 8 voices | `pe` | x1.105 (1.096-1.132) | 88.9 → 23.1 | x3.85 |
| 8 voices | `guarded` | x1.089 (1.076-1.095) | 86.6 → 27.8 | x3.12 |

Result: `guarded` is slower than `pe`. The guard rarely holds on real data. Hit counts of the guard in the 8-voice benchmark passes (temporary counter patch, not committed, so ASSUMED): add/sub 62 926 of 337 344 (19%), multiply 59 168 of 449 792 (13%), `FLOAT` 100%. Operands have full 24-bit significands or non-zero extension bytes, so the product or sum needs rounding and the kernel falls back after paying for the guard. The integer kernels already cost 4-7 ns, so there is little for the host FPU to save.

## 2. Approximate `native` (`approx`) tier

Tolerance, written in `aot_audio.rs` (`TOLERANCE`) before the first run:

| Criterion | Limit |
|---|---|
| SNR of the whole run, approximate vs exact output | >= 100 dB |
| worst single-pass SNR | >= 80 dB |
| largest band-energy difference (32 bands, bands within 80 dB of the peak) | <= 0.1 dB |
| passes with a different step count (control flow) | 0 |

Method: the exact interpreter and a core running the `approx` block get the same boot, messages and 316 passes (16 warmup + 300); nothing but the buffers is compared. Buffers: the channel A records `0xC1F96` (432 words) and B records `0xC2942` (416 words), read as `f32`. The pass buffers `DM(0xB2EF4..)` that `--buf` defaults to hold no data in this configuration (first attempt: peak 0, vacuous), so they are not used. The buffers mix floats with integers and flags, so all metrics use the *affected words*: addresses whose word differs bitwise in at least one pass (conservative: unaffected words would only raise SNR).

Result (`target/aot/audio3.sh`, one deterministic run; same state input for both cores):

| State | Block calls | Differing words (all passes) | Affected addresses | SNR | Worst pass | Windows (4 equal parts) | Max abs / max rel error | Spectrum | Step mismatches | Verdict |
|---|---:|---:|---:|---:|---|---|---|---:|---:|---|
| idle | 0 | 0 | 0 | exact | - | - | - | - | 0 | within |
| 1 voice | 632 | 0 | 0 | exact | - | - | - | - | 0 | within |
| 8 voices | 5056 | 9372 | 32 (`0xc1ff5..0xc2140`) | 137.6 dB | 77.0 dB (pass #0, energy 4.9e-9); 103.8 dB after warmup | 112.5 / 121.0 / 124.9 / 141.7 dB | 7.5e-9 / 9.2e-4 (peak 0.12) | 0.0000 dB | 0 | OUTSIDE (worst pass < 80 dB) |

Reading, without changing the tolerance after the fact:
- With one voice the `f32` kernels give bit-identical words and register state in all 316 passes, so that case carries no information about error. With 8 voices 32 words are affected.
- The 8-voice run fails the pre-set worst-pass criterion by 3 dB, only in the start-up pass #0 (reference energy 4.9e-9, absolute error 7.5e-9); the worst measured pass after warmup is 103.8 dB. Max relative error 9.2e-4 on one word is a different order than the SNR suggests (ASSUMED: cancellation in a subtraction of nearly equal values).
- Spectrum: 0.0000 dB in 32 bands over the 8192-sample stream of affected words. ASSUMED limited value: the stream interleaves records, not a single audio channel.

Checks asked for:

| Check | Finding |
|---|---|
| Does the firmware read float flags/STKY in the block? | The block is 70 straight-line unconditional steps; it ends at `0x1250aa`, then `0x1250ad..0x1250af` are three `FloatMul`s and `0x1250b0` an unconditional return (`sharc-aot IMG 0x1250ad 0x1250c0 --ops`). Flags the block sets reach the caller; I did not trace the callers. Evidence for "not read in a way that matters": 0 step-count mismatches and 0 passes with a differing full state (flags, STKY, registers) at pass end, in 316 passes x 3 states (VERIFIED by the command below for these inputs only). The `approx` tier keeps AZ/AN/AV/AU/MN/MV/MU and their sticky bits, it does not drop them. |
| Denormals | In and out flushed to zero like the exact tier (unit test `approx_flushes_denormals_and_flags_overflow`). Block entry registers often hold integers whose bit patterns are `f32` denormals (they are fixed-point data, not floats); no divergence resulted. |
| Error growth over passes | None seen: window SNR 112.5 / 121.0 / 124.9 / 141.7 dB, the error does not accumulate over 316 passes. ASSUMED for longer runs and other feedback paths (reverb, delay), which this block does not drive. |
| Float compares near thresholds | 0 step mismatches in 316 passes x 3 states, so no branch flipped. Not a proof; CLIP/COPYSIGN inside the block still see `f32`-rounded inputs. |

Speed (same alternating runs as above):

| State | Whole-pass speedup (median, min-max) | Block interp → AOT (µs/pass) | Block speedup | Block share of pass time |
|---|---:|---:|---:|---:|
| 1 voice | x1.020 (1.011-1.030) | 10.9 → 1.1 | x9.91 | 3.0% |
| 8 voices | x1.138 (1.093-1.154) | 67.1 → 7.2 | x9.32 | 12.9% |

The block is now almost free; the whole pass reaches x1.14, which is the Amdahl limit for a block that is 12.9% of the pass (1 / (1 - 0.129) = x1.148, computed). Further block work cannot move the pass; whole-pass compilation can.

## 3. Hook cost

| State | `pe` before (#467, per-step `aot_try` call) | `pe` now (one inlined compare) |
|---|---|---|
| idle | x0.95-0.98 | x1.008 (1.002-1.028) |

Idle with the flag on is now neutral (also `guarded` x1.003, `approx` x1.004, `fast` x0.997). The decision "block entry once per block" is implemented as a single `pc == aot_pc` compare; a per-pass arming step is not needed.

## Commands

```bash
IMG=<path to core2_sram.bin>
cargo build --release -p nord-sharc --features jit                       # stub build, builds sharc-aot
for T in pe guarded approx fast; do
  ./target/release/sharc-aot.exe $IMG 0x125021 0x1250aa --hot --tier $T --out "$PWD/target/aot/hot_$T.rs"
  NORD_SHARC_AOT_HOT="$PWD/target/aot/hot_$T.rs" cargo build --release -p nord-sharc --features jit
  cp target/release/sharc_jit_diff.exe target/aot/jd_$T.exe
done
COMMON="--pc 0x125c88 --boot 100000 --passes 300 --warmup 16 --dev 0x282c0380=1 --dev 0x3108d008=5 --trigger 0xB00E2 --idle-pc 0x125DDF --chunk 1000 --msg-base 0 --aot-hot 0x125021:0x1250aa"
M8=$(for k in 0 1 2 3 4 5 6 7; do printf -- '--msg 0:0x%x=0x1215fb ' $((0xc10b8 + k*0x18)); done)
# exact tiers (pe, guarded): strict lockstep diff, idle / 1 voice / 8 voices
target/aot/jd_guarded.exe $IMG $COMMON                                   # idle
target/aot/jd_guarded.exe $IMG $COMMON --msg 0:0xc10b8=0x1215fb          # 1 voice
target/aot/jd_guarded.exe $IMG $COMMON $M8                               # 8 voices
# approximate tier: audio comparison against the exact interpreter (+ --bench for speed)
target/aot/jd_approx.exe $IMG $COMMON $M8 --aot-audio --buf 0xC1F96:432:f32 --buf 0xC2942:416:f32
target/aot/jd_approx.exe $IMG $COMMON $M8 --aot-audio --bench
```

Timing medians: five alternating rounds of the three states for each tier binary (`target/aot/cmp2.py` is a local scratch script, not committed). The generated block files and the image stay under `target/` and are never committed.

## Implications

- The exact-guarded path is not worth keeping on this data: it costs more than the integer kernels it falls back to. Do not extend it to other blocks without first measuring the guard hit rate.
- The approximate tier makes the block ~9x faster but moves the pass by 14% at most; parity-wise it differs from the oracle in 32 words with 8 voices and not at all with 1 voice, and misses one pre-set criterion by 3 dB in the start-up pass. The acceptance rule should be fixed per use (steady state vs including start-up) before anyone adopts it.
- With the block at ~0 cost the remaining 87% of the pass is the target. Real time needs whole-pass compilation and, only if exact parity may be given up, a fast float path measured the same way.
- Hook overhead is no longer a reason to avoid a per-block hook.

## Follow-ups

- Repeat the audio comparison with a real note stimulus (the ORG note/drawbar inputs are not identified yet, `org-stage-map.md`), because the voice-only stimulus drives small, near-trivial signals.
- Decide the pass criterion for start-up passes (first 16) vs steady state.
- Apply the same two-path measurement to the next block of the `--profile-jit` ranking.
- Split `aot_pe.rs`, `aot_fast.rs`, `aot_hot.rs`, `aot_lower.rs` (over 400 lines with tests).

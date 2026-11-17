# ORG Core2 Single-Core Ceiling Study: SIMD JIT vs Hot-Block AOT

Date: 2026-11-16
Branch: `sharc-jit-ceiling-study`
Target: `core2_sram.bin` (8-voice pass: `--msg-base 0 --msg 0:0xc10b8=0x1215fb .. 0:0xc1160=0x1215fb`, `--warmup 16 --passes 300`, no `--lenient`).
Real-time budget (from `docs/research/sharc-jit-org-profile.md`): **46.08 µs/pass** -> **92.6 / 97.1 / 129.2 Msteps/s** (idle / 1 voice / 8 voices).

## 1. SIMD coverage & instruction/block classification (VERIFIED)

Reproducible command (commit `14be9db4` / `e62fb0d2`):

```powershell
$img = "$env:TEMP\ns4_sram\org_sram\core2_sram.bin"
cargo run --release -p nord-sharc --features jit --bin sharc_jit_diff -- $img `
  --pc 0x125c88 --boot 100000 --dev 0x282c0380=1 --dev 0x3108d008=5 `
  --trigger 0xB00E2 --idle-pc 0x125DDF --passes 300 --warmup 16 --chunk 1000 `
  --msg-base 0 --bench --profile-jit `
  --msg 0:0xc10b8=0x1215fb --msg 0:0xc10d0=0x1215fb --msg 0:0xc10e8=0x1215fb --msg 0:0xc1100=0x1215fb `
  --msg 0:0xc1118=0x1215fb --msg 0:0xc1130=0x1215fb --msg 0:0xc1148=0x1215fb --msg 0:0xc1160=0x1215fb
```

Verbatim output from the final binary (`pass 0..299: OK` lines omitted):

```text
booted 100000 steps 1:1 (pc=0x125de0)
bench (16 warmup, 300 passes): interp 1785900 steps mean=1.028 ms worst=2.618 ms (5.79 Msteps/s) | jit 1785600 steps mean=2.492 ms worst=7.954 ms (2.39 Msteps/s)
profile-jit (per pass): run_jit_calls=93.0 lookups=5787.2 hits=60.8 compiled=0.000 inelig=5726.5 (simd=5584.0) short_chunk=4.2 guard=90.0 | inv_calls=1420.0 evicted=0.000 resets=0 | steps: jit=131.2 fallback=5820.8 | time (ms/pass): total=2.492 scan=0.845 compile=0.000 jit_exec=0.021 fallback=0.976 other=0.651
classify-pass (5953 steps): jit_today=150 (2.5%) simd_only=2428 (40.8%) other=3375 (56.7%) | distinct_pcs=4208 distinct_blocks=1971 blocks_90pct=1376 blocks_99pct=1912
  other_reasons: [("Move(Other)", 1703), ("Move(DirectMem)", 739), ("RegMove(with_compute)", 302), ("Move(RegSwap)", 249), ("ShiftImm", 104), ("Compute(Multifunction)", 96), ("Flow(Return)", 54), ("Flow(CallIndirect)", 38), ("ModifyImm", 32), ("ComputeCond(cond!=31)", 25), ("Flow(CallDirect)", 16), ("Flow(JumpDirect)", 10), ("RegMove(cond!=31)", 6), ("Flow(DoUntilCounter)", 1)]
  top_20_pcs:
    pc=0x125e71 steps=24 (0.40%) simd=true class=simd_only
    pc=0x12501f steps=16 (0.27%) simd=true class=simd_only
    pc=0x125021 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125023 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125026 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125028 steps=16 (0.27%) simd=true class=simd_only
    pc=0x12502b steps=16 (0.27%) simd=true class=simd_only
    pc=0x12502d steps=16 (0.27%) simd=true class=simd_only
    pc=0x12502e steps=16 (0.27%) simd=true class=simd_only
    pc=0x125031 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125033 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125035 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125038 steps=16 (0.27%) simd=true class=simd_only
    pc=0x12503b steps=16 (0.27%) simd=true class=RegMove(with_compute)
    pc=0x12503e steps=16 (0.27%) simd=true class=simd_only
    pc=0x125040 steps=16 (0.27%) simd=true class=Compute(Multifunction)
    pc=0x125042 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125044 steps=16 (0.27%) simd=true class=Compute(Multifunction)
    pc=0x125046 steps=16 (0.27%) simd=true class=simd_only
    pc=0x125047 steps=16 (0.27%) simd=true class=RegMove(with_compute)
  top_20_blocks:
    0x125062..=0x125081 len=18 calls=16 steps=288 (4.84%) simd=true class=simd_only
    0x125089..=0x1250a8 len=18 calls=16 steps=288 (4.84%) simd=true class=simd_only
    0x12501f..=0x125038 len=12 calls=16 steps=192 (3.23%) simd=true class=simd_only
    0x12504a..=0x12505b len=11 calls=16 steps=176 (2.96%) simd=true class=simd_only
    0x12167b..=0x12169a len=14 calls=8 steps=112 (1.88%) simd=true class=Move(Other)
    0x1260bc..=0x126153 len=64 calls=1 steps=64 (1.08%) simd=true class=simd_only
    0x126156..=0x1261ef len=64 calls=1 steps=64 (1.08%) simd=true class=simd_only
    0x1261f2..=0x126282 len=63 calls=1 steps=63 (1.06%) simd=true class=simd_only
    0x12163c..=0x121649 len=6 calls=8 steps=48 (0.81%) simd=true class=Move(Other)
    0x12165e..=0x12166c len=6 calls=8 steps=48 (0.81%) simd=true class=Move(Other)
    0x1250ad..=0x1250af len=3 calls=16 steps=48 (0.81%) simd=true class=simd_only
    0x125b79..=0x125b7d len=3 calls=16 steps=48 (0.81%) simd=true class=Move(Other)
    0x12538c..=0x12539a len=7 calls=6 steps=42 (0.71%) simd=false class=jit_today
    0x1215fb..=0x121604 len=5 calls=8 steps=40 (0.67%) simd=true class=Move(Other)
    0x121609..=0x121610 len=4 calls=8 steps=32 (0.54%) simd=true class=Move(Other)
    0x12166f..=0x121678 len=4 calls=8 steps=32 (0.54%) simd=true class=RegMove(with_compute)
    0x12505c..=0x12505f len=2 calls=16 steps=32 (0.54%) simd=true class=ShiftImm
    0x125083..=0x125086 len=2 calls=16 steps=32 (0.54%) simd=true class=ShiftImm
    0x125e71..=0x125e71 len=1 calls=24 steps=24 (0.40%) simd=true class=simd_only
    0x120bfa..=0x120bfc len=2 calls=8 steps=16 (0.27%) simd=true class=Move(Other)
```

Settled 8-voice pass (`5,953` steps, `4,208` distinct PCs, `1,971` distinct dynamic basic blocks):

| Class | Steps / pass | Share | Notes |
|---|---:|---:|---|
| `jit_today` (`!simd && can_jit_op`) | 150 | 2.5% | Scalar setup (`0x12538C`, etc.) |
| `simd_only` (`simd && can_jit_op(op, false)`) | 2,428 | 40.8% | Blocked **only** by `if simd { return false; }` |
| `other` (`!can_jit_op(op, false)`) | 3,375 | 56.7% | Moves/compute/flow not handled by `can_jit_op` even without SIMD |
| **Combined with SIMD JIT** | **2,578** | **43.3%** | Remaining fallback = 3,375 steps (56.7%) |

Breakdown of `other` (`3,375` steps):

| Reason | Steps | Reason | Steps |
|---|---:|---|---:|
| `Move(Other)` | 1,703 | `Flow(CallIndirect)` | 38 |
| `Move(DirectMem)` | 739 | `ModifyImm` | 32 |
| `RegMove(with_compute)` | 302 | `ComputeCond(cond!=31)` | 25 |
| `Move(RegSwap)` | 249 | `Flow(CallDirect)` | 16 |
| `ShiftImm` | 104 | `Flow(JumpDirect)` | 10 |
| `Compute(Multifunction)` | 96 | `RegMove(cond!=31)` | 6 |
| `Flow(Return)` | 54 | `Flow(DoUntilCounter)` | 1 |

### 20 hottest PCs (all `simd=true`)

- `0x125e71`: **24 steps (0.40%)** (`simd_only`)
- `0x12501f..=0x125047` (19 PCs in `0x12501F` saw helper): **16 steps (0.27%) each** (`15 × simd_only`, `2 × RegMove(with_compute)` at `0x12503b`/`0x125047`, `2 × Compute(Multifunction)` at `0x125040`/`0x125044`)

### 20 hottest dynamic basic blocks

| Rank | PC range | Len | Calls | Steps | Share | SIMD | Class |
|---:|---|---:|---:|---:|---:|---|---|
| 1 | `0x125062..=0x1250a8` (2 blocks: `..081`, `089..0a8`) | 18 | 16 | 288 ea | 4.84% ea | true | `simd_only` |
| 3 | `0x12501f..=0x125038` | 12 | 16 | 192 | 3.23% | true | `simd_only` |
| 4 | `0x12504a..=0x12505b` | 11 | 16 | 176 | 2.96% | true | `simd_only` |
| 5 | `0x12167b..=0x12169a` | 14 | 8 | 112 | 1.88% | true | `Move(Other)` |
| 6 | `0x1260bc..=0x126153` | 64 | 1 | 64 | 1.08% | true | `simd_only` |
| 7 | `0x126156..=0x1261ef` | 64 | 1 | 64 | 1.08% | true | `simd_only` |
| 8 | `0x1261f2..=0x126282` | 63 | 1 | 63 | 1.06% | true | `simd_only` |
| 9–12 | `0x12163c..0x12166c`, `0x1250ad..0x1250af`, `0x125b79..0x125b7d` | 3–6 | 8–16 | 48 ea | 0.81% ea | true | `Move(Other)` / `simd_only` |
| 13 | `0x12538c..=0x12539a` | 7 | 6 | 42 | 0.71% | false | `jit_today` |
| 14 | `0x1215fb..=0x121604` | 5 | 8 | 40 | 0.67% | true | `Move(Other)` |
| 15–18 | `0x121609..10`, `0x12166f..78`, `0x12505c..5f`, `0x125083..86` | 2–4 | 8–16 | 32 ea | 0.54% ea | true | `Move(Other)` / `RegMove(with_compute)` / `ShiftImm` |
| 19 | `0x125e71..=0x125e71` | 1 | 24 | 24 | 0.40% | true | `simd_only` |
| 20 | `0x120bfa..=0x120bfc` | 2 | 8 | 16 | 0.27% | true | `Move(Other)` |

## 2. Cost model on 8-voice pass (ASSUMED projections from VERIFIED #455–#458 benchmarks)

Same-machine synthetic micro-benchmarks (`cargo test --release -p nord-sharc --features jit --lib bench_jit_ -- --ignored --nocapture`):
- `bench_jit_dsp_loop` (mixed DSP compute + memory): **27.44 Msteps/s** (`36.4 ns/step`)
- `bench_jit_polyphony_8_and_32_voices` (8v `jit-fast_float` `DO` loop): **56.57 Msteps/s** (`17.7 ns/step`; 32v: **63.98 Msteps/s**, `15.6 ns/step`)
- `bench_jit_dagmem_loop` (hoisted `DO` loop): **459.78 Msteps/s** (`2.17 ns/step`; not representative of 2–64-op blocks broken by `Move(Other)` / `DirectMem` / branches)
- Interpreter baseline on 8 voices: `Core::run` `98.3 ns/step` (`10.18 Msteps/s`, `task-1578`); profiled fallback `Core::step()` `0.786 ms / 5,820.8 = 135.0 ns/step` (`task-1626`).

Projected 8-voice pass with SIMD JIT (`2,578` JIT steps + `3,375` fallback steps):

| Projection / Baseline | JIT part | Fallback / Interp | Lookup/chunking | Total pass time | Projected speed |
|---|---:|---:|---:|---:|---:|
| **Low SIMD JIT** (`27.44 Msteps/s` JIT, `135.0 ns/step` fallback) | 93.8 µs | 455.6 µs | ~289 µs | **0.838 ms** | **7.1 Msteps/s** |
| **High SIMD JIT** (`63.98 Msteps/s` JIT, `98.3 ns/step` fallback) | 40.2 µs | 331.8 µs | ~150 µs | **0.522 ms** | **11.4 Msteps/s** |
| **43.3% eligible at measured JIT speed** (`16.4 Msteps/s`, `61 ns/step`) | 157.3 µs | 331.8–455.6 µs | 0 µs (no scan) | **0.489–0.613 ms** | **9.7–12.2 Msteps/s** |
| **Faster interpreter (2×–3×)** (`33–50 ns/step`, `5,953` steps) | 0 µs | 196–298 µs | 0 µs | **0.196–0.298 ms** | **20–30 Msteps/s** |
| **Core1 + Core2 on separate threads** (budget per core) | — | — | — | **46.08 µs budget/core** | **129.2 Msteps/s needed** |

Gap drivers: (1) whether SIMD DAG/compute ops inline cleanly (`15.6 ns/step`) vs `extern "C"` helpers (`36.4 ns/step`), and (2) dispatch overhead across ~1,971 fragmented blocks.

## 3. Overhead removal (scan + `Vec` alloc + 64-step chunking)

Measured in `task-1626` (`--profile-jit`): `scan = 0.641 ms/pass` (`0.478 ms` in `task-1578`), `other = 0.549 ms/pass`, `run_jit_calls = 93`, `short_chunk = 4.2`.
- **On current 2.5% JIT coverage**: removing `scan` + `64`-step chunking recovers `~0.5–0.6 ms/pass`, bringing JIT from `3.0–3.7 Msteps/s` back to **`~10 Msteps/s`** (parity with `Core::run`).
- **On projected 43.3% SIMD-JIT coverage**: removing per-lookup scan/alloc + chunking removes the `~150–289 µs` lookup/chunking overhead, yielding **`0.549 ms` (`10.8 Msteps/s`, Low)** to **`0.372 ms` (`16.0 Msteps/s`, High)** — still **8×–12× slower** than the `46.08 µs` real-time budget.

## 4. AOT alternative (VERIFIED block counts)

- **Distinct blocks for 90% / 99% of steps**: **`1,376`** blocks for **90%**, **`1,912`** blocks for **99%** (out of `1,971` total blocks, `4,208` PCs).
- **Assessment**: Even with SIMD enabled, basic blocks are fragmented by `Move(Other)` (1,703 steps), `Move(DirectMem)` (739), `RegMove(with_compute)` (302), `Move(RegSwap)` (249), `ShiftImm` (104), and `Compute(Multifunction)` (96), so **56.7% of steps still fall back to `Core::step()`**. Single-block AOT faces the same `3,375`-step (`~332 µs`) fallback floor; only **whole-pass / function-level AOT** (`sharc-aot` with `rustc -O` compiling control flow + `Move(Other)`/`DirectMem`/`RegSwap`/`ShiftImm`/SIMD) eliminates the fallback floor.

## 5. Verdict

**Neither** basic-block SIMD JIT nor single-block AOT can reach **100 Msteps/s** on one core: even if all `2,578` (43.3%) SIMD-eligible steps took **0 µs**, the remaining `3,375` (56.7%) `other` steps at `98.3 ns/step` take **`~332 µs`**, already **7.2× over** the `46.08 µs` real-time pass budget, capping basic-block SIMD JIT at **~11–16 Msteps/s** even after removing lookup overhead. Reaching `~100 Msteps/s` on one core requires **(b) whole-pass / function-level AOT** (lowering control flow + `Move(Other)`/`DirectMem`/`RegSwap`/`ShiftImm` + SIMD into compiled Rust via `sharc-aot`, paired with multi-core scheduling across the 4 SHARC cores). **First PR to write**: check `jit.blocks.get(&key)` **before** allocating `Vec` and scanning `DecodeCache` in `compile_or_lookup_jit_block`, plus a negative cache for ineligible PCs — expected gain: eliminates `~0.44–0.64 ms/pass` of scan overhead, lifting current 8-voice JIT from `3.0–3.7 Msteps/s` to **`~10 Msteps/s`** (parity with `Core::run`).

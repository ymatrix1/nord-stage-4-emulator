# ARM0 native Rust runtime: wall-clock per 1 ms tick

**Checked:** 2026-11-28 (first run 2026-11-23; predecode cache, tail, queue and budget measured 2026-11-24; tail follow-up 2 and producer thread 2026-11-28).

**Sources:** `crates/nord-arm` + `crates/nord-tools/src/bin/arm_lle_bench.rs`; user OS v1.66 images `target/fw_unpacked/bank0_CODE.bin` (loaded at `0xC0000000`) and `bank0_SRAM.bin` (read-only, never committed); a 256 KB EEPROM image built by `tools/arm_program_load.build_image` from `target/oled/eeprom.bin` plus factory program `B3 Chords n Solo.ns4p`; Python reference `tools/arm_org_stream.py`, `tools/arm_steady.py`, `tools/arm_dsp_return.py`; [pno-jit-arm-profile](pno-jit-arm-profile.md) (Python/Unicorn cost); [dsp-to-arm-return](dsp-to-arm-return.md); [arm-tick-timers](arm-tick-timers.md) (24 kHz timer, D130); [arm-dsp-live-link](arm-dsp-live-link.md) (7,340 count); [arm-oled-render](arm-oled-render.md) (display flush).

**Question:** Can ARM0 run the real OS v1.66 boot, program load and key events inside Rust below 1.0 ms wall-clock per 1 ms virtual tick (`ns4-lle2`, Gate 1a), and what is the tail, the message latency through a decoupled queue and the instruction budget per ms?

**Answer in one line:** a predecode cache doubles the instruction rate (median 93 to 187 M insns/s in an interleaved A/B, state and message content identical to the plain `Cpu::step` path at four tick budgets); the mean is 0.11 ms (p50 0.07), 0.2 to 0.5 % of ticks (4 to 9 of 1,900) still run over 1 ms (max 1.5 ms, never more than 3 in a row, worst backlog up to 0.55 ms), so "< 1 ms on every tick" is still NOT strictly met; through a decoupled queue drained once per ms no message waits more than 1 ms in the simulation.

## Method

Machine: AMD64 Family 25 Model 33 (AMD, Zen 3 class, 32 logical processors, shared with other sessions: every number is noisy, A/B comparisons are best of N), Windows 11, release build.

```
cargo run --release -p nord-tools --bin arm-lle-bench -- \
  <bank0_CODE.bin> <bank0_SRAM.bin> <eeprom_b3.bin>
```

Environment switches: `NO_FULL=1` (skip the full-execution comparison run), `BOOT_ONLY`, `PHASE_SCALE` (percent of the phase lengths), `TICK_INSNS` (instructions per tick, default 200,000), `MICRO=1` (dispatch rates on synthetic code), `PACED=1` (producer/consumer threads, `PACED_CAP` queue ticks), `STEADY_BOOT=1` (the `arm_steady.py` boot, null-call histogram).

1. Boot to the main loop `0xC00A0248` with the full board attached (panel, 4 I2C EEPROMs, UART2) and idle skipping on.
2. Real timer on, DSP port logging and the DSP return hooks installed, 64 MB of arena reserved so on-demand mapping cannot reallocate during the run.
3. Six phases: load 800 ticks, idle 300, one key on 200, one key off 200, eight keys on 200, eight keys off 200. One tick = `TICK_INSNS` ARM instructions of virtual time (200,000, ASSUMED, see "Instruction budget") plus one timer IRQ.
4. **The timed region of each tick contains** the MIDI send (first tick of a phase, includes the UART ISR), the tick run with idle skipping, and `drain_messages_into` (message hand-over plus the DSP return sync).
5. The same phases run again with full execution (no skipping) on a clone. The machine state hash and an FNV digest of the full message list (path, comm, words, pc) are compared after every phase.

## Predecode cache (follow-up 1)

One decoded page (1,024 `Entry { insn, cond mask, op, run }`) per guest 4 KiB code page, built on first execution, driven by a 128-entry handler table (`h_dp::<OP, S, FORM>` const generics for the 96 data-processing forms). `run` is the number of straight-line instructions before the next block ender (B, BL, BX, loads or data processing into r15, anything on the slow path); non-enders run without per-instruction pc, condition or limit handling. Page flag `HOOK_C` marks a page with a decode: a store to it, or a new execute hook on it, drops the decode before the next fetch, so self-modifying code and hooks keep their semantics. The idle probe runs on the same fast path through a probe mode of `SysBus` (`probing`, `impure`). Details: [arm0-runtime](../specs/arm0-runtime.md).

| Build | Boot rate (executed insns/s) | Source |
|---|---|---|
| Old interpreter (`Cpu::step` per instruction) | 93.5 M median (93 to 95), 7 interleaved runs | A/B on the shared machine, see "Measurement conditions" |
| + per-instruction decode cache | 136 M | same |
| + block execution, inline B/BL | 167 M | same |
| + probe on the cached path, page-wise zero | 187 M median (184 to 189), 7 interleaved runs | same |

About 2.0x, **not the targeted 5x**. Straight-line synthetic code reaches about 470 M insns/s (`MICRO=1`), so dispatch is no longer the limit: the firmware is branchy and load/store heavy and each guest instruction still costs about 5 ns of host work (memory entry lookup, device checks, flags). `codegen-units = 1` gave +3 % and was not adopted. A faster core needs a different technique (translation to host code or superblocks with cached flags), not a better dispatch loop.

**Equivalence (every existing test kept, plus new ones).** Boot state hash `0960ef4eefb7fad5` and the per-phase message digests (load `221c6f69751f1f8e`, idle `0cb5cb143a3b7106`, key1 on `67ab63840268b92d`, key1 off `dbdec8588970d602`, key8 on `17622e4962354279`, key8 off `8f24476a8b30f9ee`) and per-phase state hashes (`68a8abb37e4e1d0b`, `8221207b8b72c8e4`, `97893e12e5db4be8`, `d2eb2e9801341ef5`, `a5e39de19c49b79e`, `6be2c14282a84986`) are **identical** to the old interpreter's (parent commit built with the digest lines added) at the default 200 k budget. At 100 k, 300 k and 456 k the parent-commit binary was not run; there the comparison is this tree with `PREDECODE=0` (plain `Cpu::step`) against the cache, and all four budgets (100 k, 200 k, 300 k, 456 k) give identical output: 13 digest lines each, boot state hash plus six per-phase state hashes and message digests (`oldnew.py` in the scratch folder, raw output `target/lle/oldnew.txt`). The skipping run equals the full run in all phases, but both run on the predecode path (the bench clones the machine), so that check is idle skipping against full execution, not old against new. Unit tests: a 200,000-word random differential of `exec_decoded` against `Cpu::step` (registers, cpsr, bus access log), a 60-program random differential of `Machine::run` with `predecode` on and off including self-modifying code, store and hook invalidation tests, probe-mode tests. `WFI` count is 0 in this workload and is covered by `wfi_has_one_semantic_in_the_full_and_the_skipping_path`.

## Measurement conditions and variance

The machine is shared (three Claude sessions, a codebase indexer and two spinning `find` processes ran throughout), so wall-clock numbers are noisy and the tail (p99, p99.9, max, over-1-ms counts) is the most sensitive. Conditions: AMD Ryzen 9 5900XT (16 cores, 32 logical, 3.3 GHz base), Windows 11, power plan "Alto desempenho" (High performance), release build, same build flags for both binaries. No `cargo` or `rustc` process was running during the A/B below; host CPU load sampled just before each run was 49 to 99 % (median 87 to 88 %), i.e. **never idle**, so absolute values are an upper bound on cost and the other cores were busy.

A/B, load phase, `NO_FULL=1`, one warm-up run of each binary discarded, then 7 runs of each **interleaved** (old, new, new, old, ...; `ab_tail.py` in the scratch folder, raw rows in `target/lle/ab_tail.json`). Median (min to max):

| Metric (load phase unless noted) | Old interpreter | Predecode cache | Outside noise? |
|---|---|---|---|
| Boot rate, M executed insns/s | 93.5 (93 to 95) | 187 (184 to 189) | yes, 2.0x, ranges do not overlap |
| Boot wall time, s | 2.48 (2.44 to 2.49) | 1.24 (1.23 to 1.26) | yes |
| Mean ms/tick | 0.270 (0.268 to 0.275) | 0.108 (0.105 to 0.112) | yes |
| p50 ms | 0.170 (0.170 to 0.171) | 0.070 (0.068 to 0.071) | yes |
| p99 ms | 2.53 (2.49 to 2.61) | 0.963 (0.952 to 0.994) | yes |
| p99.9 ms | 2.72 (2.56 to 3.61) | 1.27 (1.19 to 1.36) | yes (old max is a spike) |
| Max ms | 2.73 (2.65 to 3.62) | 1.48 (1.42 to 1.55) | yes |
| Ticks over 1 ms (of 800) | 36 (35 to 37) | 6 (4 to 8) | yes |
| Longest run over 1 ms | 32 (32 to 32) | 1 (1 to 3) | yes |
| Worst backlog, ms | 47.9 (47.4 to 50.6) | 0.48 (0.42 to 0.55) | yes |

Earlier single runs in this note (old: 43 ticks over 1 ms, backlog 61 ms; boot 83 to 90 M insns/s) were taken under heavier load and sit outside the old binary's A/B range: they show the noise, and the A/B medians above replace them as the quoted before values. The new core's figures agree across 12 runs (5 + 7): 4 to 9 ticks over 1 ms in all phases, max 1.42 to 1.55 ms, run at most 3.

**What is deterministic.** Executed instructions per tick do not depend on the host: 12,481 per load tick (mean), 3,288 to 3,448 in the other phases, identical in every run of both binaries, and every over-1-ms tick of the old core executed the full 200,000-instruction budget. So the tail is set by how many ticks do a full budget of real work (a count fixed by the firmware and the tick budget) times the host cost per instruction (about 5.3 ns now, about 10.7 ns before); OS scheduling only adds the jitter seen in the ranges above. I did **not** measure per-thread CPU time or count host cycles, so the wall-clock tail still includes preemption by the other load: a single spike coincident with other load (such as the old max of 3.62 ms in one run) is unproven. The conclusions drawn (6x fewer slow ticks, no backlog above 0.55 ms, still 4 to 9 ticks over 1 ms) rest on medians and on ranges that do not overlap.

Not repeated under these conditions (single runs, noise band not measured, treat as indicative): the tick-budget sensitivity table (the ordering of 300 k against 456 k in ticks over 1 ms, 23 against 16, is inside the noise; the growth of p99 and max with the budget is not), the queue simulation (derived from the same per-tick costs; its 1 ms maximum is quantised by the 1 ms drain and was the same in all five runs that printed it), and the `PACED=1` runs (OS scheduler dominated, already not quoted).

## Tick tail with the faster core (follow-up 2)

Skipping run, 1,900 ticks per run, five runs (`NO_FULL=1`), min to max of the five (see the previous section for the interleaved A/B and its conditions):

| Phase | Ticks | Mean ms | p50 | p99 | p99.9 | Max | Ticks > 1 ms | Longest run > 1 ms | Worst backlog ms | Executed insns/tick | Messages |
|---|---|---|---|---|---|---|---|---|---|---|---|
| load | 800 | 0.109 to 0.111 | 0.071 | 0.965 to 1.000 | 1.25 to 1.39 | 1.43 to 1.53 | 4 to 8 (0.5 to 1.0 %) | 1 to 2 | 0.43 to 0.53 | 12,481 | 15,337 |
| idle | 300 | 0.063 to 0.066 | 0.072 | 0.09 to 0.14 | 0.10 to 0.39 | 0.13 to 0.52 | 0 | 0 | 0 | 3,384 | 3 |
| key1 on | 200 | 0.063 to 0.065 | 0.072 | 0.09 to 0.13 | 0.13 to 0.21 | 0.19 to 0.48 | 0 | 0 | 0 | 3,288 | 126 |
| key1 off | 200 | 0.067 to 0.069 | 0.072 | 0.17 to 0.20 | 0.20 to 0.31 | 0.30 to 0.54 | 0 | 0 | 0 | 3,448 | 98 |
| key8 on | 200 | 0.069 to 0.072 | 0.072 | 0.12 to 0.40 | 0.40 to 0.45 | 0.92 to 0.95 | 0 | 0 | 0 | 3,365 | 1006 |
| key8 off | 200 | 0.070 to 0.072 | 0.072 | 0.11 to 0.39 | 0.38 to 0.47 | 0.97 to 1.01 | 0 to 1 | 0 to 1 | 0 to 0.01 | 3,426 | 969 |

All phases: 4 to 9 ticks over 1 ms out of 1,900 (0.2 to 0.5 %), longest run 1 to 3, worst backlog 0.42 to 0.55 ms (backlog = work behind real time if ticks are consumed at one per ms and slow ticks carry over). Before (old interpreter, interleaved A/B medians): load mean 0.270, p50 0.170, p99 2.53, max 2.73, 36 of 800 load ticks over 1 ms (4.5 %), longest run 32, backlog 47.9 ms (first single run, under more load: 38 ticks, max 3.83, backlog 61.5 ms). A full 200,000-instruction tick cost about 2.1 ms before and costs about 1.03 ms now (`target/lle/old_full.txt`, `v3_full.txt`). The over-1-ms ticks are the same real firmware work as before (full 200,000-instruction ticks during program load); they got about 2x shorter, not 5x.

**Criterion "< 1 ms on every tick": NOT met.** 0.2 to 0.5 % of ticks exceed it, by at most 0.55 ms. The bean criterion stays unticked. For real-time use the margin is small but the tail is bounded: no backlog above 0.55 ms and no run above 3 ticks.

## Message latency through the decoupled queue (follow-up 2)

Simulation in the bench (`queue ...` lines, `metrics::simulate_queue`): the ARM thread produces one tick per recorded cost, a consumer drains the queue once per ms. Latency of a message = time from the end of its tick's virtual ms to the drain that delivers it.

| | p50 | p99 | p99.9 | max | Max queue | Max ticks per drain |
|---|---|---|---|---|---|---|
| Old interpreter, all phases | n/a | 59 ms | n/a | 61 ms | 1,800 msgs | 7 |
| Faster core, load | 0 to 1 ms | 1 ms | 1 ms | **1 ms** | 1,855 msgs | 2 |
| Faster core, key phases | 0 | 0 | 0 | 0 to 1 ms | 907 msgs (key8 on) | 1 to 2 |

Queue size: the worst backlog (up to 0.55 ms) is one tick of slack, the largest drain is 2 ticks and 1,855 messages (load phase, one tick carries up to about 1,000 messages). A bounded queue of 4 ticks / 4,096 messages suffices **only at the ASSUMED 200,000 instructions per tick**: at 300,000 the backlog is 9.4 ms with 21 slow ticks in a row and at 456,000 it is 17.0 ms with 14 in a row (budget table below), so the queue must be sized for the budget that is actually true (32 ticks covers both measured cases); the bench's real-thread run uses 64 ticks (64 ms, 120x the measured backlog) and the producer was blocked less than 0.5 ms in total, i.e. the queue was never full. `MESSAGE_CAP` in the bus is 262,144 messages.

**Real threads (`PACED=1`, three runs, EMPIRICAL, unreliable).** Producer thread paced at one tick per wall-clock ms, consumer thread draining once per ms with the OS sleep. Lateness is measured against the ideal delivery time. Best run: p50 and max 1.00 ms (load), 0 to 1 ms elsewhere; the other two runs 2 to 17 ms and 21 to 83 ms with 20 to 60 % of ticks late. Producer blocked time was 0.2 to 0.4 ms in all three runs, so the core kept up; the spread is the **consumer wake-up jitter of the shared machine** (Windows timer granularity, 32 threads used by other sessions), not core cost. This run exercises the queue and thread plumbing, not a latency number; it must be repeated on a quiet machine with raised timer resolution before a latency claim is made.

## Instruction budget per ms (follow-up 4)

`INSNS_PER_TICK = 200000` is a harness convention: the real hardware timer interrupt is at 24 kHz (41.67 us, [arm-tick-timers](arm-tick-timers.md) D130), and the Python harness runs 200,000 instructions per 1 ms of virtual time. The real figure cannot be measured without the hardware (no cycle counter is read by the firmware). It is **bounded**, not measured:

* Upper bound: the ARM926EJ-S is single-issue (IPC <= 1), so instructions per ms <= clock in kHz. The AM1705 clock is 375 or 456 MHz per the repository docs; the repository elsewhere assumes 300 MHz (12,500 cycles per 24 kHz interrupt). Bound: **<= 300 k (300 MHz), 375 k or 456 k (AM1705 clocks) instructions per ms**. The clock itself is programmed by the boot loader and not read from the images here (ASSUMED).
* Cache misses, load-use stalls, branch penalties and 16-bit device accesses put the real IPC well below 1 (typical 0.5 to 0.8 for control code), which at 300 to 456 MHz is about 150 k to 365 k per ms. The assumed 200 k is inside that range (IPC 0.67 at 300 MHz) but not pinned by it.

Sensitivity of the benchmark to the budget (skipping run, one run each, load phase; the other phases stay below 1.2 ms in all runs):

| `TICK_INSNS` | load mean | load p99 | load max | Load ticks > 1 ms | Longest run | Worst backlog | Full-exec cost per tick |
|---|---|---|---|---|---|---|---|
| 100,000 | 0.110 | 0.61 | 1.28 | 1 | 1 | 0.28 ms | 0.55 ms |
| 200,000 | 0.110 | 0.97 | 1.5 | 4 to 8 | 1 to 2 | 0.5 ms | 1.03 ms |
| 300,000 (about 300 MHz at IPC 1) | 0.110 | 1.45 | 1.73 | 23 (2.9 %) | 21 | 9.4 ms | 1.5 to 2.6 ms |
| 456,000 (456 MHz at IPC 1) | 0.109 | 2.17 | 2.39 | 16 (2.0 %) | 14 | 17.0 ms | 2.7 to 4.2 ms |

The mean does not move (executed instructions per tick stay at 12,500 because idle skipping removes the rest), the tail does: the load-phase ticks that run a full budget scale with it. State hash and message content equal the full run at every budget. If the hardware retires 300 k+ instructions per ms (an upper bound, not an expectation), the load tail needs a faster core or a decoupled thread with a backlog of up to 17 ms. If it retires 100 k, the tail disappears (1 tick over 1 ms in 800).

## Null-call count 747 against 762 (follow-up 3)

`STEADY_BOOT=1` runs the `tools/arm_steady.py` boot (display stubs on, no board) and prints a histogram of `(pc, lr)` per null call; the same histogram was taken from `arm_steady.boot_to_main_loop` with Unicorn (`uv run --with unicorn`, 36 s).

* Both runs stop at `0xC00A0248`; both hit the **same 68 distinct call sites** (`pc = 0`, different `lr`).
* 65 sites have **identical counts**. Three sites differ: `lr = 0xC007694C`, `0xC0076964`, `0xC0076978`, **8 times in Python, 3 times in Rust** (5 x 3 = 15 = 762 - 747). They lie inside the display flush `0xC0076924`, which sends the three command bytes `B0`, `00`, `10` through the display driver's vtable ([arm-oled-render](arm-oled-render.md)); with no display driver object in this boot, each send is a null call.
* So Python flushes the frame buffer 8 times during the boot and Rust 3 times; nothing else differs (VERIFIED, run). The flush is a periodic display refresh, so the likely cause is a difference in how often the boot's wait loops see timer ticks (Rust injects 6 wait-loop IRQs, the Python harness breaks polls with `enable_poll_breaker`; ASSUMED, not isolated). The difference is confined to this boot variant: the benchmark boot (board attached) ends with 25 null calls and its state matches between the skipping and full runs.
* **Documented, not fixed:** matching Python exactly would mean copying the Python poll-breaker timing into the Rust wait-loop model, which has no purpose (the Python count is the less accurate of the two, being unticked).

## Load-phase message count 15,337 against 7,340 (follow-up 3)

The two numbers count different things:

* **15,337** is every message the bench sees in the 800 load ticks: all DSP chips (ORG, PNO, FX, SMP, SY), all cores, path 1 and path 2.
* **7,340** is the PNO core 1, path 2 words only: [arm-dsp-live-link](arm-dsp-live-link.md) says "load phase (800 ticks) 7,340" for the stream into PNO core 1, and `tools/arm_org_stream.py` `select(messages, chip=PNO, core=1)` keeps only `m.chip == "PNO" and m.path == 2 and m.core == 1`. The same research finds ORG core 1, ORG core 2 and PNO core 2 receiving a common 876-word strip set (912 words per target in the load phase, [arm-dsp-live-link-synth](arm-dsp-live-link-synth.md)) and further FX and SMP traffic, which makes 15,337 total against 7,340 for one target plausible.
* **Consistent with the count, not proven:** the 7,340 stream and the 15,337 total differ in scope, and nothing here shows the PNO core 1 path 2 share of the Rust stream is 7,340.
* **Not recomputed per target.** `grouper::Message` carries `path` and `comm` but not the chip and core, which `tools/arm_dsp_route.py` derives from the comm object; the per-target split of the Rust stream was therefore not produced. The Rust stream is equal in content to the full-execution run (digest) and its load phase was the same length (800 ticks); the two counts are consistent, not proven equal per target. Follow-up: add the chip/core routing to `nord-arm` (needed for `ns4-lle3` anyway) and compare 7,340.

## Findings from the review of the first run

* A message split across a tick boundary lost its first words (the grouping state was rebuilt on each drain). Fixed: the grouper lives in the bus; test `a_message_split_across_two_drains_is_delivered_whole`.
* Stub returns now set the instruction set from `lr` bit 0 (test `stub_return_takes_the_instruction_set_from_lr`).
* `WFI` has one semantic in both run paths (the instruction completes, the rest of the budget is skipped); `Machine::wfi` counts it.
* Unbounded logs are capped (`null_calls`, `auto_mapped`, `I2cBus::log`, bus messages), `DspReturn::read` and the drain no longer allocate per call.

## Tail follow-up 2 (2026-11-28): where the heavy ticks go, the rate they need, a decoupled thread

Tools: `arm-tail-bench` (per-tick tail at four budgets, 7 interleaved runs, `PROFILE=1` pc histogram), `arm-rt-bench` (producer thread, [producer.rs](../../crates/nord-arm/src/machine/producer.rs)), shared code in `nord_tools::arm_bench`. Counters are `Machine::stats` ([RunStats](../../crates/nord-arm/src/machine/runstats.rs)): block-granular `u64` increments, always on, no timing.

**Host load.** The machine was shared and busy: 65 to 100 % CPU (Windows `LoadPercentage`) during every measurement here, 32 logical processors, AMD Zen 3 class. Wall clock and even thread cycles ranged 1.6x between runs of the same binary (heavy-tick rate 107 to 198 M insns/s). Rules used: work counted in executed instructions and counters (deterministic, asserted equal across runs), wall clock reported as median [min .. max] over 7 interleaved runs, a gain called real only outside the min-max band.

### 1. The heavy ticks are one long computation, not idle, not hooks

Per budget (deterministic, identical in all 7 runs; the digests below are equal between the 7 runs, `PREDECODE=0` and the single-thread reference):

| budget insns/ms | ticks with exec = budget | total executed (1,900 ticks) | message digest | counters (all budgets alike) |
|---|---|---|---|---|
| 150 k | 44 | 13.64 M | `3efde38f525cfbd1` | cached entries 29.9 k, blocks 1.71 M (7.8 insns/block), B/BL enders 1.32 M, other enders 0.38 M, stale exits 3.8 k, page follows 304 k, plain steps 257 k (1.9 %), hook runs 1,516 |
| 200 k | 33 | 13.71 M | `3efde38f525cfbd1` | same within 0.3 % |
| 300 k | 21 | 13.66 M | `a3aab7d387c3f514` | same within 0.3 % |
| 456 k | 14 | 13.71 M | `a3aab7d387c3f514` | same within 0.3 % |

* Every over-budget tick executes **exactly the whole budget**: one stretch of about 6.5 M instructions (44 x 150 k = 6.6 M, 33 x 200 k = 6.6 M, 21 x 300 k = 6.3 M, 14 x 456 k = 6.4 M) in the load phase is real, non-idle, non-polling work, cut into ticks by the budget. The rest of the 1,900 ticks execute a median of 4.3 k instructions (idle skipping).
* **Where:** a pc histogram of the 35 heavy ticks at 200 k (plain interpreter, 7.0 M instructions counted, `PROFILE=1`): 75 % of them lie in 2 KB at `0xC025C700..0xC025CE80` (top 64-byte buckets `0xC025C740` 10.8 %, `0xC025C7C0` 9.7 %, `0xC025CB00` 9.4 %, `0xC025C780` 8.3 %, `0xC025CB40` 7.0 %, `0xC025CB80` 5.9 %); the code there is conditional register arithmetic, `clz` and shifts by register (it looks like software floating point on a core without an FPU); the rest is spread thin (no other 64-byte bucket above 0.6 %). The routine is identified by its instruction mix only, not by name (no disassembly is committed, D20).
* Not the cause: store-induced invalidations (3.8 k exits in 13.7 M instructions, 0.03 %), hooked pages (1,516 hook runs, 1.9 % plain steps, not attributed further), device reads (earlier run: 0 to 106 per heavy tick, arena growth 0).
* The cached loop covers 98.1 % of instructions; blocks are short (7.8), so one block ender per 6 instructions.

### 2. Per-instruction cost: no candidate outside the noise

| quantity | value |
|---|---|
| dispatch ceiling, `add r0,r0,#1` x15 + `b`, predecode on (`MICRO=1`) | 375 M insns/s |
| ldr/str/add mix | 230 M insns/s |
| add + cmp + bne + bl loop | 205 M insns/s |
| firmware heavy ticks, least disturbed run | 185 to 198 M insns/s, 16.6 to 17.8 thread cycles/insn |
| firmware heavy ticks, median of 7 runs on the loaded host | 124 to 129 M insns/s, 25 to 26 thread cycles/insn |

The firmware loop sits at 50 to 85 % of the synthetic ceiling that already includes block ends and loads. Candidates tried:

| candidate | result | decision |
|---|---|---|
| per-instruction `bus.pc` store moved into the memory handlers (`Bus::set_pc`) | A/B alternating, 9 pairs, 3 runs each, thread cycles per insn on heavy ticks: parent 31.06 [19.84 .. 31.91], change 31.43 [21.23 .. 33.37] (median +1 %, bands overlap) | not outside the noise band: reverted (the test `device_store_records_the_instruction_pc_with_and_without_the_cache` stays) |
| block chaining, inline flag handling, pair fusion, RAM fast path, loop specialisation, flat address path | not started | the measurement above cannot resolve a gain below about 25 % at 65 to 100 % host load; a change that small would be unprovable here. Each needs a quiet machine for its A/B (follow-up) |

Idle-loop specialisation beyond WFI has nothing to act on: the heavy stretch is not a wait loop (registers change every iteration, the probe finds no period).

### 3. The tail at the four budgets (7 interleaved runs, median [min .. max], host load 76 to 84 %)

| budget | ticks over 1 ms | longest run | worst backlog ms | p99.9 ms | max ms | rate needed for every tick under 1 ms |
|---|---|---|---|---|---|---|
| 150 k | 41 [7 .. 46] | 11 [1 .. 32] | 5.4 [0.3 .. 7.3] | 1.64 [1.17 .. 1.77] | 1.80 [1.30 .. 2.07] | 150 M insns/s |
| 200 k | 37 [10 .. 38] | 24 [2 .. 32] | 16.6 [0.7 .. 18.2] | 1.83 [1.30 .. 2.05] | 2.20 [1.58 .. 2.77] | 200 M insns/s |
| 300 k | 26 [23 .. 28] | 22 [21 .. 22] | 27.3 [10.1 .. 28.6] | 2.53 [1.73 .. 2.64] | 2.81 [2.10 .. 2.95] | 300 M insns/s |
| 456 k | 20 [17 .. 20] | 14 [14 .. 14] | 32.8 [16.2 .. 35.5] | 3.60 [2.32 .. 3.80] | 3.86 [2.38 .. 4.32] | 456 M insns/s |

(the min column is the least disturbed run, the max the most disturbed; p50 is 0.08 to 0.10 ms and the mean 0.09 to 0.13 ms at every budget). In executed-instruction units: p50 4.3 k, p99 = budget (150 k, 200 k, 300 k) or 33 k (456 k), p99.9 and max = budget.

**Verdict for "every tick under 1 ms":** it holds at none of the four budgets on this interpreter. The criterion needs a rate of at least the budget in insns/ms, the best observed rate is 185 to 198 M/s, and the heavy ticks vary in cost: even in the least disturbed run at 150 k, 7 ticks exceed 1 ms (max 1.30 ms, backlog 0.3 ms). So "holds at 200 k, not at 456 k" is not the result; it is "not met at 150 k either, closest at 150 k (best run 7 ticks, 0.3 ms), 2.4x short at 456 k". `ns4-lle2` stays unticked.

### 4. A decoupled thread makes the criterion about the backlog, not the tick

`producer.rs` / `consumer.rs`: ARM0 runs on a thread ahead of virtual time, one `TickMsgs` per tick through a bounded queue. The real-time consumer takes exactly ONE tick per millisecond deadline (tick `k` due `k + 1` ms after the start) and counts a missed deadline as an underrun, so the producer's lead is at most the queue length (an earlier version took every ready tick at each deadline and so held about twice the lead: its "0 underruns" is withdrawn). Queue size from the deterministic instruction series and a stated rate (`queue_ticks_for(exec_i / rate)` = `ceil(1.5 x backlog) + 2`, rate 110 M insns/s = the lowest heavy-tick rate seen). `arm-rt-bench`, 5 real-time runs per budget plus a 1-tick queue for contrast, **cold start** (`PREFILL_MS=0`: the first deadline is 1 ms after the producer spawn), host load 79 to 82 %:

| budget | queue (ticks = ms of lead) | deterministic runs (2) vs single-thread reference | underruns per run of 1,900, derived queue (5 runs) | max lateness | 1-tick queue | producer waited on a full queue |
|---|---|---|---|---|---|---|
| 150 k | 25 | digest and state hash equal (`3efde38f525cfbd1` / `2eb2fd59fb12078e`) | 0, 0, 0, 0, 0 | 0 ms | 1 (0.05 %), 0.20 ms | 1,813 to 1,841 |
| 200 k | 42 | equal (`.../6be2c14282a84986`) | 2, 1, 1, 0, 0 | 5.19 ms | 19 (1.0 %), 4.7 ms | 1,790 to 1,815 |
| 300 k | 57 | equal (`a3aab7d387c3f514` / `7cc1638d7d84d519`) | 0, 0, 0, 0, 1 | 0.45 ms | 34 (1.8 %), 11.0 ms | 1,776 to 1,795 |
| 456 k | 69 | equal (`.../8aeb29f222e09e35`) | 1, 0, 0, 1, 1 | 0.36 ms | 52 (2.7 %), 27.4 ms | 1,760 to 1,776 |

* Result: a derived queue gives 0 to 2 underruns per run (11 underruns in 38,000 ticks over the 20 runs, 11 of the 20 runs with none), against 0.05 to 2.7 % for a 1-tick queue. It is not "0": the consumer starts cold. With the consumer started 50 ms after the producer (`PREFILL_MS=50`, the setting of the first version), 200 k gave 0, 0, 0, 0, 0 underruns, so the residue is the start-up (the first deadlines arrive while the producer thread is still spinning up), not the heavy stretch. A real DSP side needs its own start-up prefill of a few ticks.
* "Producer waited on a full queue" counts nearly every tick now: with one tick per deadline the producer is always the one that waits (it is far faster than 1 ms per tick outside the heavy stretch); that is back-pressure working, not a stall.
* A queue sized from ONE lucky single-thread timing run underran (3 ticks at 150 k, earlier consumer), hence the deterministic series and the 110 M insns/s rate; the 1.5 margin and 2 ticks of slack stay ASSUMED.
* The cost of the lead: while the queue is full the producer is up to `queue` ticks ahead, so live MIDI input is applied up to that many ms after it arrived (25 to 69 ms at the derived sizes). Only the program-load stretch (about 6.5 M instructions) needs the big queue; key phases run at 0.07 to 0.17 ms per tick. A cap on the lead outside heavy stretches is a follow-up.
* Determinism: unit test `the_threaded_stream_equals_a_plain_single_thread_loop` compares the producer's per-tick messages and final state hash with a plain `run_tick_idle` + `drain_messages_into` loop for queue sizes 1, 3 and 64 (synthetic machine); `arm-rt-bench` does the same on the real image (digest and state hash, table above). The real-time runs were not compared.
* Failure reporting: a faulting machine delivers the tick's messages so far and `Item::Fault` to both consumers (tests); a producer panic closes the channel without `Done`, reported as `End::Disconnected`, and `finish()` returns the panic message. The panic path itself is not unit-tested.
* Not allocation-free (`mpsc` sends allocate, consumer helpers push into a `Vec`): fine for this offline harness, a real DSP integration needs a preallocated SPSC ring.
* Counter overhead (always-on `RunStats` increments): `arm-lle-bench` `BOOT_ONLY=1` boot rate, parent `5cf37e7d^` vs current, 11 alternating pairs, M executed insns/s, median [min..max]: parent 141.6 [135.9..155.6], current 144.5 [126.9..157.1] (same state hash). Inside the noise band; no cost is claimed or ruled out below it. Shared host, load not recorded for this run.

## Not verified

* `INSNS_PER_TICK = 200000` is ASSUMED; bounded above by clock x IPC (300 k to 456 k), not measured.
* The real-thread latency run is dominated by OS scheduling noise on the shared machine; only the simulated queue (deterministic given the measured tick costs) gives a latency figure.
* The cause of the display-flush count difference (5 more flushes in Python) is not isolated; the per-target split of the 15,337 messages was not computed.
* DSP return layers beyond the SY idle mask and the shadow of the ARM's own messages (live chip, initial image) are not ported (`ns4-lle5`); no key phase reads the SPI return path often enough to measure it here (`DSP return reads` was 0).
* `inject_irq` runs the handler even if the CPSR I bit is set (as the Python harness does); this happened once (`irq_while_masked`). `Cpu::mode_changes` was 0.
* Only OS v1.66 `bank0` (ARM0) and one program were measured, on one machine, shared with other sessions.

## Implications

* A native interpreter with a predecode cache is enough for the mean and for key events; decision [D245](../DECISIONS.md), update [D250](../DECISIONS.md).
* Idle skipping needs no firmware knowledge; it is exact on the 6 benchmark phases at four tick budgets (state and message content), not proven in general.
* A decoupled ARM0 thread absorbs the heavy stretch in a queue of 25 to 69 ticks (derived from the instruction series), at the price of a lead: live MIDI is applied up to that many ms late while the queue is full (25 to 69 ms), and 0 to 2 underruns per 1,900 ticks remain at start-up. A lockstep design inside the 1 ms audio callback would still miss 0.2 to 0.5 % of ticks by up to 0.5 ms.

## Follow-ups

* A further 2x or more needs host-code translation or superblock caching, not more dispatch tuning (not started).
* Repeat the real-thread latency run on a quiet machine with raised Windows timer resolution (`timeBeginPeriod(1)`).
* Pin the instruction budget: calibrate against a firmware delay loop with a known wall-clock duration, or measure the PLL setup the boot loader writes.
* Chip/core routing in `nord-arm` and a per-target comparison with the 7,340 count (`ns4-lle3`).
* Port the live-chip and initial-image return layers (`ns4-lle5`).

## Follow-ups after tail follow-up 2

* Quiet machine: repeat the A/B of the block-chaining, flag-inlining, pair-fusion, RAM-fast-path and flat-address candidates with the host idle; none could be resolved at 65 to 100 % load (below about 25 % is unprovable here).
* Lead cap: let the producer stay only 1 to 3 ticks ahead outside heavy stretches, so live MIDI is not delayed by the program-load queue (25 to 69 ms).
* The 150 k to 456 k budgets are still ASSUMED to bracket the real one; the heavy stretch is a fixed 6.5 M instructions, so the budget decides only how many ticks it spans.
* The real-time consumer here is a spin loop on one core; a real audio callback adds its own jitter.

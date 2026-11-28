# ARM0 runtime (`crates/nord-arm`)

Status: v0.3 (2026-11-28), bean `ns4-lle2`, decisions [D245](../DECISIONS.md), [D250](../DECISIONS.md), [D254](../DECISIONS.md). Measurements: [arm0-rust-runtime](../research/arm0-rust-runtime.md).

Native Rust ARM926EJ-S (ARMv5TE) interpreter that boots the user's OS v1.66 or v1.68 ARM0 image with the hardware the Python harness stubbed. Opt-in crate (no dependencies, not linked by the audio engine). Firmware bytes are never committed: the tests use synthetic in-memory code.

## Layout

| Module | Role |
|---|---|
| `cpu/` | `Cpu`, ARM and Thumb decode (`arm*.rs`, `thumb*.rs`), `alu.rs`, `Bus` trait. WFI (`mcr p15,0,rd,c7,c0,4`) returns `Fault::Wfi`. |
| `cpu/predecode.rs`, `cpu/arm_cached.rs` | Predecode cache entries (`Entry`, `ArmPage`, `decode_arm`, `ends_block`, `COND_MASK`), `Cpu::exec_decoded` / `exec_pure`, and the 128-entry handler table `Table::ARM` (const-generic data-processing handlers). |
| `mem.rs` | 4 GiB sparse paged memory, per-page flags `HOOK_R/W/X/C`, unmapped access maps a zero page, `entry` / `*_e` accessors that reuse one page lookup, page-wise `zero`, `stale_code` list. |
| `machine/mod.rs` | `Machine::run(max, until, stop_pc)`, `call_preserving`, hooks table, null-call guard. |
| `machine/hooks.rs` | `Hook` enum: `Return`, `BumpNew`, `BoardConfig`, `SpiTransfer`, `SkipInsn`, `ZeroR5`, `BoardPresent`, `DspRead`. |
| `machine/boot.rs` | Firmware load at `0xC0000000`, boot stubs, init array, `run_tick`, timer IRQ 22 injection. |
| `machine/sysbus.rs` | Tick counter `0x80001DC0`, GPIO echo, poll breaker, fake DSP ports (path 1 `0xA0011000`, path 2 `0xA0010000`), port store grouping into messages. |
| `machine/{panel,i2c,uart}.rs` | Front panel on CS2, four 64 KB I2C EEPROMs (program memory), UART2 16550 MIDI input. |
| `machine/grouper.rs` | `Grouper`/`Message`: incremental grouping of port stores into ARM to DSP messages, state kept in the bus so a message split across a drain keeps its first words. |
| `machine/system.rs` | `boot_system`, `send_midi`, `drain_messages(_into)` (completed messages since the last call). |
| `machine/dsp_return.rs` | DSP to ARM SPI reads (`0xC00119A4`, `0xC00117E8`, `0xC0066E64`): SY idle voice mask, then shadow of the ARM's own messages, then 0. |
| `machine/idle.rs` | Idle skipping (probe on the cached path through `SysBus` probe mode). |
| `machine/fastrun.rs` | `CodeCache` (decoded pages), `run_cached` (block loop), `step_cached`, `drop_stale_code`. |
| `machine/probe.rs` | `SysBus::probe_read/probe_write/probe_watch`: record whether a probe touched anything impure. |
| `machine/{runstats,producer}.rs` | Run-loop counters; decoupled ARM0 producer thread, bounded queue, deterministic and real-time consumers. |
| `machine/{metrics,paced,micro}.rs` | Benchmark support: percentiles, backlog and queue simulation, message digest; `paced.rs` is the older wall-clock-paced producer/consumer (kept, overlaps `producer.rs`); dispatch micro-benchmark. |
| `machine/{producer,consumer}.rs` | Decoupled producer thread (queue, back-pressure, script and live input, fault reporting) and its deterministic and real-time consumers. |

## OS versions

Firmware addresses (flow control, comm table, `operator new`, stubbed functions, init array, application thunk, main loop, AINTC table, SPI read routines, panel detect and wait ranges, board attach: 27 in all) live in `machine/layout.rs` as one `Layout` per OS version. `Machine::load_firmware` picks the layout whose `image_len` equals the `bank0_CODE` length (`Layout::detect`); an unknown length keeps the layout in force (v1.66 by default). Known: `V166` (5,499,504 B) and `V168` (5,500,432 B). A new version needs one more constant, found with `python tools/fw_addr_map.py OLD_CODE NEW_CODE <old addresses>` ([D257](../DECISIONS.md), [os-1-68-vs-1-66](../research/os-1-68-vs-1-66.md)). Check: `arm-lle-bench` on the v1.68 image boots to `0xC00A029C` after 232,822,685 virtual instructions with 25 null calls and the same message counts per phase as v1.66.

## Time model

One tick = 1 ms virtual = 200,000 instructions (ASSUMED) followed by one timer IRQ. Interrupts are not CPU exceptions: the AINTC handler `IRQ_TABLE[irq]` is called with all registers and the CPSR preserved (sp lowered by `0x800`, lr = `SENTINEL`), as in the Python harness. The low 64 KB is zeroed between ticks and null calls return 0.

## Idle skipping (exact)

From state `S0`, a probe runs up to `MAX_PERIOD = 4096` instructions on an instrumented bus. If the CPU returns to the same registers/CPSR and, meanwhile, no store changed memory, no device page (tick, poll windows, GPIO, DSP ports) was touched, no hook ran and no unmapped page was mapped, the state equals `S0`. Then `k * L` instructions are added to the virtual counter without executing and the remainder runs for real. `WFI` (both `run_wfi`, the full path, and the skipping path) completes the instruction and skips the rest of the budget; `Machine::wfi` counts it. The tick interrupt is the only wake-up modelled (ASSUMED); `inject_irq` ignores the CPSR I bit like the Python harness (`irq_while_masked` counts it). Probes start after 64 executed instructions and back off to 8192. Verified by `arm-lle-bench`: state hash equal to full execution after every phase.

## Predecode cache

`Machine.predecode` (default true). The cached loop is used when no hook is armed on the current page, the CPU is in ARM state and `pc` is word aligned; everything else (Thumb, hooked pages, misaligned pc) uses `Cpu::step`.

* One `ArmPage` of 1,024 `Entry { insn, mask, op, run }` per guest 4 KiB page, decoded whole on the first fetch. `mask` is the condition table row for the 16 NZCV values (`COND_MASK`), `op` the handler index (`op::*`; data processing = `DP_BASE + form * 32 + opcode * 2 + s`), `run` the number of straight-line instructions to the next block ender (B, BL, BX, SLOW, UNCOND, loads or data processing writing r15). A non-ender never changes `pc` except by `+4`, so a block executes without per-instruction limit, pc and hook checks.
* Handler ABI: `fn(&mut Cpu, &mut B, insn) -> bool`; `true` = fault stored in `cpu.fault`. The handlers call the same `dp_exec` / `ldst_exec` helpers as `Cpu::step`, so the two paths share one implementation of each instruction.
* Invalidation: page flag `HOOK_C` marks a page with a decode. A store to it, or `add_flag(HOOK_X)` on it, pushes the page into `Mem::stale_code` and clears the flag; `run` drops stale decodes before the next fetch. A page with a decode is never hooked. B and BL to another decoded page are followed inside the loop; `until` / `stop_pc` guards are checked at block boundaries and before every ender.
* Equivalence is the requirement, not an approximation: `exec_decoded` against `Cpu::step` on 200,000 random words (registers, cpsr, ordered bus accesses), `Machine::run` with `predecode` on and off on 60 random programs including stores into the code, and the benchmark digests (state hash and message content identical with `PREDECODE=0` and with the cache at 100 k, 200 k, 300 k and 456 k instructions per tick; identical to the parent-commit binary at 200 k).
* Measured: 93.5 M to 187 M executed instructions per second on the boot (about 2.0x, interleaved A/B medians); see [arm0-runtime research](../research/arm0-rust-runtime.md). The remaining cost per instruction is memory and device handling, not dispatch.

## Probe mode

`SysBus.probing` makes `probe_read` / `probe_write` run before the device hooks and set `impure` when a probe touches a device page, an executable hook page or a store that changes memory. `probe_idle` runs the candidate loop on the cached path with probing on and compares registers and CPSR (`same_state`); the old `Probe` bus wrapper is gone, so probes run at normal speed (20 M of 232 M boot instructions run in probes).

## Bounds and logs

Capped: `null_calls` (4096, `null_call_count` counts all), `Mem::auto_mapped` (4096, `auto_mapped_count`), `I2cBus::log` (4096), bus messages (`MESSAGE_CAP` 262,144, `dropped_messages`). `run` stops with `Stop::HookLoop` after 64 hook runs without a CPU step. The arena can be reserved with `Mem::reserve_pages` so on-demand mapping does not reallocate while timed. `BumpNew` never frees: heap and arena grow with each `operator new` (not observed to grow during the benchmark phases, 37 MB after boot).

## Run counters

`Machine::stats` (`machine/runstats.rs`, `RunStats`): `cached_entries`, `blocks`, `enders_branch` (B/BL inline), `enders_full` (`exec_decoded`), `stale_exits`, `page_follows`, `plain_steps` (`Cpu::step` path), `hook_runs`. Block-granular `u64` increments, always on, never per instruction and never timed, so they are deterministic for a program and budget; executed instructions come from `cpu.insns` / `IdleStats`. `arm-tail-bench` asserts them equal across runs.

## Decoupled producer thread (`machine/producer.rs`, D254)

| Item | Behaviour |
|---|---|
| Thread | `Producer::spawn(machine, Config { budget, queue_ticks, ticks, script })` moves the machine to its own thread; one `TickMsgs { tick, msgs }` per tick through `sync_channel(queue_ticks)`; `finish()` returns the machine and a `Report` |
| Back-pressure | `try_send` first; a full queue counts `full_events`, then a blocking `send` (time in `blocked`). The producer is at most `queue_ticks` (+1 blocked in `send`) ahead of the consumer |
| Input | script `(tick, bytes)` applied at the start of `tick` (deterministic); `Producer::input(bytes)` applied at the next tick start; both logged in `Report::applied` with the tick they landed on (a live session replays as a script) |
| Deterministic mode | `collect_blocking`: blocking `recv`, no clock read anywhere; the stream and the final state hash depend only on machine, budget, tick count and script, not on queue size or thread timing (tests; real image at 4 budgets) |
| Real-time mode | `consume_realtime(p, t0, ticks)`: at each deadline `t0 + k ms` take every ready tick; a tick not ready counts an underrun and its lateness; drained buffers go back through `Producer::recycle` |
| Queue size | `queue_ticks_for(cost_ms)` = `ceil(1.5 x worst backlog) + 2`, from the deterministic series `exec_i / rate`; ASSUMED margin, rate 110 M insns/s (lowest seen) |
| Allocation | NOT allocation-free: `mpsc` sends allocate and the producer and consumer helpers push into `Vec`s (buffers are recycled, but the path is not proven allocation-free); fine for the offline harness, a DSP integration needs a preallocated SPSC ring |

## Acceptance (`ns4-lle2`)

| Item | Status |
|---|---|
| Opt-in native core, choice recorded | done, D245 |
| Stubs ported, no Python callbacks | done (timer, ports, SPI read return partial: `ns4-lle5` for live layers) |
| Idle / WFI / poll fast-forward | done; state and message content equal to full execution on the 6 benchmark phases |
| Mean below 1.0 ms/tick on real boot + load + keys | done: 0.09 to 0.13 ms (tail follow-up 2 table) (0.20 to 0.34 before the predecode cache) |
| Every tick below 1.0 ms | NOT met at any of the four budgets (7 interleaved runs, host load 76 to 84 %): 150 k: 41 [7 .. 46] ticks of 1,900 over 1 ms, max 1.80 [1.30 .. 2.07] ms, backlog 5.4 [0.3 .. 7.3] ms; 200 k: 37 [10 .. 38], max 2.20, backlog 16.6; 300 k: 26, 2.81, 27.3; 456 k: 20, 3.86, 32.8. Cause: every over-budget tick executes the whole budget (one non-idle stretch of about 6.5 M instructions), so the tick cost is budget / rate and the criterion needs a rate of at least 150, 200, 300, 456 M insns/s against 185 to 198 M at best. Unticked ([D254](../DECISIONS.md)) |
| Message latency through a decoupled queue | done on the real image with a producer thread ([D254](../DECISIONS.md), `arm-rt-bench`): queue 25, 42, 57, 69 ticks at 150 k, 200 k, 300 k, 456 k gives 0 to 2 underruns of 1,900 ticks per run in 5 cold-start real-time runs each (one tick per deadline; 11 in 38,000 ticks over 20 runs; with a 50 ms prefill 0 in 5 runs at 200 k); a 1-tick queue underran 0.05 to 2.7 % (up to 27 ms late). The first version (consumer took every ready tick, hidden 50 ms prefill) reported 0 underruns; that claim is withdrawn Live MIDI is applied up to the queue length late while it is full |
| Instruction budget per ms | bounded, not measured: <= clock x IPC (300 k to 456 k); 200 k stays ASSUMED; sensitivity measured at 100 k, 200 k, 300 k, 456 k |
| Synthetic tests only | done |

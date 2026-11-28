---
# ns4-lle2
title: 'Rust ARM0 runtime with idle-skipping (replace Python/Unicorn harness)'
status: in-progress
type: task
priority: high
created_at: 2026-11-23T21:40:00Z
updated_at: 2026-11-24T12:00:00Z
parent: ns4-lle1
---

## Context
`docs/research/pno-jit-arm-profile.md` measured the Python/Unicorn ARM0 harness at 18–24 ms per 1 ms tick (8–11 ms even with all 34 Python hooks removed) against an ASSUMED 200,000 instructions/ms budget. Real-time LLE requires ARM0 to run inside Rust with zero Python/IPC overhead and with idle/poll-loop skipping.

## Higher Goal
ARM0 boots the user's OS update image in Rust and advances in real time (< 1 ms wall-clock per 1 ms virtual time) while emitting live ARM→DSP messages and accepting DSP→ARM reads.

## Acceptance Criteria
- [x] Embed ARM0 execution in Rust (e.g., `unicorn-engine` Rust bindings or native Rust ARMv7/Thumb core) behind an opt-in feature/crate
- [x] Port the essential hardware stubs from `tools/arm_org_stream.py` / `tools/arm_dsp_return*.py` (timer IRQ, DSP mailboxes, SPI peek return path) to Rust with zero Python callbacks
- [x] Implement idle / `WFI` / poll-loop fast-forwarding so ARM0 does not burn full 200k-instruction ticks while waiting for the next timer interrupt
- [x] Benchmark wall-clock cost per 1 ms tick on a real OS v1.66 boot + program load + key events and record measurements in `docs/research/` (done: mean 0.20 to 0.34 ms)
- [ ] Every tick under 1.0 ms (real-time headroom): NOT met, 4.8 % of load ticks over 1 ms, up to 32 in a row (open; split out of the criterion above). Re-measured 2026-11-24 with the predecode cache: 4 to 9 of 1,900 ticks (0.2 to 0.5 %) over 1 ms, max 1.5 ms, at most 3 in a row, worst backlog up to 0.55 ms; closer, still not every tick, so unticked
- [x] Unit tests with synthetic ARM code fixtures in memory (no firmware bytes committed)

## Out of Scope
Wiring live GUI controls (`ns4-lle6`); multi-chip SHARC audio routing (`ns4-lle4`).

## Summary of Changes
- New crate `crates/nord-arm`: native ARM926EJ-S interpreter (ARM + Thumb), sparse memory, boot hooks, tick/GPIO/poll devices, panel, I2C EEPROMs, UART2 MIDI, incremental port grouping, DSP to ARM SPI return, exact idle skipping, one `WFI` semantic in both run paths. Decision D245 (Unicorn rejected: GPLv2, callbacks).
- `arm-lle-bench` (`nord-tools`): OS v1.66 boot + program load + key events; timed region includes MIDI send, tick run and message drain. Mean 0.20 to 0.34 ms/tick. NOT every tick under 1 ms: 4.8 % of load ticks (longest run 32 ticks, backlog 73 ms) and 0.5 to 1.0 % of key-event ticks; all are real 200k-instruction ticks (no device polling, no arena growth). State hash and message content equal to full execution after every phase.
- Review fixes: port grouping state persists across drains, stub returns set Thumb state from lr, logs capped, provenance tags, panics on bad input removed.
- Docs: `docs/research/arm0-rust-runtime.md`, `docs/specs/arm0-runtime.md`, README index, ROADMAP Gate 1a split (core done, every-tick-under-1-ms open).
- Open (bean stays in-progress): predecode cache for about 5x, message-latency/queue sizing, real instructions per ms, null-call 747 vs 762, live DSP return layers (`ns4-lle5`).

### Follow-ups (2026-11-24, D250)
- Predecode cache (`cpu/predecode.rs`, `cpu/arm_cached.rs`, `machine/fastrun.rs`, page flag `HOOK_C`, probe mode): boot median 93 M to 187 M (interleaved A/B, 7 runs each) executed insns/s (about 2.0x, not the 5x target); state hash and message digests identical between `PREDECODE=0` and the cache at tick budgets 100 k, 200 k, 300 k, 456 k, and identical to the parent-commit binary at 200 k.
- Tail (5 runs): mean 0.11 ms (load), p99 0.97 to 1.0, p99.9 1.25 to 1.39, max 1.43 to 1.53; 4 to 9 of 1,900 ticks over 1 ms, longest run 1 to 3, worst backlog 0.42 to 0.55 ms (was 38 ticks, run 32, 61 ms). Criterion NOT ticked.
- Queue: simulated drain per ms gives max message latency 1 ms, at most 2 ticks / 1,855 messages per drain at the ASSUMED 200 k budget (at 300 k: 21 ticks in a row, 9.4 ms backlog; at 456 k: 14, 17 ms) (was p99 59 ms). Real-thread run (`PACED=1`) is OS-scheduler noise on the shared machine.
- Budget: `INSNS_PER_TICK = 200000` stays ASSUMED; bound clock x IPC <= 300 k to 456 k per ms; sensitivity 100 k to 456 k in the research note (load tail: 1, 4 to 8, 23, 16 ticks over 1 ms).
- Null calls 747 vs 762: same 68 sites, 65 equal; three display-flush sites 8 vs 3 (Python flushes 5 more times). Messages 15,337 vs 7,340: all targets against PNO core 1 path 2 only (consistent with the counts, not proven; no per-target split). Documented, not fixed.
- Still open: 5x needs host-code translation or superblocks; quiet-machine latency run; real instruction budget; per-target message split; live DSP return layers (`ns4-lle5`).

### Tail follow-up 2 (2026-11-28, D254)
- Counters (`Machine::stats`) and `arm-tail-bench`: every over-budget tick executes exactly the whole instruction budget. One non-idle stretch of about 6.5 M instructions in the load phase (44 ticks at 150 k, 33 at 200 k, 21 at 300 k, 14 at 456 k), 75 % of it in 2 KB at `0xC025C700..0xC025CE80` (conditional register arithmetic, looks like software floating point). Not idle, not polling, not invalidation (3.8 k exits per 13.7 M instructions) and not hooks (1.9 % plain steps).
- Per-instruction cost: lazy `bus.pc` was inside the noise band (31.06 [19.84 .. 31.91] vs 31.43 [21.23 .. 33.37] thread cycles/insn, 9 interleaved pairs) and was reverted; the other candidates were not started because a gain below about 25 % is unprovable at 65 to 100 % host load. Dispatch ceiling 375 M insns/s; firmware 185 to 198 M at best, 124 to 129 M median on the loaded host.
- Tail (7 interleaved runs, median [min .. max] ticks over 1 ms of 1,900): 150 k 41 [7 .. 46], 200 k 37 [10 .. 38], 300 k 26 [23 .. 28], 456 k 20 [17 .. 20]; max tick 1.8, 2.2, 2.8, 3.9 ms. The "every tick under 1 ms" criterion holds at none of them (needs at least budget M insns/s). Left UNTICKED.
- Decoupled thread: `machine/{producer,consumer}.rs`, `arm-rt-bench`. Queue 25, 42, 57, 69 ticks (from the deterministic instruction series at 110 M insns/s, margin 1.5 + 2) gives 0 to 2 underruns of 1,900 ticks per cold-start real-time run (one tick per deadline; 0 of 5 with a 50 ms prefill at 200 k; the earlier "0 underruns" is withdrawn), against 0.05 to 2.7 % with a 1-tick queue (up to 27 ms late). Deterministic runs equal the single-thread reference in message digest and state hash at all four budgets; tests cover queue sizes 1, 64 and 2 with a slow consumer. Cost: live MIDI is applied up to the queue length late while it is full (lead cap is a follow-up).

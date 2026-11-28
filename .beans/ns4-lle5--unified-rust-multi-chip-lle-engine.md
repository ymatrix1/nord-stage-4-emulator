---
# ns4-lle5
title: 'Unified Rust multi-chip LLE engine (ARM0 + 5 DSP images)'
status: in-progress
type: task
priority: normal
created_at: 2026-11-23T21:40:00Z
updated_at: 2026-11-27T18:00:00Z
parent: ns4-lle1
blocked_by:
    - ns4-lle2
    - ns4-lle4
---

## Context
Once ARM0 runs in Rust (`ns4-lle2`), SHARC JIT/AOT meets real-time speed (`ns4-lle3`), and the inter-chip audio bus + SDRAM work (`ns4-lle4`), all six processors need to run together inside a single lock-free, real-time-safe Rust engine crate/module.

## Higher Goal
A single Rust `LleEngine` loads the user's OS update file and `.ns4b` backup at startup and renders real-time 48 kHz stereo audio with zero heap allocations or mutex locks inside `process_block()`.

## Acceptance Criteria
- [ ] Create a unified Rust `LleEngine` combining ARM0 (`ns4-lle2`), all 5 SHARC DSP images (`ORG`, `PNO`, `FX`, `SY1SY2`, `SMP`), live ARM↔DSP mailbox & return path (`ns4-nds8`), and `SMP` CNSP page refill (`ns4-pnoc`)
- [ ] Enforce real-time audio invariants: pre-allocate all SRAM/SDRAM/ring buffers at init; zero heap allocations, locks, or disk I/O inside `process_block()`
- [ ] Add a CLI render smoke test (`ns4 render-lle` or `nord-tools` binary) that boots a user-supplied OS image and renders stereo audio end-to-end
- [ ] Unit tests with synthetic in-memory fixtures verifying lockstep scheduling and zero-allocation audio rendering

## Out of Scope
GUI/plugin UI wiring (`ns4-lle6`, `ns4-lle7`).

## Summary of Changes (2026-11-27, partial)
New crate `crates/nord-lle` (virtual-clock engine over ARM0 and the DSP images (PNO core 1 and 2, ORG core 2, SY, SMP, FX 1, FX 2; ORG core 1 is not loaded), routing, return path via `nord-arm`, SMP refill hook, input and telemetry traits, directory loader), `BusNode::post` / per-node time and input and output levels in `nord-sharc`, binary `lle_engine_run`. Spec [lle-engine](../docs/specs/lle-engine.md), run [lle-engine-run](../docs/research/lle-engine-run.md), D253.

Result on local images: boots, no fault in 4,000 ticks, two runs equal in ARM state, DSP core state and counters (the digest hashes both since the review), 100x to 108x slower than real time on a busy host, **all outputs silent, audio all zero (cause open)**. Of the 8 DSP cores 7 are loaded, 6 are driven by ARM messages and the ARM addresses 7; SY is loaded but idle (no message addressed to it); ORG core 1 is not loaded (it does not return to the poll loop under the `org` preset) and its 912 messages are counted without a chip; PNO core 2 is loaded. Silence bisect (see the run note): every node input and output stays 0.0 for 0.5 s of virtual time, with the factory-restore EEPROM and with the B3 factory program recalled; a standalone PNO core 1 with the known Vox script is audible (peak 0.117), so the preset, tap and wiring are fine; after the key no key flag or weight word reaches PNO core 1, only housekeeping. Missing data: the ARM-side key-flag and weight sender (open in `pno-full-pass.md`) or an EEPROM state that reaches it. Criteria 1 (engine composed, return path has two of four layers) and 3 (smoke test runs, no audio) are partly met and stay unticked; 2 (no alloc/locks in `process_block()`) is not met (offline engine); 4 (synthetic tests) has scheduling, routing, return, refill, determinism and fault tests, not zero-allocation.

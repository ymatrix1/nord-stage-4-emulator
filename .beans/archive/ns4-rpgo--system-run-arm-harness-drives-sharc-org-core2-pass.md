---
# ns4-rpgo
title: 'System run: ARM harness drives SHARC ORG Core2 passes (D119)'
status: completed
type: task
priority: normal
created_at: 2026-11-16T23:15:26Z
updated_at: 2026-11-16T23:26:05Z
parent: ns4-trkb
---

## Context
Nobody has measured the ARM side, DMA, interrupts or the other cores together with the SHARC pass loop, so real-time feasibility of the whole system is unknown (#463, #464).

## Higher Goal
Audio 1:1 with the hardware via the SHARC oracle plus the ARM Unicorn harness; first system-level measurement.

## Acceptance Criteria
- [x] D119 + `docs/specs/sharc-core.md` record the scope change (proposed)
- [x] Real ARM message streams (program load + idle, 1 key, 8 keys) captured and fed into the pass runner; coverage and step counts compared with the poke proxy
- [x] State comparison real vs proxy vs `sharc-org` hand-written messages
- [x] Per-component timing table (ARM, SHARC, stub/DMA, message delivery), 300 passes after 16 warm-up
- [x] Research note `docs/research/org-system-run.md`; checks pass

## Out of Scope
JIT, AOT, multi-core, porting the ARM side to Rust, whole-pass anything, default behaviour changes, merging PRs.

## Summary of Changes
- `sharc-sys` + `runner/system.rs`: message replay between interpreter passes, per-component timing, state and coverage comparison.
- `tools/arm_org_stream.py`, `tools/org_system_timing.py`: real ARM stream capture, timing aggregation.
- D119 (proposed), `docs/specs/sharc-core.md` section 7, `docs/research/org-system-run.md`.
- Finding: keys never reach ORG Core2; real load runs 6,377 steps per pass and faults at pass 15 without an external-port DMA stub.

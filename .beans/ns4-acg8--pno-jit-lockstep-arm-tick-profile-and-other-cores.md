---
# ns4-acg8
title: PNO JIT lockstep, ARM tick profile and other-cores measurements (D119)
status: completed
type: task
priority: normal
created_at: 2026-11-17T01:36:57Z
updated_at: 2026-11-17T01:37:08Z
---

Context: follow-up to the first system run (#468). Higher Goal: know whether the JIT, the ARM harness and the other cores can join a multi-core system run. Acceptance Criteria: - [x] PNO core 1 JIT lockstep 316 passes - [x] ARM tick profile - [x] other-cores table - [x] note + PR. Out of Scope: default changes, AOT/jit files.

## Summary of Changes

- `runner/system_diff.rs` + `sharc-sys --jit-diff/--irq/--flag1`: JIT vs interpreter lockstep over the system run; PNO core 1 matches 316/316 passes for load, 1 key, 8 keys.
- `tools/arm_profile.py`: ARM tick split (Python hooks, interrupt injection, Unicorn); the 17x is mostly the harness.
- `tools/arm_org_stream.py`: scripts for every path-2 receiver; other-cores table.
- Note `docs/research/pno-jit-arm-profile.md`, data under `docs/research/data/`.

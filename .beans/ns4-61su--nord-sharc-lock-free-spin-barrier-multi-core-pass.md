---
# ns4-61su
title: 'nord-sharc: lock-free spin-barrier multi-core pass scheduler'
status: todo
type: task
created_at: 2026-11-16T21:59:24Z
updated_at: 2026-11-16T21:59:24Z
parent: ns4-zp6a
---

## Context
Full Nord Stage 4 runs multiple SHARC cores (ORG C1/C2, PNO, SY, FX); a 46.08 us pass cannot afford OS mutex/park jitter.

## Higher Goal
Run 1 dedicated host thread per SHARC core synchronized via lock-free atomics / spin-barrier.

## Acceptance Criteria
- [ ] Pre-spawned worker threads (1 per SHARC core) with zero allocations or OS locks in the pass loop.
- [ ] Configurable batching (1..64 passes per barrier) and deterministic 1:1 output test.

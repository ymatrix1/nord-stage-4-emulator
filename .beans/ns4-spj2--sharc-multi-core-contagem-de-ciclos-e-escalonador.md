---
# ns4-spj2
title: 'SHARC multi-core: contagem de ciclos e escalonador 1 thread por core'
status: completed
type: task
priority: normal
created_at: 2026-11-16T17:36:03Z
updated_at: 2026-11-16T17:39:08Z
parent: ns4-trkb
blocked_by:
    - ns4-z2fq
---

Implemented Core::run_for_cycles and MultiCoreScheduler (1 dedicated OS thread per SHARC Core, lockstep cycle quanta, inter-core mailbox poke/peek, and clean Drop shutdown).

## Summary of Changes
- crates/nord-sharc/src/core/mod.rs: added Core::run_for_cycles(cycles, use_jit).
- crates/nord-sharc/src/lib.rs: exported pub mod scheduler.
- crates/nord-sharc/src/scheduler.rs: added MultiCoreScheduler and unit tests (four_cores_run_in_lockstep_and_exchange_mailboxes, worker_fault_is_reported_with_core_index).
- crates/nord-sharc/src/core/jit_diff.rs: added bench_multicore_4x8_32_voices.

---
# ns4-z2fq
title: 'SHARC Nivel 2: promover i[ii] e mr[reg] para SSA em loops JIT'
status: completed
type: task
priority: normal
created_at: 2026-11-16T17:30:21Z
updated_at: 2026-11-16T17:35:27Z
parent: ns4-trkb
blocked_by:
    - ns4-wmdv
---

Promoted loop-carried i[ii] and mr[reg] registers to Cranelift SSA Variables across eligible DO loops via LoopSsaVars.

## Summary of Changes
- crates/nord-sharc/src/core/jit.rs: added LoopSsaVars (flush/reload), is_inline_compute_for_ssa, can_promote_loop_ssa, and preload_loop_ssa; wired SSA Variables into emit_fixed_mult, emit_jit_compute_op, emit_inline_sram_dag_transfer, emit_inline_dual_sram_load, and get_or_compile_jit_block.

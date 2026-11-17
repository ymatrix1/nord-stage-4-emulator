---
# ns4-zp6a
title: 'SHARC Real-Time: Whole-Pass AOT + Multi-Core Scheduler'
status: todo
type: epic
created_at: 2026-11-16T21:58:55Z
updated_at: 2026-11-16T21:58:55Z
---

## Context
Profile (#464) and ceiling study (#465) on real ORG Core2 firmware show one pass = 46.08 us (92.6 / 97.1 / 129.2 Msteps/s for idle / 1 voice / 8 voices). Basic-block JIT leaves 56.7% of steps in Core::step() fallback even with SIMD (~11-16 Msteps/s ceiling).

## Higher Goal
Reach real-time (<46.08 us/pass, >=130 Msteps/s per SHARC core) executing unmodified Nord SHARC firmware 1:1 across all DSP cores.

## Acceptance Criteria
- [ ] Whole-pass AOT (sharc-aot -> ustc -O) covers ORG Core2 pass (SIMD + Move(Other/DirectMem/RegSwap) + ShiftImm + Multifunction + Flow) with 0 divergences vs Core::run.
- [ ] 300-pass warmed-up ORG Core2 8-voice pass runs <= 46.08 us/pass (>= 129.2 Msteps/s).
- [ ] Lock-free multi-core pass scheduler runs 1 dedicated host thread per SHARC core without OS mutex/allocation overhead.

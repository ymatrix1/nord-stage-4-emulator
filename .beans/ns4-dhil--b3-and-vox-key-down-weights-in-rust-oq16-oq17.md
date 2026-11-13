---
# ns4-dhil
title: B3 and Vox key-down weights in Rust (OQ16, OQ17)
status: completed
type: task
created_at: 2026-11-13T19:20:55Z
updated_at: 2026-11-13T19:20:55Z
---

## Context
Captured key-down data (PR #143) gives the B3 per-wheel amplitude A, per-key footage weights W and the Vox key to row map.

## Higher Goal
Make the organ firmware paths the B3 and Vox defaults (D22: only when verified end to end).

## Acceptance Criteria
- [x] B3Wheels applies A and W (derived tables, tags)
- [x] Vox uses the captured four-footage key map, no velocity
- [x] Oracle comparison per partial (B3 +12.7..+31.8 dB, Vox 31 of 39 lines within 1 dB)
- [ ] Defaults flipped (criterion not met: stays opt-in)
- [x] emu_vs_hw re-run for B3 and Vox

## Out of Scope
B3 processor tail, Vox sine bank and start phases.

## Summary of Changes
See docs/research/organ-keydown-weights.md.

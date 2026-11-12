---
# ns4-ak3v
title: 'FX EQ/Amp: biquad EQ, hardware EQ freq table, Amp Sim modes, FX morph (PR 9)'
status: in-progress
type: task
created_at: 2026-11-12T01:46:46Z
updated_at: 2026-11-12T01:46:46Z
parent: ns4-trkb
---

## Context
LayerFxChain used 1-pole EQ, an invented British cabinet, a geometric EQ freq curve and ignored FX morph deltas.

## Higher Goal
Match hardware FX behaviour using firmware data with provenance tags (see .agents/rules/provenance-in-code.md).

## Acceptance Criteria
- [x] RBJ biquad EQ/cabinet, stereo preserved
- [x] 128-step EQ freq table (EMPIRICAL, ns4decode)
- [x] Amp modes 0..9 incl. LP24/HP24 (names VERIFIED at 0x2cc6cc)
- [x] FX morph wired for 9 params
- [ ] Delay-time (14-bit) morph
- [ ] Hardware audio validation (see bean on Bank A comparison)

## Out of Scope
Firmware 18x128 biquad table (see its own bean).

## Summary of Changes
PR https://github.com/ymatrix1/nord-stage-4-emulator/pull/9

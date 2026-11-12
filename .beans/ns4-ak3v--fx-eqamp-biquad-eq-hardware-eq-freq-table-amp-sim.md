---
# ns4-ak3v
title: 'FX EQ/Amp: biquad EQ, hardware EQ freq table, Amp Sim modes, FX morph (PR 9)'
status: completed
type: task
priority: normal
created_at: 2026-11-12T01:46:46Z
updated_at: 2026-11-12T02:39:45Z
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
- [x] Delay-time morph and hardware audio validation moved to ns4-13ua and ns4-t5zr

## Out of Scope
Firmware 18x128 biquad table (see its own bean).

## Summary of Changes
Merged in https://github.com/ymatrix1/nord-stage-4-emulator/pull/9 (`fx.rs` biquads, `EQ_FREQ_HZ`, Amp modes 0..9, `FxMorphSet`; `ns4 render-patch --events/--program-file`; `video_ref.py notes/scenes`).

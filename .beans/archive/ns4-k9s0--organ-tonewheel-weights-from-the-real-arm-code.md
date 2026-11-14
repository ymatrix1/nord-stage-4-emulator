---
# ns4-k9s0
title: Organ tonewheel weights from the real ARM code
status: completed
type: task
priority: normal
created_at: 2026-11-13T11:49:14Z
updated_at: 2026-11-13T11:49:22Z
parent: ns4-trkb
---

## Context

Section 10 of arm-organ-messages.md saw all drawbar and vibrato words as 0 and guessed missing SPI weight tables.

## Higher Goal

Decode the drawbar to per-wheel weight path so the Rust organ matches the firmware.

## Acceptance Criteria

- [x] Find who sends a, b, wB (and wA)
- [x] Decode footage offsets, foldback, key range, tapering, curve
- [x] Replay ARM burst into the PNO core 1 image and measure
- [x] Document in docs/research/organ-weights.md with Rust to-do list

## Out of Scope

Percussion consumer, source of the engine drawbar indices, organ velocity source.

## Summary of Changes

- tools/arm_organ_weights.py (keymap, verify, curve, script) and tools/organ_weights_replay.py with tests.
- docs/research/organ-weights.md, data CSVs and replay scripts; cross-references in arm-organ-messages, organ-oracle-audio, organ-engine spec, OPEN-QUESTIONS.
- drawbar.rs: DRAWBAR_OFFSETS and foldback tagged VERIFIED, test against the firmware key tables.

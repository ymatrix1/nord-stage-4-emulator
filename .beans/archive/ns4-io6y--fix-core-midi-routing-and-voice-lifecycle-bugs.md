---
# ns4-io6y
title: Fix core MIDI routing and voice lifecycle bugs
status: completed
type: task
priority: normal
created_at: 2026-11-11T12:05:52Z
updated_at: 2026-11-11T12:17:49Z
parent: ns4-707f
---

## Context
The audit identified reproducible engine bugs in split gain, Note Off routing, piano release, and synth arpeggiation.

## Higher Goal
Correct core MIDI, split, and voice behavior without changing instrument velocity response or interrupting unrelated layers.

## Acceptance Criteria
- [x] Add regression tests for each corrected bug where feasible
- [x] Preserve MIDI velocity while applying split gain to rendered note signals
- [x] Route Note Off to layers that received the corresponding Note On
- [x] Release piano voices with a damping envelope
- [x] Correct Poly arp behavior and use the synth voice allocator
- [x] Update the affected subsystem specifications
- [x] Run project quality gates and record output

## Out of Scope
- Hardware-fidelity validation, MIDI channel assignment, broader DSP redesign

## Summary of Changes
- Split gain is applied per note for Organ, Piano, and Synth while velocity remains unchanged.
- The engine records Note On recipients and routes matching Note Off events to those layers across scene changes and repeated notes.
- Piano key and damper pedal release starts a 120 ms damping phase.
- Synth Poly triggers held chords; arp steps use the regular voice allocator and Random uses a seeded pseudo-random sequence.
- Regression tests cover split gain, note recipients, piano release, Poly chords, and allocator behavior.
- Full quality gates passed on the updated main branch: fmt, Clippy, Rust workspace tests, Ruff, pytest, and test ratchet.

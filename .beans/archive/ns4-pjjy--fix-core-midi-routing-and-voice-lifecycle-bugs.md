---
# ns4-pjjy
title: Fix core MIDI routing and voice lifecycle bugs
status: completed
type: task
priority: normal
created_at: 2026-11-11T12:13:41Z
updated_at: 2026-11-11T12:13:49Z
parent: ns4-707f
---

## Context
The audit identified reproducible engine bugs in split gain, Note Off routing, piano release, and synth arpeggiation.

## Higher Goal
Correct core MIDI, split, and voice behavior without changing instrument velocity response or interrupting unrelated layers.

## Acceptance Criteria
- [x] Add regression tests for each corrected bug
- [x] Preserve MIDI velocity while applying split gain to rendered note signals
- [x] Route Note Off to layers that received the corresponding note
- [x] Release piano voices with a damping envelope
- [x] Correct Poly arp behavior and use the synth voice allocator
- [x] Update the affected subsystem specifications
- [x] Run project quality gates

## Out of Scope
- Hardware-fidelity validation, MIDI channel assignment, broader DSP redesign

## Summary of Changes
- Split gain is applied per note for organ, piano, and synth layers while velocity remains unchanged.
- The engine records Note On recipients and routes matching Note Off events to those layers.
- Piano key and pedal release starts a 120 ms damping phase.
- Synth Poly now triggers held chords; arp steps use regular voice allocation and Random uses a seeded pseudo-random sequence.
- Added regression tests for split gain, scene changes during held notes, piano release, Poly chords, and voice allocation.

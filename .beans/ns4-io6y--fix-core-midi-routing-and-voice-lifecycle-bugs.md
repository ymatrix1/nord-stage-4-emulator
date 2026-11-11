---
# ns4-io6y
title: Fix core MIDI routing and voice lifecycle bugs
status: in-progress
type: task
created_at: 2026-11-11T12:05:52Z
updated_at: 2026-11-11T12:05:52Z
parent: ns4-707f
---

## Context
The audit identified reproducible engine and voice lifecycle bugs in split gain, Note Off routing, piano release, and synth arpeggiation.

## Higher Goal
Make core layer behavior respect MIDI and split semantics while preventing abrupt piano voice termination and arpeggiator voice overwrites.

## Acceptance Criteria
- [ ] Add a regression test for each corrected bug where feasible
- [ ] Preserve MIDI velocity while applying split gain to rendered audio
- [ ] Route Note Off only to layers that received matching Note On
- [ ] Release piano voices with a damping envelope
- [ ] Correct Poly arp allocation and deterministic Random behavior
- [ ] Update affected specs
- [ ] Run relevant checks and record output

## Out of Scope
- Full hardware fidelity, channel assignment configuration, broad DSP redesign

## Summary of Changes
Pending

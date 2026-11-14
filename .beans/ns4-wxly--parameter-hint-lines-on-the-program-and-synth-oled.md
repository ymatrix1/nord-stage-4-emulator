---
# ns4-wxly
title: Parameter hint lines on the Program and Synth OLEDs
status: todo
type: task
created_at: 2026-11-14T03:34:14Z
updated_at: 2026-11-14T03:34:14Z
---

## Context
Hardware shows hints while a control moves: synth OLED top row becomes LFO Rate 0.08 Hz / LFO Destination OscPitch / LFO Amount 7.1 (LFO video 0:27-0:45); Program display lower half shows hints (manual p.42). Not implemented in either OLED. See docs/research/dsoundman-course-findings.md.

## Higher Goal
Match hardware display behaviour.

## Acceptance Criteria
- [ ] Behaviour implemented with tests
- [ ] Spec and provenance tags updated

## Out of Scope
Engine changes unrelated to the display.

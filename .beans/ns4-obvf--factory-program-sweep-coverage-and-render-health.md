---
# ns4-obvf
title: 'Factory program sweep: coverage and render health'
status: in-progress
type: task
created_at: 2026-11-14T02:52:23Z
updated_at: 2026-11-14T02:52:23Z
---

## Context
Which engine paths do the 384 factory programs rely on, and which are firmware-verified, assumed or not modelled?

## Higher Goal
Know where the engine is still on assumed paths before claiming coverage.

## Acceptance Criteria
- [x] tools/program_sweep.py with tests
- [x] every program rendered with defaults and --firmware-all --firmware-routing
- [x] docs/research/program-coverage.md and data/program-sweep.json
- [x] panics/NaN checked; small fixes with tests

## Out of Scope
Audio-quality claims, level calibration, scene-mask semantics (OQ24).

## Summary of Changes
See docs/research/program-coverage.md.

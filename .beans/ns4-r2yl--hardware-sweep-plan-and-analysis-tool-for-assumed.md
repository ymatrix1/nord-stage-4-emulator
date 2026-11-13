---
# ns4-r2yl
title: Hardware sweep plan and analysis tool for ASSUMED synth scales
status: completed
type: task
created_at: 2026-11-12T02:35:22Z
updated_at: 2026-11-12T02:35:22Z
---

## Context
After the Synth parameter work (PR #8) glide, vibrato, unison cents, osc-env pitch depth and amp-velocity steps are still ASSUMED because the firmware holds no table for them.

## Higher Goal
Replace each ASSUMED Synth scale with a measured, reproducible value (OQ7).

## Acceptance Criteria
- [x] `tools/sweep_analyze.py` (glide, vibrato, unison, excursion, level) with tests on synthetic signals
- [x] `docs/research/synth-hardware-sweep-plan.md` maps every ASSUMED constant to a capture and a command
- [ ] Record the sweeps on hardware and update the constants (needs a real Stage 4)

## Out of Scope
Doing the hardware recording; filter-envelope depth tool.

## Summary of Changes
- Pure-Python pitch tracker (interpolated zero crossings), constant-rate glide fit, vibrato rate/depth, FFT unison spread, pitch excursion, RMS level.
- Sweep plan with setup, per-sweep procedures and how to record results.

---
# ns4-sh34
title: 'Synth stages batch 11: SHAPE 3 and 4 stages; SUPER 2..4 blocker'
status: completed
type: task
created_at: 2026-11-14T18:00:00Z
updated_at: 2026-11-14T18:00:00Z
---

## Context
Ten ANALOG layers had no stage model: SHAPE 3 (0x120F55), SHAPE 4 (0x12145E) and SUPER 2..4 (0x124985, 8 layers).

## Higher Goal
1:1 synth stage parity with the firmware (OQ22, TD-11).

## Acceptance Criteria
- [x] 0x120F55 and 0x12145E bit exact against sharc-org (320 of 320 words each over 40 trials, s5 and pitch varied)
- [x] Rust FwStage::Shape3 / Shape4, golden tests, opt-in wiring behind --synth-stages
- [x] Recorded oracle cases, Python tests, docs (synth-stage-models-11, ROADMAP, OQ22, TD-11, D53)
- [ ] SUPER 2..4 0x124985 modelled (blocked: wavetable at 0x08030000 is not in the boot log)

## Out of Scope
SUPER 2..4 port; SAMPLE / WAVE banks.

## Summary of Changes
See docs/research/synth-stage-models-11.md. 304 of 312 ANALOG layers modelled; SUPER 2..4 (8 layers) open.

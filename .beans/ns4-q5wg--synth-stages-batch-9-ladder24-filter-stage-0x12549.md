---
# ns4-q5wg
title: 'Synth stages batch 9: Ladder24 filter stage 0x12549D and fw_stages split'
status: completed
type: task
created_at: 2026-11-14T16:34:02Z
updated_at: 2026-11-14T16:34:02Z
---

## Context
Ladder24 (kFltType 2, 68 filter layers) was the last filter stage without a model; fw_stages.rs was over 400 lines.

## Higher Goal
1:1 synth stage parity with the firmware (OQ22, TD-11).

## Acceptance Criteria
- [x] 0x12549D bit exact against sharc-org (1840 of 1840 words over 40 trials)
- [x] Rust FwFilterType::Ladder, golden tests, opt-in wiring in the strip chain
- [x] fw_stages split into a directory module, tests unchanged
- [x] Docs (synth-stage-models-9, filter type map, ROADMAP, OQ22, TD-11, D40)

## Out of Scope
SUPER 2..4, SHAPE 3 / 4; ARM cutoff word for type 2.

## Summary of Changes
See docs/research/synth-stage-models-9.md.

---
# ns4-4wzo
title: 'Organ: B3 Bass key-down weights for every key; Bass stage blockers'
status: in-progress
type: task
created_at: 2026-11-14T13:04:37Z
updated_at: 2026-11-14T13:04:37Z
---

## Context
B3 Bass (model 5) table and wiring (ns4-b511 follow-up, organ-farfisa-bass s9).

## Higher Goal
1:1 audio parity of the organ models.

## Acceptance Criteria
- [x] Key-down weights of model 5 decoded for every key from the ARM (organ_bass_keys.py, b3_bass_keys.rs, bass-keydown.csv)
- [x] Oracle chain tool for the Bass processor (organ_bass_proc.py); window 1 inaudible, window 2 carries the bass
- [ ] Line-gain table: not valid, the output depends on per-key lattice words (s10)
- [ ] Opt-in Bass stage with oracle chord goldens within 1 dB
- [ ] B3 clip comparison (emu_vs_hw.py) and default decision

## Out of Scope
Bit-exact 0x122296 model.

## Summary of Changes
Weights table and findings only; no render change (render_hash unchanged).

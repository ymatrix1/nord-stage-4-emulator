---
# ns4-4wzo
title: 'Organ: B3 Bass key-down weights for every key; Bass stage blockers'
status: in-progress
type: task
created_at: 2026-11-14T13:04:37Z
updated_at: 2026-11-14T15:00:00Z
---

## Context
B3 Bass (model 5) table and wiring (ns4-b511 follow-up, organ-farfisa-bass s9).

## Higher Goal
1:1 audio parity of the organ models.

## Acceptance Criteria
- [x] Key-down weights of model 5 decoded for every key from the ARM (organ_bass_keys.py, b3_bass_keys.rs, bass-keydown.csv)
- [x] Oracle chain tool for the Bass processor (organ_bass_proc.py); window 1 inaudible, window 2 carries the bass
- [x] State decoded (s11): windows additive per key, `G = phi(S)` accumulator, output `1 - 0.84 G`, line-gain table at G = 0; the per-key lattice words are not the cause
- [x] Processor part with oracle chord goldens (1, 3, 10 keys, 3 orders): max 1.45 dB, rms <= 0.65 dB (`b3_bass_state.rs`, not wired)
- [ ] Opt-in Bass stage on the full chain within 1 dB: needs the generator weight rule (`DM 0x95122..`)
- [ ] B3 clip comparison (emu_vs_hw.py) and default decision

## Out of Scope
Bit-exact 0x122296 model.

## Summary of Changes
Weights table, then the state model (`b3_bass_state.rs`, `tools/organ_bass_state.py`, data CSVs) and the correction that the section words are not read; no render change (render_hash unchanged), `emu_vs_hw.py` not run (stage not wired, delta 0.00 dB).

---
# ns4-ubmo
title: 'FX: reverb dry/wet law per manual (fully wet at max) and wet level calibration'
status: todo
type: task
created_at: 2026-11-12T03:38:51Z
updated_at: 2026-11-12T03:38:51Z
parent: ns4-trkb
---

## Context
Manual p.52: the reverb Dry/Wet control provides a fully wet signal at its maximum position. process_reverb keeps dry at 1 - 0.22*mix (78 % dry at max) and the wet gain is uncalibrated. Hardware Jazz Guitar II clip is much wider than any emulator reverb setting tried (docs/research/hardware-audio-reference-fx.md).

## Acceptance Criteria
- [ ] True dry/wet crossfade, fully wet at amount 10
- [ ] Wet level calibrated against a reverb-dominated hardware clip
- [ ] Constants tagged per provenance rule

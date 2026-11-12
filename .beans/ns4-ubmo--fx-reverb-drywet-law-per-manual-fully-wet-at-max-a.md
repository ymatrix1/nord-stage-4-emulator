---
# ns4-ubmo
title: 'FX: reverb dry/wet law per manual (fully wet at max) and wet level calibration'
status: todo
type: task
priority: normal
created_at: 2026-11-12T03:38:51Z
updated_at: 2026-11-12T03:44:10Z
parent: ns4-trkb
---

## Context
Manual p.52: the reverb Dry/Wet control provides a fully wet signal at its maximum position. process_reverb keeps dry at 1 - 0.22*mix (78 % dry at max) and the wet gain is uncalibrated. Hardware Jazz Guitar II clip is much wider than any emulator reverb setting tried (docs/research/hardware-audio-reference-fx.md).

## Acceptance Criteria
- [ ] True dry/wet crossfade, fully wet at amount 10
- [ ] Wet level calibrated against a reverb-dominated hardware clip
- [ ] Constants tagged per provenance rule


## Findings (2026-11-12)
- Manual p.52 only fixes the maximum end (fully wet); the mid law (linear vs equal power) is not documented. Changing it blindly risks the Jazz Guitar II level match (currently ~1 dB RMS).
- Calibration method: measure the reverb tail after a clean note release in a hardware clip (dB/s decay and level relative to the dry level at release), per reverb type, and compare with the emulator render of the same program. A full-video mono 16 kHz envelope scan found 225 release candidates, but most are rhythmic playing; programs with a single sound source and a long sustained release (pads) are needed. The one clean release checked (35:48, A:75 Filter Bank Ld) has reverb off, so it measures an amp-envelope release, not a reverb.
- Needed: map each candidate release to its program (docs/research/data/video_part_a_programs.json) and to the layers that actually sound (scene masks and section enables, not only layer enable flags).

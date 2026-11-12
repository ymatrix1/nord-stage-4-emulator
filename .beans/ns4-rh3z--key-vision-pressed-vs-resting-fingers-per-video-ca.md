---
# ns4-rh3z
title: 'Key vision: pressed vs resting fingers, per-video calibration, MIDI-validated note detection'
status: todo
type: task
created_at: 2026-11-12T01:57:09Z
updated_at: 2026-11-12T01:57:09Z
parent: ns4-trkb
---

## Context
tools/key_vision.py finds fingertips over keys (candidate set, +-1 key, geometry measured by eye on one frame). tools/audio_notes.py is validated only on synthetic tones.

## Acceptance Criteria
- [ ] Auto-calibrate keyboard edges per video
- [ ] Distinguish pressed from resting fingers (key shadow/brightness change)
- [ ] Validate note detection on a recording with known MIDI (render emulator, transcribe, compare)

## Out of Scope
Velocity estimation.

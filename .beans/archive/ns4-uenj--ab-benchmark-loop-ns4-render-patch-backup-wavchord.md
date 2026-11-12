---
# ns4-uenj
title: 'A/B benchmark loop: ns4 render-patch --backup/--wav/chords/pedal'
status: completed
type: task
created_at: 2026-11-12T01:02:01Z
updated_at: 2026-11-12T01:02:01Z
---

## Context
render-patch rendered a single note, ignored .npno/.nsmp4, wrote no WAV.

## Higher Goal
Objective dB tuning of spectral balance, width, loudness vs hardware clips via video_ref.py compare.

## Acceptance Criteria
- [x] --backup resolves program + samples from .ns4b
- [x] --wav writes 32-bit float stereo WAV
- [x] --notes chord, --velocity, --hold, --sustain, --cc pedal events
- [x] Unit tests, gates green

## Out of Scope
Auto-tuning DSP from compare output; automatic note-sequence extraction from hardware clips.

## Summary of Changes
New crates/nord-core/src/render.rs (timeline renderer, WAV encoder, parsers); ns4 render-patch flags; tests; docs.

---
# ns4-6056
title: Harden audio output formats and MIDI delivery
status: completed
type: task
priority: normal
created_at: 2026-11-11T12:47:35Z
updated_at: 2026-11-11T12:49:22Z
parent: ns4-707f
---

## Context
The CPAL host currently only accepts f32 output, leaves extra channels unwritten, and can drop MIDI events when its bounded queue is full.

## Higher Goal
Bring desktop audio host behavior closer to a dependable Stage 4 instrument, preserving MIDI note lifecycle and producing deterministic channel output.

## Acceptance Criteria
- [x] Support f32 and i16 CPAL output buffers.
- [x] Define mono downmix and zero-fill extra channels.
- [x] Preserve MIDI events under queue saturation.
- [x] Add regression unit tests and update the audio host spec.
- [x] Pass all repository quality gates.

## Out of Scope
- Physical audio device selection and latency tuning.
- Changing Stage 4 DSP sound models.

## Summary of Changes
Added `f32`/`i16` stream callbacks, deterministic mono/stereo/multichannel
mapping, bounded sample conversion, and blocking MIDI delivery for queue
saturation. Added four regression tests and documented the host contract.
Verified fmt, clippy, Rust workspace tests, Ruff, pytest, and the source test
ratchet on Windows.

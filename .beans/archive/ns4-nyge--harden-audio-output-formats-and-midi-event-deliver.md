---
# ns4-nyge
title: Harden audio output formats and MIDI event delivery
status: completed
type: bug
created_at: 2026-11-11T12:43:48Z
updated_at: 2026-11-11T12:56:00Z
parent: ns4-707f
---

## Context
The CPAL host supports only F32 output, leaves output channels beyond stereo unwritten, and silently drops events when its MIDI queue is full.

## Higher Goal
Initialize common audio devices reliably, produce defined output on every channel, and preserve note lifecycle events under queue pressure.

## Acceptance Criteria
- [x] Support F32 and I16 audio output
- [x] Zero every output channel beyond left/right
- [x] Prevent silent Note Off/All Notes Off drops when the event queue is full
- [x] Add unit tests for format conversion, extra-channel zeroing, and queue overflow policy
- [x] Update audio host specification
- [x] Run project quality gates

## Out of Scope
Additional CPAL sample formats and resampling

## Summary of Changes
- Added support for both F32 and I16 audio output streams via `render_frame_to_f32` and `render_frame_to_i16` in `crates/nord-gui/src/audio.rs`.
- Guaranteed that all channels beyond stereo (channel indices 2..N) are strictly zeroed out.
- Implemented queue overflow backpressure policy in `AudioHandle::send_midi` prioritizing note release and all-notes-off events (`send_timeout`) to eliminate stuck notes under burst queue pressure.
- Added comprehensive unit tests in `crates/nord-gui/src/audio.rs` verifying conversion, surplus channel zeroing, and queue overflow delivery.
- Updated audio host and event delivery contract in `docs/specs/midi-and-control.md`.

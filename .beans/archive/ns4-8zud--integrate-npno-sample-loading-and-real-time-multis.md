---
# ns4-8zud
title: Integrate .npno sample loading and real-time multisample playback into PianoLayer
status: completed
type: task
created_at: 2026-11-11T13:32:00Z
updated_at: 2026-11-11T13:35:00Z
parent: ns4-trkb
---

## Context
We reverse-engineered the .npno container and 4-bit DPCM audio codec in crates/nord-core/src/cbin/piano_sample.rs, but PianoLayer in piano.rs currently renders only a hybrid harmonic synthesis wave.

## Higher Goal
Enable PianoLayer to load real decoded .npno multisample strokes (root key mapping, micro-cent detuning, sample playback pitch transposition, multi-velocity layers) while strictly obeying zero-allocation real-time safety in render_frame().

## Acceptance Criteria
- [x] Add sample playback buffer representation for decoded NPNO strokes (zero allocations in render_frame)
- [x] Wire root key lookup and micro-cent detuning transposition into voice pitch calculation
- [x] Implement linear or hermite sample interpolation for non-integer sample playback rates
- [x] Fall back smoothly to synthetic tone if no .npno sample is loaded
- [x] Add unit tests verifying multisample stroke pitch transposition, boundary clamping, and real-time safety
- [x] Update piano engine specifications in docs/specs/piano-engine.md
- [x] Pass all workspace quality gates (cargo test, clippy, fmt, ruff, pytest, check_tests)

## Out of Scope
Disk streaming during real-time render (pre-load sample memory instead)

## Summary of Changes
- Implemented `LoadedPianoStroke` and `NpnoSound` in `crates/nord-core/src/piano.rs` to hold pre-decoded, contiguous PCM buffers with 128-key zone mapping and signed tuning cents.
- Integrated real-time multisample stroke playback with linear interpolation and pitch transposition into `PianoLayer::render_frame`, strictly obeying zero-heap-allocation and lock-free audio thread safety.
- Implemented an intelligent polyphonic voice-stealing priority algorithm that recycles quiet/releasing voices before active sounding notes.
- Added comprehensive unit tests in `crates/nord-core/src/piano.rs` covering untransposed playback, octave-up (+12 st), octave-down (-12 st) transposition, and voice-stealing priority.
- Updated specifications in `docs/specs/piano-engine.md` and tracked roadmap checkboxes in `docs/ROADMAP.md`.

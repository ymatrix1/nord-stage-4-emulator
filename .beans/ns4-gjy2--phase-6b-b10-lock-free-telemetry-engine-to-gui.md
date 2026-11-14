---
# ns4-gjy2
title: 'Phase 6b B10: lock-free telemetry engine to GUI'
status: completed
type: task
created_at: 2026-11-12T21:36:10Z
updated_at: 2026-11-12T21:36:10Z
parent: ns4-xc5n
blocked_by:
    - ns4-q6p7
---

## Context
Meters and LED ladders must show morphed levels without the editor locking the engine.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] Audio thread publishes a lock-free telemetry snapshot without allocating
- [x] Test proves process_block does not allocate with telemetry enabled
- [x] LED ladders show the morphed level

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
- `crates/nord-core/src/engine/telemetry.rs` + `telemetry_cell.rs`: fixed-size `Telemetry` (per-layer morphed level, active / aux flags, held keys, morph values, split LEDs and zone gains, MIDI in / out counters, Extern mask, drawbars, rotary, transpose, BPM, clock step, undo / redo, overflow counters) published through a seqlock over `AtomicU32` words (`std` only, no `unsafe`).
- `Stage4Engine::process_block` publishes; the CPAL callback calls `publish_telemetry()` per buffer.
- `crates/nord-core/tests/no_alloc_audio_path.rs`: counting global allocator, 0 allocator calls over 400 blocks; stress test with 1 writer and 3 readers shows no torn snapshot.
- `crates/nord-gui/src/telemetry.rs` (`TelemetryHandle` in `AudioHandle`, `PanelTelemetry`, `PanelLeds`, `aux_flag_from_telemetry`); surgical bay edits make ladders show the morphed level and the LEDs follow real engine state; `ProgramEdit::Redo` and UNDO / REDO gating.
- Spec: `docs/specs/telemetry.md`.

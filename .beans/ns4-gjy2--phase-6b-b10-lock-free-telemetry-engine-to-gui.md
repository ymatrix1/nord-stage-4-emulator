---
# ns4-gjy2
title: 'Phase 6b B10: lock-free telemetry engine to GUI'
status: todo
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
- [ ] Audio thread publishes a lock-free telemetry snapshot without allocating
- [ ] Test proves process_block does not allocate with telemetry enabled
- [ ] LED ladders show the morphed level

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

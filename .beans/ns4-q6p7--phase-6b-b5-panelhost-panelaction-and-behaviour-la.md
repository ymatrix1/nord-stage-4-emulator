---
# ns4-q6p7
title: 'Phase 6b B5: PanelHost, PanelAction and behaviour layer'
status: todo
type: task
created_at: 2026-11-12T21:36:07Z
updated_at: 2026-11-12T21:36:07Z
parent: ns4-xc5n
blocked_by:
    - ns4-r7vt
---

## Context
Panel logic must drive both the standalone AudioHandle and the plugin ParamSetter without locking the engine.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] PanelHost trait (get, set, begin/end gesture) and PanelAction enum
- [ ] Shift, hold-for-shift, double-tap latch, selector cycling, pot snap, Live Mode with unit tests
- [ ] Timings tagged ASSUMED until hardware-confirmed

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

---
# ns4-vip1
title: 'Phase 6b B12: standalone integration'
status: todo
type: task
created_at: 2026-11-12T21:36:11Z
updated_at: 2026-11-12T21:36:11Z
parent: ns4-xc5n
blocked_by:
    - ns4-q6p7
    - ns4-0sud
    - ns4-9h25
    - ns4-otuf
    - ns4-z6fi
    - ns4-irgv
    - ns4-hcuh
    - ns4-jpgx
---

## Context
nord-gui becomes a thin shell over nord-panel.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] PanelHost over AudioHandle
- [ ] Panorama view, focused rack view, library drawer

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

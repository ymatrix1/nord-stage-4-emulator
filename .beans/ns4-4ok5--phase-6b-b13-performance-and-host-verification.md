---
# ns4-4ok5
title: 'Phase 6b B13: performance and host verification'
status: todo
type: task
created_at: 2026-11-12T21:36:11Z
updated_at: 2026-11-12T21:36:11Z
parent: ns4-xc5n
blocked_by:
    - ns4-hud1
    - ns4-vip1
---

## Context
The panel must meet its frame budget and run in real hosts.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] 2x-scale frame under 4 ms on the reference machine, measured
- [ ] Repaint at most 60 Hz and only on change
- [ ] Host smoke test with evidence

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

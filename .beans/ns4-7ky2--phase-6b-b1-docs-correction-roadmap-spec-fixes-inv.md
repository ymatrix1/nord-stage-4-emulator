---
# ns4-7ky2
title: 'Phase 6b B1: docs correction (ROADMAP, spec fixes, inventory spec, D15)'
status: in-progress
type: task
created_at: 2026-11-12T21:36:06Z
updated_at: 2026-11-12T21:36:06Z
parent: ns4-xc5n
---

## Context
Phase 6 panel ticks overstate completion and the visual spec has known discrepancies with the manual.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] Reopen Phase 6 bay checkboxes and add Phase 6b to docs/ROADMAP.md
- [x] Add docs/specs/control-inventory.md and index it in docs/specs/README.md
- [x] Add Known discrepancies to docs/specs/visual-style-and-panel.md
- [x] Record D15 (proposed) and OQ8..OQ14
- [ ] Planning PR merged

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

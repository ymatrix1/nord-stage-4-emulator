---
# ns4-7cv6
title: 'Phase 6b B6: headless golden-image harness'
status: todo
type: task
created_at: 2026-11-12T21:36:07Z
updated_at: 2026-11-12T21:36:07Z
parent: ns4-xc5n
blocked_by:
    - ns4-r7vt
---

## Context
Rendering changes need a deterministic CPU check that runs in CI.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] Rasterise egui output on CPU with tiny-skia
- [ ] Per-pixel tolerance plus error budget, goldens at 0.5x
- [ ] Optional local-only comparison against a reference photo via tools/crop_panel.py, ignored by default and skipped when .reference/ is absent

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

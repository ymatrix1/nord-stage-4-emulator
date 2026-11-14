---
# ns4-hcuh
title: 'Phase 6b B8f: section renderer - layer effects'
status: in-progress
type: task
created_at: 2026-11-12T21:36:09Z
updated_at: 2026-11-12T21:36:09Z
parent: ns4-xc5n
blocked_by:
    - ns4-3n0d
    - ns4-q6p7
    - ns4-7cv6
    - ns4-77s9
---

## Context
Section renderer for layer effects in crates/nord-panel/src/render and layout.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] Every inventory control in the section has a layout rect, renderer and binding
- [ ] Golden image for the section
- [ ] No overlaps; controls inside section bounds

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
Standalone renderer in `crates/nord-gui/src/panel/bay_effects/` (`ctx`, `focus`, `modulation`, `delay`, `amp`, `reverb`, `ids`): all 50 `Effects` ids drawn at `layout_px` rectangles (headless test `every_inventory_id_has_a_painted_widget`), Shift and hold-to-shift through `nord_panel::behaviour`, FX focus / GROUP / GLOBAL, tap tempo, MST CLK. Engine edits: `FxEdit` (`audio/edits/fx.rs`). GUI chain mirror: `panel/ui_state/fx/`. Still open: golden image, plugin editor (B11).

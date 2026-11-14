---
# ns4-irgv
title: 'Phase 6b B8e: section renderer - synth'
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
Section renderer for synth in crates/nord-panel/src/render and layout.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] Every inventory control in the section has a layout rect, renderer and binding (standalone GUI; 73/73 ids painted, 9 dimmed with a tooltip naming the missing feature)
- [ ] Golden image for the section
- [ ] No overlaps; controls inside section bounds

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

Standalone GUI bay delivered on `feat/gui-bay-synth`: `panel/bay_synth/` (strip, OLED + page contents, controls, widgets), `panel/ui_state/synth/` (per-layer values, page machine, buttons with SHIFT/hold via `nord_panel::behaviour`, dials, pots with pick-up, GROUP latches, Sound Init), `audio/edits/synth/` (`SynthEdit`, `SynthParam`, `SynthGroup`). Golden image and the plugin editor renderer remain open.

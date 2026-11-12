---
# ns4-77s9
title: 'Phase 6b B7: asset baker (xtask bake-panel) and material/shadow system'
status: todo
type: task
created_at: 2026-11-12T21:36:08Z
updated_at: 2026-11-12T21:36:08Z
parent: ns4-xc5n
blocked_by:
    - ns4-7ky2
    - ns4-r7vt
---

## Context
D15 (proposed) chooses baked sprites for static layers plus egui painter for dynamic layers.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] xtask bake-panel produces 1x and 2x atlases from original art (no photo crops) with tiny-skia
- [ ] Atlas content hashes reproducible across runs and platforms
- [ ] Theme tokens, one top-left light, per-element height token drives shadow offset/blur

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

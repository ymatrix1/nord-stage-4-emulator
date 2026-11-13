---
# ns4-3n0d
title: 'Phase 6b B2: control inventory data from manual ch. 3-9'
status: in-progress
type: task
created_at: 2026-11-12T21:36:07Z
updated_at: 2026-11-12T21:36:07Z
parent: ns4-xc5n
blocked_by:
    - ns4-7ky2
---

## Context
The gap list in control-inventory.md is from a code review, not the manual; counts are unverified.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] Every control in manual chapters 3-9 has a ControlId, kind, manual page and provenance tag
- [ ] Per-section and per-kind counts match the manual and the reference panel photo (manual done; photo checked for headline counts only, per-knob check needs a higher-resolution photo or B8 layout)
- [x] Counts in control-inventory.md marked verified with the check used (pinned tests in `crates/nord-panel/src/inventory/tests.rs`; manual-only, see photo note above)

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
- New crate `crates/nord-panel` (types + `inventory/`, 12 files, all under 400 lines) with 273 `ControlEntry` records: 66 `Bound`, 49 `UiOnly`, 158 `Unsupported` (each tied to an existing bean).
- Tests check unique ids, pages 11-53, `Bound` ids against `nord-plugin` sources, bean existence, and pinned counts.
- `docs/specs/control-inventory.md` §2/§4/§5 and the gap list rewritten; `visual-style-and-panel.md` §4.6 gains the green split LEDs and unknown fader LED count.
- Not done: per-knob photo comparison, layout rects (`rect_mm`), renderers.

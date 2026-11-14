---
# ns4-ow85
title: 'Program display fidelity: E marker, Live N, section rows with preset ids and size letters'
status: completed
type: task
created_at: 2026-11-14T03:34:07Z
updated_at: 2026-11-14T03:34:07Z
---

## Context
Hardware (DSoundman videos) shows a boxed E after the location when a program is edited (cleared by STORE or loading), Live N instead of the location in Live Mode, and per-section rows with preset id (1:05) or source name plus size letter (XL, M) and mini drawbars for the organ. ProgramUi has no dirty flag and bays do not report edits. See docs/research/dsoundman-course-findings.md.

## Higher Goal
Match hardware display behaviour.

## Acceptance Criteria
- [x] Boxed `E` after the location once a sound edit was sent; cleared by STORE, program load and external program change
- [x] `Live N` is the big title in Live Mode
- [x] Layers view shows one row per active section
- [x] Unit tests for edit tracking, titles, rows and the painter
- [x] Spec (`docs/specs/visual-style-and-panel.md`), research table and provenance tags updated

## Out of Scope
Engine changes unrelated to the display. Preset ids (`1:05`), piano size letters (`XL`, `M`) and organ mini drawbars: the emulator has no source for the ids and sizes yet; tracked in ns4-bjvn.

## Summary of Changes
- `AudioCommand::is_program_edit` classifies sound edits; `AudioHandle` counts them (`edit_count`).
- `bay_program/edited.rs`: `ProgramUi` compares the count with its value at the last load / store (`mark_clean`, `sync_edits`, `is_edited`). Loads in `library/poll.rs`, `select_loc`, external program changes and `accept_snapshot` (STORE) rebase it. The engine has no dirty flag (Undo tracks only the last undoable operation), so none was added there.
- `bay_program/oled_home.rs`: `location_title` (`A:11` + marker, or `Live N`), `name_line`, `section_rows`; `oled_paint.rs` draws the boxed `E`.
- Provenance: boxed E and `Live N` VERIFIED (course footage); no `E` in Live Mode, same-value setters counting, and section rows without ids ASSUMED.

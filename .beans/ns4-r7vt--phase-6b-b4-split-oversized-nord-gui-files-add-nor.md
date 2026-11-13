---
# ns4-r7vt
title: 'Phase 6b B4: split oversized nord-gui files, add nord-panel skeleton'
status: in-progress
type: task
created_at: 2026-11-12T21:36:07Z
updated_at: 2026-11-12T21:36:07Z
parent: ns4-xc5n
---

## Context
panel.rs 1920, library.rs 1051, audio.rs 775, widgets.rs 644 lines exceed the 400-line rule.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] nord-gui audio/, library/ and widgets/ split into modules, every file under 400 lines (panel.rs split tracked in PR #176)
- [ ] crates/nord-panel created (egui only, no cpal/midir/nih_plug) with inventory/, layout/, model/, behavior/, render/ modules and tests
- [ ] Behaviour unchanged: all tests pass and a before/after render matches

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
- Split `library.rs` into `library/{mod,loaders,poll,program,ui,support}.rs`, `audio.rs` into `audio/{mod,command,handle,frame,apply,io}.rs`, `widgets.rs` into `widgets/{mod,basic,faders,knob,display,wheels,support}.rs`; code moved verbatim, only `schedule_*_entry` became `pub(super)` and imports were trimmed.
- 12 small new tests so every new file carries its own `#[test]`.
- Remaining: `panel.rs` split (PR #176), then tick B4.

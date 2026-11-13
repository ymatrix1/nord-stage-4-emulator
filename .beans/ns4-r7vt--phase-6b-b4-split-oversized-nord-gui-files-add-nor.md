---
# ns4-r7vt
title: 'Phase 6b B4: split oversized nord-gui files, add nord-panel skeleton'
status: completed
type: task
priority: normal
created_at: 2026-11-12T21:36:07Z
updated_at: 2026-11-13T10:26:44Z
parent: ns4-xc5n
---


## Context
panel.rs 1920, library.rs 1051, audio.rs 775, widgets.rs 644 lines exceed the 400-line rule.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] nord-gui audio/, library/, panel/ and widgets/ split into modules, every file under 400 lines (largest: `library/archive.rs` 311)
- [x] crates/nord-panel created (egui only, no cpal/midir/nih_plug) with inventory/, layout/, model/, behavior/, render/ modules and tests
- [x] Behaviour unchanged: all tests pass and a before/after render matches (headless shape-list hash, see below)

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

### Split
- `audio/`: `command` (AudioCommand, AudioHandle), `apply`, `frame`, `io` (CPAL, MIDIR, raw MIDI bytes)
- `library/`: `mod` (state), `archive`, `loaders`, `poll`, `ui`, test `support`
- `panel/`: `mod` (render_stage4_panel), `state`, `program`, `bay_master|organ|piano|program|synth|fx`, `keybed`, test `support`
- `widgets/`: `chrome`, `buttons`, `sliders`, `selectors` (flat re-export), test `support`
- Public API unchanged (re-exported from each `mod.rs`); `Stage4PanelState` still `nord_gui::panel::Stage4PanelState`.

### nord-panel
`crates/nord-panel` (egui 0.31 only): `inventory` (types, `validate`), `layout` (`PanelRect`), `model` (`Normalized`), `behavior` (`hit_test`, `drag_to_value`), `render` (egui conversions, the only egui user). 11 unit tests.

### Single egui (D15)
Temporary dev-dependency `nih_plug_egui` (nih-plug git): `cargo check -p nord-panel --tests` passed; `cargo tree -i egui` = one `egui v0.31.1` shared by `eframe` (nord-gui), `egui-baseview` (nih_plug_egui) and `nord-panel`; no duplicate egui. Dev-dependency removed afterwards. Recorded in D15.

### Before/after evidence
Throwaway integration test (not committed; hash of `format!("{:?}", ctx.run(..).shapes)` for `render_stage4_panel`, default state): origin/main `1247 shapes, hash 5a409628042a69ff`; after split identical. Edited state (drawbars, cutoff, master volume): `1259 shapes, da90e1f478455456` before and after. Not a pixel render (no GPU/raster in test), but the full painted shape list is byte-identical. Interactive behaviour (mouse/keybed) is covered by the existing `test_keybed_mouse_interaction_no_stuck_notes`, moved unchanged.

## Summary of Changes
Split four oversized nord-gui files into directory modules with per-module tests and shared `support` fixtures; added the `nord-panel` skeleton; verified single egui 0.31.1 with `nih_plug_egui`; ROADMAP B4 ticked, D15 and specs updated. Not verified: plugin-editor runtime (no editor yet), pixel-level render.

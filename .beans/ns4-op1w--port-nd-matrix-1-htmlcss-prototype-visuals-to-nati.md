---
# ns4-op1w
title: Port nd matrix 1 HTML/CSS prototype visuals to native nord-panel / nord-gui (baker + egui widgets)
status: completed
type: task
created_at: 2026-11-19T18:52:52Z
updated_at: 2026-11-20T21:08:00Z
---

## Context

PR #533 (`ns4-gnvr`) established the approved 1:1 HTML/CSS hardware front-panel visual reference (`nd matrix 1`) in `prototypes/panel-mock/`. The native standalone app (`crates/nord-gui`) and DAW plugin (`crates/nord-plugin`, CLAP/VST3 via `nih_plug_egui`) needed to match the HTML/CSS prototype 1:1 while staying fast, lightweight, and single-source-of-truth inside DAWs (D15).

## Higher Goal

Deliver an automated, deterministic HTML/CSS-to-`egui` baking pipeline plus updated `nord-gui` / `nord-panel` widget painters and a 1:1 baked panel view so visual edits made in `prototypes/panel-mock/` port cleanly into both the standalone app and CLAP/VST3 plugin without embedding a WebView.

## Acceptance Criteria

- [x] `tools/bake_panel_from_html.py` deterministically exports `panel_plate_2x.png`, `panel_sprites_2x.png`, and `panel_layout.json` from `prototypes/panel-mock/` with unit tests in `tools/tests/`.
- [x] `crates/nord-gui` renders the baked plate, sprites, LEDs, and OLEDs in both standalone (`eframe`) and DAW plugin (`nih_plug_egui`) targets.
- [x] Visual diff against `prototypes/panel-mock/` confirms 1:1 parity across all 6 bays and the 88-key keybed.
- [x] `legibility_tests.rs`, `frame_cost_tests.rs`, and all Rust/Python CI gates pass.

## Out of Scope

- Embedding a live WebView inside `nord-plugin`.
- Changing DSP synthesis behavior in `crates/nord-core`.

## Summary of Changes

- **Phase 1 (PR #535, `tools/bake_panel_from_html.py`):** Added headless-Chromium HTML/CSS-to-`egui` baker exporting `panel_plate_2x.png`, `panel_sprites_2x.png`, `panel_sprites.json`, and `panel_layout.json` plus unit tests in `tools/tests/test_bake_panel_from_html.py`.
- **Phase 2 (PR #536, `crates/nord-gui/src/baked.rs`, `style.rs`):** Added `BakedPanelAssets` with `<= 2048px` horizontal plate tiling, sprite/plate painters, and `PROTOTYPE_*` tokens.
- **Phase 3 (PR #537, `crates/nord-gui/src/widgets/`):** Added `paint_sprite_rotated`, 14 px unclipped sprite shadow padding in `prototypes/panel-mock/`, and wired baked sprites into `basic.rs`, `knob.rs`, `faders.rs`, and `wheels.rs`.
- **Phase 4 (`feat/baked-panel-plate-1to1`, `crates/nord-gui/src/panel/baked_view.rs`):** Implemented 1:1 `render_baked_panel` wired to `AudioHandle` / `Stage4PanelState`, enabled `use_baked_plate = true` in `nord-stage4` (`main.rs`), updated `docs/specs/visual-style-and-panel.md` and `docs/DECISIONS.md` (D15 amendment).

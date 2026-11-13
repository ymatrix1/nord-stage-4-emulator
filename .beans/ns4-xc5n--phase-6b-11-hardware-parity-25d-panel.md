---
# ns4-xc5n
title: 'Phase 6b: 1:1 Hardware Parity & 2.5D Panel'
status: todo
type: epic
created_at: 2026-11-12T21:36:06Z
updated_at: 2026-11-12T21:36:06Z
---

## Context
The Phase 6 ROADMAP ticked the six panel bays as done, but `crates/nord-gui/src/panel.rs` (1920 lines) draws only a functional subset: SOLO, AUX KB, SUSTPED/PSTICK enables, per-layer KB ZONE, most Organ/Piano/Synth/FX sub-controls and most of the Program bay have no state, parameter or widget. `nord-plugin` has no editor. `panel.rs`, `library.rs` (1051), `audio.rs` (775) and `widgets.rs` (644) exceed the 400-line rule.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] B1 docs correction
- [ ] B2 control inventory data
- [ ] B3 parameter-ID stability test
- [ ] B4 nord-gui split + nord-panel skeleton
- [ ] B5 host and behaviour layer
- [ ] B6 headless golden-image harness
- [ ] B7 asset baker and material/shadow system
- [ ] B8a-g section renderers
- [ ] B9a-d parameter and engine parity
- [ ] B10 telemetry engine to GUI
- [ ] B11 plugin editor
- [ ] B12 standalone integration
- [ ] B13 performance and host verification

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)
- New DSP models beyond what parity of existing controls needs

## Summary of Changes

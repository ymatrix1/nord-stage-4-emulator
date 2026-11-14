---
# ns4-fqym
title: 'Section Edit: apply bay edits to all Layers of the Section'
status: completed
type: task
created_at: 2026-11-14T14:19:35Z
updated_at: 2026-11-14T18:00:00Z
parent: ns4-xc5n
---

## Context
Manual p.43: "While SECTION EDIT is pressed, edits done to a parameter are performed on all Layers of that Section"; double tap makes it sticky; Shift/Exit leaves. `nord-core` already has the engine helpers `section_edit_organ/piano/synth/fx` (`engine/section_edit.rs`, unit-tested). The panel tracks the held / sticky state (`ProgramUi::section_edit_active()`, LED lit), but no bay read it: Organ, Piano, Synth and Effects edits still went to the focused Layer only (the Synth GROUP latches are a different function).

## Higher Goal
`program.section_edit` is a working control: with it active, any knob, fader, drawbar or button edit in a bay is applied to every Layer of that Section, as on the hardware.

## Acceptance Criteria
- [x] Each bay reads `ProgramUi::section_edit_active()` when it emits an edit and fans the edit out to the Layers of its Section (Organ A/B, Piano A/B, Synth A/B/C, Layer Effects chains of the Section)
- [x] One GUI test per bay turns a control with Section Edit held and shows the other Layers changed in a real engine (`panel/section_edit_tests.rs`, with a latch-off negative per bay)
- [x] `program.section_edit` becomes `Bound("ui:program.section_edit")` and its proof joins `bay_program/binding_tests.rs`
- [x] Spec `docs/specs/control-inventory.md` section 10 row replaced (new section 10a), counts re-pinned (`Bound` 208, `Unsupported` 0)
- [x] Plugin editor: the editor draws the same panel, so the fan-out reaches each layer's host parameter (`nord-plugin/src/editor/section_edit_tests.rs`)

## Out of Scope
- GROUP (Synth Filter / LFO / Arp), which exists
- Plugin-host automation of the Section Edit button itself (it is a panel mode, not a host parameter)
- Layer on/off buttons, Aux KB LEDs, SOUND / LAYER INIT, EXTERN and sample / piano-model loads (stay per layer, see control inventory section 10a)

## Summary of Changes
- `panel/section_edit.rs`: `Stage4PanelState::section_edit()` and `edit_layers(focus, count)`, one rule for "focused layer or whole section".
- Organ (`organ_strip.rs`, `bay_organ.rs`) and Piano (`piano_logic.rs`): per-layer edits loop over `edit_layers`, mirroring the panel state and sending one edit per layer.
- Synth: `SynthUi::section_edit` (set per frame by the bay) folds into `SynthUi::grouped`, so every `SynthUi::edit` writes `SynthTarget::All`; `ui_state/synth/section.rs` fans the waveform selection and KB ZONE; the strip fader fans the level.
- Effects: `FxUi::section_edit` and `FxUi::section_edits` (called by `after_edit`) copy the edited unit to the other chains of the section and send the edits; `ChainUi::copy_unit_from`.
- Inventory `program.section_edit` is `Bound("ui:program.section_edit")`; pins and `control-inventory.md` counts updated; `Support::Partial` tooltip removed; ROADMAP and `morph-and-zones.md` updated.
- `Sim` got a `screen` size field so the full-panel tests reach the Effects bay.

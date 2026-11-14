---
# ns4-fqym
title: 'Section Edit: apply bay edits to all Layers of the Section'
status: todo
type: task
created_at: 2026-11-14T14:19:35Z
updated_at: 2026-11-14T14:19:35Z
parent: ns4-xc5n
---

## Context
Manual p.43: "While SECTION EDIT is pressed, edits done to a parameter are performed on all Layers of that Section"; double tap makes it sticky; Shift/Exit leaves. `nord-core` already has the engine helpers `section_edit_organ/piano/synth/fx` (`engine/section_edit.rs`, unit-tested). The panel tracks the held / sticky state (`ProgramUi::section_edit_active()`, LED lit), but no bay reads it: Organ, Piano, Synth and Effects edits still go to the focused Layer only (the Synth GROUP latches are a different function).

## Higher Goal
`program.section_edit` is a working control: with it active, any knob, fader, drawbar or button edit in a bay is applied to every Layer of that Section, as on the hardware.

## Acceptance Criteria
- [ ] Each bay reads `ProgramUi::section_edit_active()` when it emits an edit and fans the edit out to the Layers of its Section (Organ A/B, Piano A/B, Synth A/B/C, Layer Effects chains of the Section)
- [ ] One GUI test per bay turns a control with Section Edit held and shows the other Layers changed in a real engine
- [ ] `program.section_edit` becomes `Bound("ui:program.section_edit")` and its proof joins `bay_program/binding_tests.rs`
- [ ] Spec `docs/specs/control-inventory.md` section 10 row removed, counts re-pinned

## Out of Scope
- GROUP (Synth Filter / LFO / Arp), which exists
- Plugin-host automation of Section Edit

## Summary of Changes
(open)

Blocker: the edit paths live in `crates/nord-gui/src/panel/bay_*` and `panel/ctl.rs`, which were being reworked by the legibility work when this was classified; do it after that lands.

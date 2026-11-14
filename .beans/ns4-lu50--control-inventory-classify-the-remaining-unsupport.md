---
# ns4-lu50
title: 'Control inventory: classify the remaining Unsupported controls'
status: completed
type: task
created_at: 2026-11-14T14:19:35Z
updated_at: 2026-11-14T14:19:35Z
parent: ns4-xc5n
---

## Context
After the Organ, Piano, Synth-strip, Synth-dial and Effects rewires, 64 inventory controls were still `Unsupported`, although most of them already had a real behaviour in `nord-gui` (Program browser, Store, Undo, Copy/Paste, Morph Assign, Preset Library, the Synth page and shift buttons, the Layer Effects ON buttons). The tag only recorded that no host parameter backs them.

## Higher Goal
The inventory tells the truth: `Bound` means a host parameter, an engine path, a dial table cell or a named GUI path that a test proves; `UiOnly` means the control changes no sound parameter, with the reason; `Unsupported` is only a control whose behaviour is missing, with the bean that holds the work.

## Acceptance Criteria
- [x] Every one of the 64 controls is classified control by control (`docs/specs/control-inventory.md` section 10)
- [x] New binding form `Bound("ui:<area>.<name>")` for behaviour that lives in the panel; `inventory/tests.rs` checks the form, `nord-plugin` treats it as `Passive`
- [x] GUI proof tests apply each `ui:` control through the painted button path to a real engine: `bay_program/binding_tests.rs` (29 ids), `ui_state/synth/binding_tests.rs` (18 ids), `bay_effects/tests/binding.rs` (5 ids); each test fails when a `ui:` id has no proof
- [x] PEDAL TAP (Shift+Mst Clk) implemented: `ProgramEdit::PedalTap` sets the engine's panel-assigned pedal function to Master Clock tap (manual p.40)
- [x] Counts re-pinned from the test output: Bound 207, UiOnly 67, Unsupported 1
- [x] The remaining `Unsupported` control (`program.section_edit`) has its own bean with the exact reason

## Out of Scope
- Section Edit wiring into the Organ, Piano, Synth and Effects bays (own bean)
- Host parameters for the `ui:` controls (the plugin editor shows them as `Passive`)
- Hardware verification of the Pedal Tap panel-assigned semantics (`ASSUMED`)

## Summary of Changes
- 64 controls moved: 52 to `Bound("ui:...")`, 2 to host ids (`delay.tap_set` to `delay_ms`, `amp.filter` to `fxsel_cabinet_{n}`), 9 to `UiOnly` (PAGE/BANK +/-, PROG VIEW, PRESET NAME, the three FX FOCUS buttons), 1 stays `Unsupported`.
- New: `ProgramEdit::PedalTap`, `ProgramUi::pedal_tap`, three GUI proof test modules, `ui:` handling in `nord-plugin` `editor/coverage.rs`.

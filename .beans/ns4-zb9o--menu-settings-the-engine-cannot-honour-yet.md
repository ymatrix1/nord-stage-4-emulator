---
# ns4-zb9o
title: Menu settings the engine cannot honour yet
status: completed
type: task
created_at: 2026-11-14T00:02:26Z
updated_at: 2026-11-14T00:02:26Z
parent: ns4-xc5n
---

## Context
Menu items marked Unsupported in crates/nord-panel/src/menus/ that need new engine behaviour.

## Higher Goal
Full menu parity with the OS v1.6x manual.

## Acceptance Criteria
- [x] Output bus routing (Global / Per Program): engine `OutputRouting` and `process_block_multi` (engine/output_routing.rs); multi-output host audio ports remain open (the plugin carries outputs 1-2)
- [x] MIDI CC/NRPN/Device send-receive filters, Program Change modes, Transpose MIDI at In/Out, Local Control, Sect On (engine/midi_menu.rs)
- [x] Extern menu: Echo Global, Keyboard Velocity, Device MIDI filters (engine/extern_menu.rs)
- [x] Triple pedal assignments (Pump, Mst Clk Tap, Program Up/Down, Synth Vibrato) and Foot Switch SW B (engine/foot_switch.rs)
- [x] Organ keyboard trigger point High/Low (engine/organ_trigger.rs)
- [x] PSTICK bends Organ and Piano layers
- [x] Organize Swap/Move on the program bank and on the Preset Library (`bay_program/organize.rs`, `organize_presets.rs`, written by Export backup; ns4-bq4q, ns4-r8t9)

## Out of Scope
- Panel rendering of the menus (nord-gui)

## Summary of Changes
Every menu item is `Bound` or `UiOnly` now (133 items: 111 bound, 22 UiOnly, 0 Unsupported); see docs/specs/midi-and-control.md section 5.

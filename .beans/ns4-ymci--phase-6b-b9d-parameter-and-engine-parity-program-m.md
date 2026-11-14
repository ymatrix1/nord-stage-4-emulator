---
# ns4-ymci
title: 'Phase 6b B9d: parameter and engine parity - program, menus and Extern'
status: in-progress
type: task
created_at: 2026-11-12T21:36:10Z
updated_at: 2026-11-12T21:36:10Z
parent: ns4-xc5n
blocked_by:
    - ns4-xhbf
---

## Context
Hardware controls for program, menus and Extern lack engine state or host parameters.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] Missing engine state and host params added (additive IDs only, nested params_*.rs files)
- [ ] Real-time rules hold (no allocation/locks in process_block)
- [ ] Unit tests per new parameter; specs updated

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
Part 2 (branch `feat/aux-pedals-extern`): `MidiEvent::PitchBend`; engine Aux KB (`engine/aux_kb.rs`), pedals with SUSTPED/PSTICK, half pedaling, Swell, control/rotor/foot switch (`engine/pedals.rs`), Extern output queue (`engine/extern_out.rs`); host params `ped_*` and `auxkb_*` (`aux_pedal_params.rs`). Remaining: Extern host params and plugin MIDI output port, Organ/Piano pitch bend, Echo Global To Extern, Extern device filters, Triple Pedal left/middle assignments, program/menu state (Sound/Output menu values).

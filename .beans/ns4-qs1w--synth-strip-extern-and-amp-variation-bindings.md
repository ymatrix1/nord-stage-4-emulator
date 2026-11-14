---
# ns4-qs1w
title: Synth strip, Extern and Amp VARIATION bindings
status: completed
type: task
created_at: 2026-11-14T11:01:11Z
updated_at: 2026-11-14T11:01:11Z
parent: ns4-trkb
---

## Context
The Synth section of the control inventory still listed `synth.on`, `synth.kb_zone_down/up`, `synth.zone_leds`, `synth.aux_led.a/b/c` and `synth.extern` as `Unsupported`, and the Effects bay dimmed `amp.variation` as "no distinct voicing" although `fx/amp_eq.rs` already voices Twin II / JC II / Small II separately. Aux KB (`engine/aux_kb.rs`), zone assignment (`engine/zone_assign.rs`) and Extern output (`engine/extern_out.rs`) exist in the engine, but only Organ and Piano used them.

## Higher Goal
Every control the engine can honour is `Bound` and live in the panel; what stays `Unsupported` says exactly why.

## Acceptance Criteria
- [x] `amp.variation`: confirmed modelled (direction VERIFIED by manual p.51, Hz/dB ASSUMED), stale dim removed, spec corrected
- [x] `Stage4Engine::set_layer_extern` (allocation free) with deterministic tests
- [x] Synth KB ZONE, Aux KB LEDs and EXTERN send `SynthEdit::{Zone,AuxKb,Extern}`; Extern dials send CC VAL 1/2 and PROGRAM
- [x] Inventory: 8 entries `Bound` (Bound 149, Unsupported 67), pins and `control-inventory.md` updated
- [x] No host parameter added (`param-ids.snapshot` untouched)

## Out of Scope
- OLED rendering under `crates/nord-gui/src/panel/bay_program/`
- Synth octave buttons (no host octave parameter), OLED dials, page selector buttons: stay `Unsupported` under `ns4-4w9l` (page selectors and multi-parameter controls; their parameters are bound individually)
- Per-program storage of the Extern flag (CBIN field not located)
- Hardware sweep of the Amp II voicings (`ns4-4p0s`)

## Summary of Changes
- `nord-core`: `Stage4Engine::set_layer_extern` + 2 tests.
- `nord-gui`: `SynthEdit::{Zone,AuxKb,Extern,ExternCc,ExternProgram}`; Synth strip uses the shared `kb_zone` model and a clickable Aux KB LED; Shift+Mode toggles Extern; Extern page dials are CC VAL 1 / CC VAL 2 / PROGRAM; Amp VARIATION no longer dimmed.
- `nord-panel`: inventory bindings and pins.
- Docs: `synth-engine.md`, `control-inventory.md`, `effects-and-routing.md`.

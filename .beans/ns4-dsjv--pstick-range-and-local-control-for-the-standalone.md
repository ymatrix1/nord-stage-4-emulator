---
# ns4-dsjv
title: PSTICK range and Local Control for the standalone keybed
status: completed
type: task
created_at: 2026-11-14T11:14:36Z
updated_at: 2026-11-14T11:14:36Z
parent: ns4-xc5n
---

## Context
Two gaps in the standalone panel. (1) The engine and host already carried a per-layer PSTICK range (`pedals.pstick_range`, `ped_range_1..7`) but nothing in the panel set it, the engine did not clamp it, and Organ / Piano bent by the stored range although the manual fixes them at 2 semitones (p.18, p.23). (2) The standalone on-screen keybed, mod wheel and pitch stick sent `AudioCommand::Midi` (`handle_midi`), so Local Control Off (MIDI menu page 1) did not silence the internal sound. The plugin editor already used the gate (`handle_keyboard_midi`).

## Higher Goal
Hardware parity for the pitch stick and the MIDI menu (OS v1.6x p.28, p.56, p.61).

## Acceptance Criteria
- [x] Engine: `Stage4Engine::set_pstick_range` clamps 1..=12; Synth layers use it, Organ and Piano bend a fixed 2 semitones (`engine/pstick.rs`, constants tagged VERIFIED / ASSUMED)
- [x] Host: additive-only; `param-ids.snapshot` unchanged (`ped_range_N` already existed); editor maps `SynthEdit::PstickRange` to `ped_range_5..7`
- [x] GUI: Shift + wheel over Synth Layer B sets the range (`SynthEdit::PstickRange`), legend shows the value; the on-screen pitch stick sends pitch bend
- [x] Local Control: `AudioCommand::Keyboard` runs `handle_keyboard_midi` for the keybed, mod wheel and pitch stick; MIDI In keeps `handle_midi`
- [x] Tests fail before the fix (`local_control_off_silences_the_on_screen_keybed`, `keybed_notes_are_keyboard_events_not_midi_in`) and pass after
- [x] Inventory pins unchanged (no control added); specs updated

## Out of Scope
- An OLED list for the range (the manual's "displayed list"); the wheel gesture stands in for it
- Per-layer range on the Synth strip (the strip sets all three Synth layers, like PSTICK itself)
- Draining the MIDI-out queue in the standalone (no MIDI output port yet)

## Summary of Changes
- `nord-core`: `engine/pstick.rs` (range consts, `set_pstick_range`, `pstick_range_semitones`); `apply_bend` uses it
- `nord-gui`: `AudioCommand::Keyboard`, `AudioHandle::send_keyboard_midi`, shadow-engine observe, keybed / mod wheel / pitch stick routed through it; `SynthEdit::PstickRange`, `SynthUi::pstick_range_step`, Shift+wheel on Layer B
- `nord-plugin`: editor mapping and probe for `PstickRange`
- Docs: `midi-and-control.md`, `control-inventory.md`, `daw-plugin.md`, `HOST_COMPATIBILITY.md`

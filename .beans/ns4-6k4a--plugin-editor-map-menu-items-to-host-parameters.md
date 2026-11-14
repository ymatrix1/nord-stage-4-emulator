---
# ns4-6k4a
title: 'Plugin editor: map menu items to host parameters'
status: completed
type: task
created_at: 2026-11-14T11:54:42Z
updated_at: 2026-11-14T11:54:42Z
parent: ns4-hud1
---

## Context
The plugin editor mapped only the Aux KB menu items to host parameters. Changing any other menu item (System, Sound, Output, Pedal, MIDI, Extern) in the plugin GUI wrote nothing to the host, so it was neither automatable nor saved, and host automation did not move the OLED pages.

## Higher Goal
Every menu setting that has a host parameter is editable and automatable from the plugin GUI, and the GUI follows the host, with a test that stops a new `Bound` item from silently missing its mapping.

## Acceptance Criteria
- [x] Every `Bound` menu item with a host parameter writes it as a begin / set / end gesture (81 items, `editor/menu_table.rs`)
- [x] Host values (automation, presets) update the menu pages (`editor/readback_menu.rs`)
- [x] Unit test iterates the menus model and fails on a `Bound` item with no mapping and no allow-list reason; a stale allow-list pattern also fails
- [x] Converted label orders (Aux synth, triple pedal Left / Middle, Foot Switch SW A) match the standalone engine edit
- [x] No new host parameter ids (`param_snapshot` unchanged)
- [x] `docs/plugin-editor.md` and `docs/specs/daw-plugin.md` updated

## Out of Scope
- Host parameters for items that have none (Local Control, MIDI channels, Extern bank / program / CC / send-on-load): allow-listed with reasons
- Changes to `nord-gui` `panel/bay_program/` and `panel/ctl.rs` (other agents)
- Verifying in a DAW (none available here)

## Summary of Changes
- `editor/menu_table.rs`: one table of 81 menu item to host parameter rows with explicit label conversions, plus the `UI_ONLY` allow-list with reasons
- `editor/map_settings.rs`: write side now driven by the table; tests cover the model iteration, stale allow-list patterns, default agreement, host range fit, and engine equivalence of the converted rows
- `editor/readback_menu.rs`: host values to `MenuUi` each frame without snapping unsupported labels
- `editor/probe.rs`: menu probe commands derive from the table
- Finding: the model default for Sustain Pedal Type is SP-1 while the engine and host default to SP-2; the read-back shows SP-2


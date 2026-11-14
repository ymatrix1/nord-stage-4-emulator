---
# ns4-9nj8
title: 'GUI OLED: title bar, menu body, soft buttons and popups from the real screens'
status: in-progress
type: task
created_at: 2026-11-14T14:50:23Z
updated_at: 2026-11-14T14:50:23Z
parent: ns4-xc5n
---

## Context

docs/research/arm-oled-render.md s9 lists the differences between the real OLED screens (frames of the ARM emulator) and crates/nord-gui: inverted title bar (rows 0 to 10) with a right-aligned page counter, menu body (8 px label at y = 15, 13 px value at y = 26), outlined soft buttons in rows 53 to 63, Program Organize list with the inverted row, parameter popups in the lower half of the home screen (label y = 45, value y = 55) instead of full-screen pages.

## Higher Goal

The Program / Synth OLED matches the real screens' structure.

## Acceptance Criteria

- [x] Title bar painted inverted with the page counter right-aligned (menus, Preset, Layer Init; `OledText::bar`). Section Edit and Morph assign keep their plain text screens; the Synth pages are on display 1 (own painter).
- [x] Menu body: 8 px label, 13 px value. Done for one-item pages (`OledText::body_big`, value alone in 13 px under the page title); several-item blocks keep rows; the real labels of research note s14 are applied (`menu_labels.rs`).
- [x] Outlined soft buttons with the active one inverted (`OledText::soft_on`, menus).
- [ ] Parameter popups in the lower half of the home screen: Transpose and Master Clock done (`OledText::popup`); Octave Shift (frame in research s17.4, slider graphic) and effect types open.
- [x] Unit tests assert the measured rows (`oled_paint::bar`, bar / popup / lit soft button paint tests, menu bar titles).

## Out of Scope

Proportional bitmap font (own bean); shipping any firmware pixel data.

## Summary of Changes

- `OledText` gained `bar`, `soft_on`, `popup`; `oled_paint::paint` draws the inverted title bar with the right-aligned counter, outlined soft buttons (lit when active) and the popup rows (`bar()` = rows 11 / 15 / 26 / 53 / 45 / 55 of the 64 px grid).
- Menus: counter in the bar, page title as the sub line, lit soft button = focused item; `menu_screen::bar_title` (`PROGRAM LEVELS` for Sound page 1, `... OVERVIEW` pages); Layer Init and the Preset screens get the bar; Transpose is a popup (`Transpose` / `+1 semi`).
- Synth section titles in mixed case where a frame proves it (`Filter`, `Filter Envelope`, `Amp Envelope`, `Osc Envelope`, `Osc Pitch`, `Vibrato`, `Arpeggiator`).
- Left: menu body layout, remaining popups, Organize inverted row.
- Fourth pass: one-item menu pages show the value alone in the 13 px font (`menu_screen::compose`, `oled_paint::paint`); tests `a_one_item_page_shows_its_title_and_a_lone_value`, `a_lone_menu_value_paints_like_any_body_row`. Not done (no frame of the exact layout was captured this pass): Master Clock / Octave Shift popup graphics, Store As, UNDO / arp LEFT-RIGHT / LFO titles, program list `Import into A:11`.
- Fifth pass: the Store screens (`STORE PROGRAM TO` / `STORE PROGRAM AS`) use the title bar and the measured rows (`oled_store::compose`, `oled_grid::Line`; `As:Unnamed` without a space, `Cat: <None>`, lit cursor cell); tests in `oled_store.rs` and `oled_paint_tests.rs`. Still open (frames now exist in research s16.4 / s16.5): the program list `Import into A:11`, the Synth `OSC WAVEFORM` columns, two-part menu values.
- Sixth pass: program list screen (`oled_lists.rs`: list icon bar, rows y = 13 + 10 i, lit selected row; LIST view and Program Organize), Synth `OSC WAVEFORM` (`1 (6)`, `SHAPE:` caption, inverted soft tabs), real menu labels and values (`Volume Level Offset`, `Control Pedal Type`, `+3 semi`, Fine Tune `440.8Hz` / `+3 cent` as a two-part tab value), Undo dialog, Master Clock popup. Left: display 1 blank while a list is open, beat boxes / octave slider graphics, `Undo to before <name>?` with the item name.

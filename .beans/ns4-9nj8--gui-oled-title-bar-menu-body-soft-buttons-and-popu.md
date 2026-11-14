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
- [ ] Menu body: 8 px label, 13 px value (one item per page with the real labels of research note s14; the rows still list the items with focus markers).
- [x] Outlined soft buttons with the active one inverted (`OledText::soft_on`, menus).
- [ ] Parameter popups in the lower half of the home screen: Transpose done (`OledText::popup`); Master Clock, Octave Shift and effect types open.
- [x] Unit tests assert the measured rows (`oled_paint::bar`, bar / popup / lit soft button paint tests, menu bar titles).

## Out of Scope

Proportional bitmap font (own bean); shipping any firmware pixel data.

## Summary of Changes

- `OledText` gained `bar`, `soft_on`, `popup`; `oled_paint::paint` draws the inverted title bar with the right-aligned counter, outlined soft buttons (lit when active) and the popup rows (`bar()` = rows 11 / 15 / 26 / 53 / 45 / 55 of the 64 px grid).
- Menus: counter in the bar, page title as the sub line, lit soft button = focused item; `menu_screen::bar_title` (`PROGRAM LEVELS` for Sound page 1, `... OVERVIEW` pages); Layer Init and the Preset screens get the bar; Transpose is a popup (`Transpose` / `+1 semi`).
- Synth section titles in mixed case where a frame proves it (`Filter`, `Filter Envelope`, `Amp Envelope`, `Osc Envelope`, `Osc Pitch`, `Vibrato`, `Arpeggiator`).
- Left: menu body layout, remaining popups, Organize inverted row.

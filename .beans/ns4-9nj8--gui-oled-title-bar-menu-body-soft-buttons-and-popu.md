---
# ns4-9nj8
title: 'GUI OLED: title bar, menu body, soft buttons and popups from the real screens'
status: todo
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

- [ ] Title bar painted inverted with the page counter right-aligned (menus, Preset, Section Edit, Morph assign, Synth pages).
- [ ] Menu body: 8 px label, 13 px value.
- [ ] Outlined soft buttons with the active one inverted.
- [ ] Parameter popups in the lower half of the home screen (Transpose, tempo, effect types).
- [ ] Unit tests assert the measured rows of section 9.

## Out of Scope

Proportional bitmap font (own bean); shipping any firmware pixel data.

## Summary of Changes

(pending)

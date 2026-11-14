---
# ns4-pyqg
title: Populate the ARM program store to get real program names on the OLED
status: todo
type: task
created_at: 2026-11-14T14:50:24Z
updated_at: 2026-11-14T14:50:24Z
parent: ns4-xc5n
---

## Context

The program memory is four 64 KB I2C EEPROMs (0x50..0x53); a blank store shows <empty> (research note s6). The record format and directory keys are not decoded.

## Higher Goal

Capture the Program home, Organize and Store screens with real names.

## Acceptance Criteria

- [ ] Identify the Store key, capture the I2C writes of a Store and decode the record header and name field.
- [ ] Replay them as an initial EEPROM image (local only) and show a real name on the home screen.
- [ ] Identify Program 1, 2, 5, 6, Page, Undo, Layer Init keys; capture System / Sound / Output / Pedal menus.

## Out of Scope

Committing EEPROM images or firmware data.

## Summary of Changes

(pending)

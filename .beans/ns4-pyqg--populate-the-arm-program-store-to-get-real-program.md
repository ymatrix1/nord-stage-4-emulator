---
# ns4-pyqg
title: Populate the ARM program store to get real program names on the OLED
status: in-progress
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

- [x] Identify the Store key (`(0x10, 0, 1)`), capture the I2C writes of a Store and decode the record header and name field (`tools/arm_store.py`; research note s13).
- [x] Replay them as an initial EEPROM image (local only) and show a real name on the home screen (`A:11` / `Studio Grand`, Organize list, section rows).
- [x] Identify Program 1, 2, 5, 6, Page and Layer Init keys; capture the System / Sound / Output / Pedal menus (all 56 pages of the seven menus).
- [ ] Identify Undo (Shift + Solo) and make the encoders effective (decoded to counters, no consumer reached); decode the directory entry for more than one program.

## Out of Scope

Committing EEPROM images or firmware data.

## Summary of Changes

- `tools/arm_store.py` (+ tests): cluster / record layout of the program volume, `store_current` (Store key twice with Memory Protect forced off at `0xC0150540`, then the storage task step `0xC0240518` called by hand), 256 KB image join / split, CLI `store`.
- `tools/arm_screens.py`: `Session.load_eeproms`, `Session.turn` (encoders), `boot --eeprom`, `sweep --shift`; `tools/arm_keys.py`: `ENCODER_PAIRS`, `encoder_steps`.
- Finding: Store is `(0x10, 0, 1)`, gated by Memory Protect; a Store writes a RAM cache that the storage task flushes; the record is 128-byte clusters, `size`, CRC-32 of the 824-byte body, a 24-byte name outside the CRC.
- `docs/research/arm-oled-render.md` s11 to s14, `docs/specs/arm-oled-render.md` v1.2.
- Left: Undo key, encoder consumer, several programs (directory entry), Store As name.

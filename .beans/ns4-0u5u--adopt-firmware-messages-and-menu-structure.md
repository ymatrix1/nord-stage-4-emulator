---
# ns4-0u5u
title: Adopt firmware messages and menu structure
status: todo
type: task
created_at: 2026-11-14T04:01:14Z
updated_at: 2026-11-14T04:01:14Z
parent: ns4-xc5n
---

## Context

`docs/research/oled-strings-vs-firmware.md` s3, s4, Follow-up 3: the firmware has user-visible messages and menu structure we do not model: `Piano not found` (`0x2CA180`), `Sample not found` (`0x2CA198`), `Please wait...` (`0x2C75D0`), `Pasted` / `Swapped` / `Nothing to Swap with` (`0x283758`, `0x283760`, `0x2837E5`), `Are you sure?` (`0x2C0E38`), `Press to initialize` (`0x2C004C`), `Single Layer` (`0x2C7584`), `Change param to morph` (`0x2C0E5C`), `Clear Morph %s` (`0x2CA154`), `%c Memory Protected` (`0x2837A0`, the `%c` is not decoded). The Output and MIDI menus use one page per channel with `Global` / `Per Program` values in the firmware (`Organ Output Channel`, `Rotary Output Channel`, `Piano Output Channel`, `Synth Output Channel`, `Output Routing Mode`, `Bus Output Channels`); ours use descriptive per-item labels (115 `affix` rows in `docs/research/data/oled-strings-vs-firmware.csv`).

## Higher Goal

The menus and OLED text read like the hardware.

## Acceptance Criteria

- [ ] Show the firmware messages in the matching flows (swap, paste, missing piano / sample, init confirm, wait) with tests.
- [ ] Decode what `%c` prints in `%c Memory Protected` and use the full message.
- [ ] Output / MIDI menu pages and labels follow the firmware list (keep ids stable), with tests; `python tools/ns4_oled_strings.py check` affix count for `menu` drops.

## Out of Scope

New engine behaviour behind the messages.

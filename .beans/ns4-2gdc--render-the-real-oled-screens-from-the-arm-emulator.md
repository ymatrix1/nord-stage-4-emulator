---
# ns4-2gdc
title: Render the real OLED screens from the ARM emulator
status: in-progress
type: task
priority: normal
created_at: 2026-11-14T04:01:13Z
updated_at: 2026-11-14T13:14:05Z
parent: ns4-xc5n
---


## Context

docs/research/oled-strings-vs-firmware.md s5: the ARM emulator boots to the main loop but renders no screen (display path stubbed, font undecoded, draw primitive unknown, panel link unidentified).

Result (docs/research/arm-oled-render.md): the harness answered `0` to the panel presence read of the board bring-up (`0xC024D648`, EMIFA CS2 `0x60000010` / `0x60000020`), so the display objects were never built. With the presence answer, a timer interrupt for the wait loop `0xC0075D0C`, and `TEXT_MEASURE` / `EVENT_INSERT` unstubbed, the firmware draws its own screens to two SSD1309-class 128 x 64 OLEDs over EMIFA CS2; `tools/arm_panel.py capture` rebuilds the controller RAM from the bus.

## Higher Goal

Compare our Program / Synth OLED geometry (rows, fonts, widths, title case) with the real screens instead of manual text and videos.

## Acceptance Criteria

- [x] The link from the host firmware to the panel boards is identified and documented with addresses (EMIFA CS2: presence `0x60000010/20`, latch `0x60000025`, ports `0x60000017/27`).
- [x] TEXT_MEASURE and EVENT_INSERT run unstubbed with real font objects (`arm_boot_full.prepare(display_stubs=False)`); font object layout decoded and validated against the firmware draw routine on 18 fonts (`arm_panel.py oracle`).
- [x] The frame buffer / flush found (`0xC0076924`, `[display + 0x18]`, 1024 bytes) and the boot splash and the Program screen (`A:11`) captured as PNG in `target/oled/` (local only).
- [x] The Program screen no longer ends in `Write error`: the store is four 64 KB I2C EEPROMs (`0x50..0x53`), modelled in `tools/arm_i2c.py`; the blank store is formatted and the screen shows `A:11` + `<empty>`.
- [x] The Program screen with a real program name (third pass, `tools/arm_store.py`: `A:11` / `Studio Grand`; bean `ns4-pyqg`).
- [x] Menu pages captured by injecting panel events (`tools/arm_keys.py`, `tools/arm_screens.py`): Shift + Program 4 / 7 / 8 (`AUX KB` / `MIDI` / `EXTERN SETTINGS`), Program Organize, the three Preset screens, Section Edit, Morph assign and the Synth `OSC WAVEFORM` page. System, Sound, Output and Pedal menus: third pass (research note s12), all 56 pages of the seven menus read.
- [x] Findings (fonts, row structure) recorded in the research note; compared with `bay_program/oled_home.rs`: the real big title is the 13 px font at (1, 3), next line at y = 18.
- [x] Case-only differences settled where a screen was reached: title bars upper-case through a widget flag (`ORGAN PRESET`, `AUX KB SETTINGS` right; `AUX KB`, `PROGRAM ORGANIZE`, `... MENU` wrong and fixed). Third pass: `Layer Init` right, `TRANSPOSE` / `FILTER` / `VIBRATO` / `OSC PITCH` wrong and fixed (research note s14). Open: `UNDO`, `LEFT` / `RIGHT`.
- [x] Real title / row layout (13 px at the top left, 15 px pitch, rule y = 33, rows from y = 36) compared with `oled_home.rs` / `oled_paint.rs`; every difference recorded in `docs/research/arm-oled-render.md` s9.
- [x] Fourth pass: the encoder consumer found (queue, UI task, screen routing, value widget; `tools/arm_events.py`) and the Program dial identified (pair 1): it pages menus, steps presets, opens the program list `Import into A:11` (research note s15).
- [ ] Menu values, Store As name entry and the editing screens of the Synth dials captured: blocked (parameter step `0xC00EEEB0` returns without a change: empty value container; Store As belongs to the `As:` screen object, not to the Store handler).
- [ ] Undo (Shift + Solo) found: the dialog class and refresh method `0xC01327BC` are found, the constructor and trigger are not (s15.4).

## Out of Scope

Shipping any firmware-derived pixel data (frames, font bitmaps, EEPROM images); GUI changes beyond the measured Program home grid and the settings menu titles (the other differences are follow-up beans: inverted title bar, menu body, popups, section row icons, proportional font).

## Summary of Changes

- `tools/arm_oled.py`, `tools/arm_panel.py` (+ tests): font parser and reference rasteriser, controller / panel-link model, harness glue and CLI (`fonts`, `oracle`, `capture`).
- `tools/arm_boot_full.py`: `display_stubs` switch.
- `docs/research/arm-oled-render.md`, `docs/specs/arm-oled-render.md`, update banner in `docs/research/oled-strings-vs-firmware.md`.
- Second pass: `tools/arm_i2c.py` (I2C0 + four EEPROMs; found as the program memory), `tools/arm_keys.py` (key matrix, 98 bits), `tools/arm_screens.py` (Session: boot once, save / restore, press keys, capture frames and widget text; CLI `boot` / `press` / `sweep`), `arm_panel.install_panel_link(emu, keys)` + `panel_block`, tests for each.
- `crates/nord-gui` (small, verified): `oled_paint::grid` (title y = 3, name line at the title's 13 px size, y = 18, rule y = 33, rows pitch 10); settings menu titles `AUX KB SETTINGS`, `MIDI SETTINGS`, `EXTERN SETTINGS`, ... and `Program Organize` (`MenuId::title`).
- `docs/research/arm-oled-render.md` s6 to s11, `docs/specs/arm-oled-render.md` v1.1, banner and follow-up 2 of `docs/research/oled-strings-vs-firmware.md`.
- Follow-up beans: `ns4-9nj8` (GUI title bar / menu body / popups), `ns4-aivy` (home rows), `ns4-pyqg` (populate the program store).
- Left: populated program store (record format), keys for Program 1 / 2 / 5 / 6, Page, Store, Undo, Layer Init, encoders; GUI follow-ups 2 to 5 of the research note.
- Third pass (PR feat/arm-oled-render-3): program store populated, keys / menus / Synth pages captured, real-vs-ours table, GUI title bar / popup / icons (beans `ns4-pyqg`, `ns4-9nj8`, `ns4-aivy`).
- Fourth pass (PR feat/arm-oled-render-4): `tools/arm_events.py` (+ tests), research note s15 (event chain, Program dial, program list screen, Undo dialog class, Store handler flags), GUI: one-item menu pages draw the value alone in 13 px (`OledText::body_big`). Left: menu value editing, Store As, Undo trigger, Master Clock / Octave Shift popup graphics, program list screen in the GUI.

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
- [ ] The Program screen with a program name (blocked: program storage ends in `Write error`).
- [ ] One menu page (e.g. System page 1) captured by injecting the panel event (blocked: no panel input model).
- [x] Findings (fonts, row structure) recorded in the research note; compared with `bay_program/oled_home.rs`: the real big title is the 13 px font at (1, 3), next line at y = 18.
- [ ] Case-only differences of the note (s3, 19 literals) settled (needs a menu / Synth screen).

## Out of Scope

Shipping any firmware-derived pixel data (frames, font bitmaps); changing the GUI layout before the comparison exists (`crates/nord-gui` untouched).

## Summary of Changes

- `tools/arm_oled.py`, `tools/arm_panel.py` (+ tests): font parser and reference rasteriser, controller / panel-link model, harness glue and CLI (`fonts`, `oracle`, `capture`).
- `tools/arm_boot_full.py`: `display_stubs` switch.
- `docs/research/arm-oled-render.md`, `docs/specs/arm-oled-render.md`, update banner in `docs/research/oled-strings-vs-firmware.md`.
- Left: program storage model, panel scan input, display 1 content (research note s6).

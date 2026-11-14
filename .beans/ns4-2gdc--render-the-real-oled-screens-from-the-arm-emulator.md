---
# ns4-2gdc
title: Render the real OLED screens from the ARM emulator
status: todo
type: task
created_at: 2026-11-14T04:01:13Z
updated_at: 2026-11-14T04:01:13Z
parent: ns4-xc5n
---

## Context

`docs/research/oled-strings-vs-firmware.md` s5: the ARM emulator boots to the main loop but renders no screen. The draw path is stubbed (`TEXT_MEASURE` `0xC001B9D0` and `EVENT_INSERT` `0xC001B67C` return 0, the font object `[widget + 0x2C]` is null), and in 100 M steady-state instructions no store reaches a display-capable peripheral (no EMIFA CS2 / UART1 / SPI0 traffic; I2C0 is four EEPROM-like slaves, SPI1 is the DSP link, GPIO is a boot-time bit stream). The panel link is not identified.

## Higher Goal

Compare our Program / Synth OLED geometry (rows, fonts, widths, title case) with the real screens instead of manual text and videos.

## Acceptance Criteria

- [ ] The link from the host firmware to the panel boards is identified (candidate: EMIFA CS2 `0x6000....`, scan loop `0xC024DA6C`) and documented with addresses.
- [ ] `TEXT_MEASURE` and `EVENT_INSERT` run unstubbed with a font object built from the image (font descriptor layout decoded).
- [ ] The frame buffer / draw primitive is found and the Program view (`A:11` layout) is captured as PBM in a local scratch dir.
- [ ] One menu page (e.g. System page 1) is captured by injecting the panel event that opens it.
- [ ] Findings (fonts, row structure, capital letters in titles) recorded in the research note and compared with `bay_program/oled_paint.rs`; case-only differences of the note (s3, 19 literals) settled.

## Out of Scope

Shipping any firmware-derived pixel data; changing the GUI layout before the comparison exists.

# ARM OLED render (local research tooling)

Status: v1.0 (2026-11-14) · Evidence: [research/arm-oled-render](../research/arm-oled-render.md) · Bean `ns4-2gdc`

Purpose: run the OS v1.66 host firmware on the ARM harness with both OLED panels attached and capture the pixels it sends, so the Program / Synth OLED layout of the GUI can be compared with the real screens. Local only: the firmware, the font bitmaps and every captured frame stay under `target/` or `.reference/` (git-ignored). The repo holds code, tests with synthetic fixtures and small derived numbers.

## Components

| File | Role |
|---|---|
| `tools/arm_oled.py` | Pure functions: font object parser (`parse_font`, `font_table`), `text_width` (as `0xC001B9D0`), reference rasteriser `draw_text`, frame encoders (`to_pbm`, `to_png`, `to_ascii`), controller model `Ssd13xx`, panel link `PanelLink` |
| `tools/arm_panel.py` | Harness glue: `install_panel_link` (presence answer + store capture on EMIFA CS2), `boot_with_panels` (timer interrupt in the wait loop `0xC0075D38..0xC0075D50`), firmware oracles `firmware_draw` / `firmware_width`, CLI `fonts` / `oracle` / `capture` |
| `tools/arm_boot_full.py` | `prepare(..., display_stubs=False)` / `install_ui_stubs(..., display_stubs=False)` leave `EVENT_INSERT` and `TEXT_MEASURE` unstubbed |

## Interfaces

| Item | Value | Status |
|---|---|---|
| Panel presence | read bit 7 of `0x60000010` (panel 0), `0x60000020` (panel 1) inside `0xC024D648..0xC024D70C` | VERIFIED |
| Data / command line | latch byte `0x60000025`; display 0 bit `0x04`, display 1 bit `0x20`; set = data | VERIFIED |
| Byte ports | display 0 `0x60000017`, display 1 `0x60000027` | VERIFIED |
| Controller | SSD1309-class 128 x 64 mono, horizontal addressing, init `FD 12 AE D5 A0 A8 3F D3 20 60 C8 A1 DA 12 81 DF D9 82 DB 34 A4 A6 20 00` | VERIFIED (stream); family ASSUMED |
| Frame | 1024 bytes, `page * 128 + x`, bit `i` = row `page * 8 + i`; the firmware buffer `[display + 0x18]` has the same layout | VERIFIED |
| Font object | `{height u32, first u8, last u8, pad u16, table u32, mask u32}`, table entries `{ptr, width}`, glyph = `width` u32 column words (bit `i` = row `i`) | VERIFIED (oracle, 18 of 18 fonts) |
| Font array | `0xC029E5AC`, 17 pointers; crash-text font `0xC0282E48` | VERIFIED |
| Draw routine | `0xC004064C(text, font, plane, mask, [sp] x, [sp+4] y, [sp+8] limit)`; used by the crash screen only | VERIFIED |
| Measure routine | `0xC001B9D0(text, font)`: sum of advances of the first line, minus 1 for an empty last column | VERIFIED |

## Behaviour

- `capture` boots to the main loop (about 700 M instructions, several minutes), optionally runs `--steady N` more, and writes one 3x PNG per distinct frame of each display to `--out` (default `target/oled`).
- `oracle` is the regression check of the decode: it must print `oracle: all equal`.
- A frame is complete after 1024 data bytes without a command in between.

## Known limits

- Display 1 stays dark; the Program screen shows `A:11` + `Write error` because the program storage is not modelled; no button input reaches the UI (see the research note, section 6).
- No pixel of a menu or the Synth page has been captured, so no title-case question is settled yet.

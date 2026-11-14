# ARM OLED render (local research tooling)

Status: v1.1 (2026-11-14) · Evidence: [research/arm-oled-render](../research/arm-oled-render.md) · Bean `ns4-2gdc`

Purpose: run the OS v1.66 host firmware on the ARM harness with both OLED panels, the program-memory EEPROMs and a front-panel key matrix attached, press keys and capture the pixels and the widget text it produces, so the Program / Synth OLED layout of the GUI can be compared with the real screens. Local only: the firmware, the font bitmaps and every captured frame stay under `target/` or `.reference/` (git-ignored). The repo holds code, tests with synthetic fixtures and small derived numbers.

## Components

| File | Role |
|---|---|
| `tools/arm_oled.py` | Pure functions: font object parser (`parse_font`, `font_table`), `text_width` (as `0xC001B9D0`), reference rasteriser `draw_text`, frame encoders (`to_pbm`, `to_png`, `to_ascii`), controller model `Ssd13xx`, panel link `PanelLink` |
| `tools/arm_panel.py` | Harness glue: `install_panel_link(emu, keys=None)` (presence answer, key matrix reads, store capture on EMIFA CS2), `panel_block`, `boot_with_panels` (timer interrupt in the wait loop `0xC0075D38..0xC0075D50`), firmware oracles `firmware_draw` / `firmware_width`, CLI `fonts` / `oracle` / `capture` |
| `tools/arm_i2c.py` | `I2cBus`: register-level I2C0 model (`ICSTR` / `ICCNT` / `ICSAR` / `ICDXR` / `ICDRR` / `ICMDR`) with four 64 KB 24xx `Eeprom` slaves at `0x50..0x53`; `install_i2c` hooks it to the `0x01C22000` window |
| `tools/arm_keys.py` | `KeyMatrix`: idle-high key matrix (strobe `0x60000015`, ports `0x60000010` / `0x60000020`), `all_keys()` (98 bits) |
| `tools/arm_screens.py` | `Session`: boot once, save / restore the machine plus EEPROMs, `press` keys for N ticks, `Capture` of frames and widget strings (`TextEvent`: widget, font, upper-case flag, text); CLI `boot` / `press` / `sweep` |
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
| Program memory | four 64 KB EEPROMs on I2C0 slaves `0x50..0x53`, 16-bit address, probed with zero-length writes; a blank store is formatted at boot (header `79 B8 F8 99 01 00 00 00` on device 0) and shows `<empty>` | VERIFIED (run); device type ASSUMED 24LC512 |
| I2C driver | polls `ICSTR` `ARDY` `0x04` / `RRDY` `0x08` / `NACK` `0x02`; `MST` `0x0400` clears after a stop (`0xC005B09C..0xC005B2B0`) | VERIFIED (disassembly), bit layout ASSUMED (TI TRM) |
| Key scan | one column per call (`0x800014E8`): strobe `0x01..0x40` (keys), `0x80` (kind 1); `word = byte[0x20] \| halfword[0x10] << 16`, stored `~(word & mask)`; a pressed key reads 0 | VERIFIED (table `0xC05298D8`), polarity ASSUMED then confirmed by the run |
| Keys | Shift `(0x02, 1, 4)`; Program 3 / 4 / 7 / 8 = `(0x08, 0, 2)` / `(0x04, 0, 2)` / `(0x08, 0, 4)` / `(0x04, 0, 4)` (Shift + these open Program Organize, AUX KB, MIDI, EXTERN); Preset keys `(0x08, 0, 0 / 1 / 5)` | EMPIRICAL (sweep) |
| Text widget | set-text routine `0xC007E3AC`: text at `[w + 0x54]` (0x40 bytes), font `[w + 0x2C]`, upper-case flag `[w + 0x88]` applied before the hook address `0xC007E43C` | VERIFIED (disassembly, run) |
| Screen geometry | home: title y = 3, name y = 18 (13 px), rule y = 33, rows from y = 36 pitch 10 (8 px); title bar rows 0 to 10 inverted; menu label y = 15 (8 px), value y = 26 (13 px); popup label y = 45, value y = 55; soft buttons rows 53 to 63 | VERIFIED (measured, research note s9) |

## Behaviour

- `arm_panel.py capture` boots to the main loop (about 700 M instructions, several minutes), optionally runs `--steady N` more, and writes one 3x PNG per distinct frame of each display to `--out` (default `target/oled`); it models neither storage nor keys (it shows the `Write error` state).
- `arm_screens.py boot` boots with the EEPROMs and key matrix (about 800 M instructions; 2 to 3 minutes), runs 100 ticks and writes the machine state (`--out`, default `target/oled/state.bin`). `press` restores it in seconds, holds `--key strobe,port,bit` (repeatable for chords) for `--hold` ticks (a tick is 200,000 instructions, ASSUMED 1 ms; Program buttons need 60 or more) and writes the last frame of each display, its ASCII form and the widget strings to `--out`. `sweep` does this for every key bit alone.
- After a restore the `PanelLink` is synchronised (`Session.sync_panels`): latch byte from `0x60000025`, horizontal addressing, page and column 0; otherwise the firmware's skipped latch / command stores turn data into commands.
- `oracle` is the regression check of the decode: it must print `oracle: all equal`.
- A frame is complete after 1024 data bytes without a command in between.

## Known limits

- The program store is blank: the Program screen shows `<empty>`; the record format is not decoded (research note s6, s11).
- Encoders and dials are not driven (`0x80` kind 1 columns, SPI / ADC inputs); Program 1 / 2 / 5 / 6 menus, Page, Store, Undo and Layer Init keys are not identified.
- Display 1 only shows the Synth page; other screens leave it blank, as on the unit.
- Title case: settled for the screens reached (title bars upper-case through the widget flag); open for the rest (research note s10).

# ARM OLED render (local research tooling)

Status: v1.3 (2026-11-14) · Evidence: [research/arm-oled-render](../research/arm-oled-render.md) · Beans `ns4-2gdc`, `ns4-pyqg`

Purpose: run the OS v1.66 host firmware on the ARM harness with both OLED panels, the program-memory EEPROMs and a front-panel key matrix attached, press keys and capture the pixels and the widget text it produces, so the Program / Synth OLED layout of the GUI can be compared with the real screens. Local only: the firmware, the font bitmaps and every captured frame stay under `target/` or `.reference/` (git-ignored). The repo holds code, tests with synthetic fixtures and small derived numbers.

## Components

| File | Role |
|---|---|
| `tools/arm_oled.py` | Pure functions: font object parser (`parse_font`, `font_table`), `text_width` (as `0xC001B9D0`), reference rasteriser `draw_text`, frame encoders (`to_pbm`, `to_png`, `to_ascii`), controller model `Ssd13xx`, panel link `PanelLink` |
| `tools/arm_panel.py` | Harness glue: `install_panel_link(emu, keys=None)` (presence answer, key matrix reads, store capture on EMIFA CS2), `panel_block`, `boot_with_panels` (timer interrupt in the wait loop `0xC0075D38..0xC0075D50`), firmware oracles `firmware_draw` / `firmware_width`, CLI `fonts` / `oracle` / `capture` |
| `tools/arm_i2c.py` | `I2cBus`: register-level I2C0 model (`ICSTR` / `ICCNT` / `ICSAR` / `ICDXR` / `ICDRR` / `ICMDR`) with four 64 KB 24xx `Eeprom` slaves at `0x50..0x53`; `install_i2c` hooks it to the `0x01C22000` window |
| `tools/arm_keys.py` | `KeyMatrix`: idle-high key matrix (strobe `0x60000015`, ports `0x60000010` / `0x60000020`), `all_keys()` (98 bits), `ENCODER_PAIRS` / `encoder_steps` (Gray sequences for strobe `0x80`) |
| `tools/arm_screens.py` | `Session`: boot once, save / restore the machine plus EEPROMs, `load_eeproms`, `press` keys for N ticks, `turn` an encoder, `Capture` of frames and widget strings (`TextEvent`: widget, font, upper-case flag, text); CLI `boot [--eeprom image]` / `press` / `sweep [--shift]` |
| `tools/arm_events.py` | Event chain of the panel (encoder counter -> queue entry `{tag, source, index, delta}` -> UI task -> screen -> value widget): constants, `decode_event`, `ENCODER_PAIR_OF_ID`, `trace_turn(session, pair, detents)` and a `turn` CLI that prints the dispatched events |
| `tools/arm_store.py` | Program-store record layout (`pack_clusters`, `unpack_clusters`, `unpack_record`, `set_name`, `join_image` / `split_image`) and `store_current`: Store key twice with Memory Protect forced off, then the storage task step by hand (`flush`); CLI `store` writes a 256 KB EEPROM image for `boot --eeprom` |
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
| Program record | 128-byte clusters from `0x40` (`01 00 <i> 00` + 124 bytes); stream `size u32` (860), `crc32 u32` of the 824-byte body, name[24], `p4sn`, location, body; the name is outside the CRC; one record per Store, flushed by the storage task step `0xC0240518` | VERIFIED (run, `tools/tests/test_arm_store.py` for the layout) |
| Memory Protect | the Store handler tests it at `0xC0150540` (`cmp r0, #1`); on (default) it shows `Store Disabled / Memory Protected / in System menu` | VERIFIED (disassembly, run) |
| I2C driver | polls `ICSTR` `ARDY` `0x04` / `RRDY` `0x08` / `NACK` `0x02`; `MST` `0x0400` clears after a stop (`0xC005B09C..0xC005B2B0`) | VERIFIED (disassembly), bit layout ASSUMED (TI TRM) |
| Key scan | one column per call (`0x800014E8`): strobe `0x01..0x40` (keys), `0x80` (kind 1); `word = byte[0x20] \| halfword[0x10] << 16`, stored `~(word & mask)`; a pressed key reads 0 | VERIFIED (table `0xC05298D8`), polarity ASSUMED then confirmed by the run |
| Keys | Shift `(0x02, 1, 4)`; Program 1 .. 8 = `(0x10, 0, 2)` `(0x10, 0, 3)` `(0x08, 0, 2)` `(0x04, 0, 2)` `(0x10, 0, 4)` `(0x10, 0, 5)` `(0x08, 0, 4)` `(0x04, 0, 4)`; Shift + these open System, Sound (`PROGRAM LEVELS`), Program Organize, AUX KB, Output, Pedal, MIDI, EXTERN; Store `(0x10, 0, 1)`; Page next `(0x08, 0, 6)`; Layer Init = Shift + `(0x08, 0, 3)`; Preset keys `(0x08, 0, 0 / 1 / 5)` | EMPIRICAL (sweep, 100-tick holds) |
| Encoders | strobe `0x80`: five A / B pairs, decoded by `0xC0082A9C` into +1 / -1 counters (indices 7, 6, 15, 14, 13), posted as events (`0xC007B0BC`) on the UI ring and dispatched by `0xC00A0240`; **pair 1 (index 6) is the Program dial** (pages the menus, steps the presets, opens the program list `Import into A:11`); pair 0 (index 7) reaches the menu value widget but the parameter step `0xC00EEEB0` finds an empty container; pairs 2 to 4 find no widget | VERIFIED (hooks, frames; research note s15) |
| Text widget | set-text routine `0xC007E3AC`: text at `[w + 0x54]` (0x40 bytes), font `[w + 0x2C]`, upper-case flag `[w + 0x88]` applied before the hook address `0xC007E43C` | VERIFIED (disassembly, run) |
| Screen geometry | home: title y = 3, name y = 18 (13 px), rule y = 33, rows from y = 36 pitch 10 (8 px); title bar rows 0 to 10 inverted with the page counter right-aligned; menu label y = 15 (8 px), value y = 26 (13 px); popup label y = 45, value y = 55 under the home header; soft buttons rows 53 to 63, outlined, the active one lit; home rows start with an 8 px icon, text from x = 12 | VERIFIED (measured, research note s9, s12; GUI: `oled_paint::grid` / `bar`, `oled_icons`) |

## Behaviour

- `arm_panel.py capture` boots to the main loop (about 700 M instructions, several minutes), optionally runs `--steady N` more, and writes one 3x PNG per distinct frame of each display to `--out` (default `target/oled`); it models neither storage nor keys (it shows the `Write error` state).
- `arm_screens.py boot` boots with the EEPROMs and key matrix (about 800 M instructions; 2 to 3 minutes), runs 100 ticks and writes the machine state (`--out`, default `target/oled/state.bin`). `press` restores it in seconds, holds `--key strobe,port,bit` (repeatable for chords) for `--hold` ticks (a tick is 200,000 instructions, ASSUMED 1 ms; Program buttons need 60 or more) and writes the last frame of each display, its ASCII form and the widget strings to `--out`. `sweep` does this for every key bit alone.
- After a restore the `PanelLink` is synchronised (`Session.sync_panels`): latch byte from `0x60000025`, horizontal addressing, page and column 0; otherwise the firmware's skipped latch / command stores turn data into commands.
- `arm_store.py store` restores the blank-boot state, presses Store twice (Memory Protect forced off at `0xC0150540`), calls the storage task step until the cache is written and writes the four EEPROMs as one 256 KB file (`--name` renames the record). `arm_screens.py boot --eeprom <file>` then boots with that store in about a minute (no format pass); frames and states stay under `target/`.
- `oracle` is the regression check of the decode: it must print `oracle: all equal`.
- A frame is complete after 1024 data bytes without a command in between.

## Known limits

- The store holds the default program only (renamed in the image); other locations need the directory entry format (device 3 `0xFE98`), Store As and the name entry need the encoders; the program body (824 bytes) is not decoded (research note s13).
- Encoder events are delivered; only the Program dial changes a screen. Menu values, Store As name entry and dials behind SPI / ADC are not driven (research note s15.2); Undo (Shift + Solo): the dialog class and its refresh method `0xC01327BC` are found, its constructor and trigger are not (s15.4); Page previous is not found in the 97 matrix keys (s12.1).
- Display 1 only shows the Synth section pages (they need the Synth page open first); it is blank on every other screen, as on the unit.
- Title case: settled for the screens reached (title bars upper-case through the widget flag); `UNDO`, `FILTER`, `OSC PITCH`, arp `LEFT` / `RIGHT` still open (research note s14).
- The row texts `B3` and `Pure Saw` of the home screen are set but not in the final frame (no DSP, sound not loaded: ASSUMED).

# Rendering the real OLED screens from the ARM emulator

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66 ARM0 image, load base `0xC0000000`) and `bank0_SRAM.bin`; the ARM harness (`tools/arm_boot_full.py`, `tools/arm_steady.py`, `unicorn` 2.1); new tools `tools/arm_panel.py` (run + capture) and `tools/arm_oled.py` (fonts, panel stream, encoders); `tools/arm_xref.py dis` (capstone). Prior notes: [oled-strings-vs-firmware](oled-strings-vs-firmware.md) s5 (the blockers), [arm-fw-emulation](arm-fw-emulation.md), [arm-organ-messages](arm-organ-messages.md) s10.
- **Question:** the blockers of [oled-strings-vs-firmware](oled-strings-vs-firmware.md) s5 (display path stubbed, font undecoded, draw primitive unknown, panel link unidentified): can the ARM emulator draw the firmware's own screens so that our OLED layout can be compared with the real one?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Panel link | EMIFA CS2 (`0x6000....`): panel presence on `0x60000010` / `0x60000020`, a latch byte `0x60000025` (data / command line), one byte port per display `0x60000017`, `0x60000027`. Two 128 x 64 OLEDs. | VERIFIED (run) |
| Why the old harness saw no display | The board bring-up `0xC024B484` creates the display objects only if both panel boards answer the presence read; the harness answered `0`. | VERIFIED (run) |
| Text measure / refresh queue | `TEXT_MEASURE` `0xC001B9D0` and `EVENT_INSERT` `0xC001B67C` run **unstubbed**: once the displays exist their objects and fonts are real. Boot still reaches the main loop `0xC00A0248` (700 M instructions, 6 timer interrupts injected in one wait loop). | VERIFIED (run) |
| Font | Decoded (section 3); a 60-line reference rasteriser reproduces the firmware's own draw routine pixel for pixel on all 18 fonts. | VERIFIED (`arm_panel.py oracle`) |
| Frame buffer / draw | Frame buffer `[display + 0x18]`, 1024 bytes, flushed by `0xC0076924`; the controller stream is captured directly from the bus, so no draw routine has to be called by hand. | VERIFIED (run) |
| Screens captured | The OS boot splash (`Nord Stage 4` / `Compact` / progress bar / `OS v1.66`, 114 distinct frames on display 0) and the Program home screen in its error state (`A:11` + `Write error`). Display 1 stays blank. | VERIFIED (run) |
| Real Program screen with a program name | **Not yet**: the program load ends in `Write error` because the storage back end is not modelled (section 6). | blocker |

## 2. The panel link

### 2.1 Presence and display creation (VERIFIED)

`0xC024B484` (the board bring-up, no direct caller; it is the function that logs `DSP Boot Error`, see [arm-dsp-protocol](arm-dsp-protocol.md)) calls `0xC024D648(i, &presence[i], &type[i])` for i = 0, 1 and skips the display setup (`b 0xC024B7B4`) unless both presence bytes are set. `0xC024D648`: stores `1` then `2` to `0x60000015`, waits a few cycles, reads `0x60000010` (i = 0) or `0x60000020` (i = 1); bit 7 = board present, bit 6 selects the answer value. The harness had left those words `0`. `arm_panel.panel_answer` returns `0x80` for those two addresses while the program counter is inside `0xC024D648..0xC024D70C` and `0` everywhere else (the key scan `0xC024DA6C` reads the same words).

With the answer in place `0xC024B484` builds two display objects (`0xC007744C`, one per panel, transport descriptors with `0x60000025`), stores them at `[0xC0583118 + 0x48]` / `+0x4C` and calls `0xC00770B4(2)` (the controller init, `0xC00770BC`). The object array that routine walks is `0xC058469C` (2 entries).

### 2.2 Display and transport objects (VERIFIED, read after the boot)

| Object | Fields |
|---|---|
| display (`0x28` bytes) | `+0` transport, `+4` width `0x80`, `+8` height `0x40`, `+0x10` kind, `+0x14` enabled, `+0x18` frame buffer (`0x400` bytes), `+0x20` second plane |
| transport | vtable `0xC029FFD8`; `+4` command port, `+8` data port, `+0xC` latch register `0x60000025`, `+0x1C` latch mask |

Display 0: ports `0x60000017`, latch mask `0x04`. Display 1: ports `0x60000027`, latch mask `0x20`. The other latch bits (`0x01` `0x02`, `0x08` `0x10`) are driven by the same code (chip select / reset, not decoded). Writers: command `0xC0072790` clears the mask bit in a shadow byte (`0xC05842F8 + [obj+0x10] + 8`), stores the shadow to the latch, then stores the byte to the port; data `0xC00727F0` / `0xC0072890` set the bit first. So **latch bit set = data, clear = command**; `arm_oled.PanelLink` implements exactly this.

### 2.3 Controller and frame format (VERIFIED, init stream `0xC00770BC..0xC0077300`)

The init sends `FD 12` (command unlock), `AE` (off), `D5 A0`, `A8 3F` (64 mux), `D3 20`, `60`, `C8`, `A1`, `DA 12`, `81 DF` (contrast), `D9 82`, `DB 34`, `A4`, `A6`, `20 00` (horizontal addressing): an SSD1309-class monochrome 128 x 64 controller. The flush `0xC0076924(display)` sends `B0`, `00`, `10` and then 1024 data bytes from `[display + 0x18]`: byte `page * 128 + x`, bit `i` (LSB first) = row `page * 8 + i`. The same layout is the frame buffer of every draw routine (`add r3, r3, r2, lsl #7` per page in `0xC004064C`, `0xC007xxxx`). The vertical / horizontal remap (`C8`, `A1`) and the display offset (`D3 20` with start line `60`) cancel or flip the RAM on the glass; RAM is shown upright because the RAM text reads upright (ASSUMED: the glass shows RAM as is; a video frame would confirm).

## 3. The font objects (VERIFIED)

Object (16 bytes): `{height u32, first u8, last u8, pad, table u32, mask u32}` with `mask = (1 << height) - 1`. `table` holds `last - first + 1` entries `{ptr, width}`; a glyph is `width` u32 column words, bit `i` = row `i` from the top, `1` = lit; the glyph advance is `width` (the gap column is part of the glyph). `TEXT_MEASURE` sums the advances of the first line and subtracts 1 when the last glyph's final column is empty (`0xC001BA40..0xC001BA6C`). Characters outside `first..last` draw the first glyph (the space) and add no advance in the measure.

Font array `0xC029E5AC` (17 pointers) plus the crash-text font `0xC0282E48`. `python tools/arm_panel.py fonts target/fw_unpacked/bank0_CODE.bin` prints:

| idx | object | height | chars | idx | object | height | chars |
|---:|---|---:|---|---:|---|---:|---|
| 0 | `0xC029E0AC` | 13 | `0x20..0xBF` | 9 | `0xC029E02C` | 8 | `0x41..0x5F` |
| 1 | `0xC029E09C` | 13 | `0x20..0xBF` | 10 | `0xC029E01C` | 7 | `0x20..0x7E` (wide glyphs, not text) |
| 2 | `0xC029E08C` | 10 | `0x20..0x95` | 11 | `0xC029E00C` | 32 | `0x20..0x6A` |
| 3 | `0xC029E5F0` | 8 | `0x20..0xB9` | 12 | `0xC029DFFC` | 17 | `0x41..0x5E` |
| 4 | `0xC029E07C` | 8 | `0x20..0xB4` | 13 | `0xC029DFEC` | 6 | `0x20..0xD8` |
| 5 | `0xC029E06C` | 8 | `0x20..0x8B` | 14 | `0xC029DFDC` | 2 | `0x20..0x3B` |
| 6 | `0xC029E05C` | 11 | `0x20..0x8E` | 15 | `0xC029DFCC` | 3 | `0x20..0x52` |
| 7 | `0xC029E04C` | 11 | `0x20..0x8B` | 16 | `0xC029DFBC` | 9 | `0x20..0x53` |
| 8 | `0xC029E03C` | 5 | `0x20..0x7B` | crash | `0xC0282E48` | 8 | `0x20..0x8C` |

**Validation.** `uv run --with unicorn python tools/arm_panel.py oracle ...` renders a sample string in each of the 18 fonts through the firmware's own draw routine `0xC004064C` (arguments `(text, font, plane, mask of 0xFF, x, y, limit)`, the last three on the stack) and through `arm_oled.draw_text`, and compares the width from `0xC001B9D0` with `arm_oled.text_width`: **18 of 18 equal** (output `oracle: all equal`).

### 3.1 The `0x89` glyph of `%c Memory Protected` (VERIFIED glyph shapes, ASSUMED font)

`Unable to Paste\n%c Memory Protected` (`0xC0283790`, argument `0x89` from `0xC003D33C`) draws a font glyph, not a letter. The glyph depends on the font: in the 13 px text fonts (idx 0 and 1) `0x89` is a 4 x 4 hollow box (5 columns with the gap); in the 8 px fonts idx 3, 4 and 5 it is an 11 x 8 filled triangular icon; in the crash font a 9 x 7 rounded blob. The message line uses which font is open: the `Write error` message of the same screen is font idx 0 (section 5), so the message is ASSUMED to use idx 0 and the prefix to be the 4 x 4 box (not a padlock). Confirming needs the Program screen with a protected memory (section 6).

## 4. What was un-stubbed (VERIFIED)

`arm_boot_full.prepare(..., display_stubs=False)` leaves `EVENT_INSERT` and `TEXT_MEASURE` running. With the displays present:

- the 188 + 144 calls of `TEXT_MEASURE` that used to see a null font are gone; the fonts passed are real objects (`0xC029E02C`, `0xC029E5F0`, `0xC029E0AC`, ...);
- `EVENT_INSERT` (`0xC001B67C`) is the **dirty-region queue** of the display: it allocates a `0x28` byte record `{layer, x, y, ...}`, converts `y` to a byte offset `((y - y % 8) * 128) / 8 + x` capped at `0x400` and pushes it on a sorted list in the compositor. Its old "owner" argument was null because the display existed in no list;
- the other stubs of `install_ui_stubs` (SPI answer, count check, null font page) are kept: they are not display stubs.

One more harness fix was needed: after the displays are created, the boot blocks in `0xC0075D0C` (posts a request with `0xC00A05D4`, then spins on `[req + 0x1C]` until another context serves it). The timer interrupt handler (`0x800007D8`, [arm-organ-messages](arm-organ-messages.md) s10) is the other context; `arm_panel.boot_with_panels` injects it whenever the program counter sits in `0xC0075D38..0xC0075D50` (6 times in a full boot).

## 5. Captured screens (VERIFIED, numbers EMPIRICAL)

`uv run --with unicorn python tools/arm_panel.py capture target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --out target/oled --steady 150000000` boots (700.2 M instructions), runs 150 M more with the 1 kHz timer and writes one PNG per distinct frame under `target/oled/` (git-ignored; the images contain firmware font bitmaps, never commit them): display 0 sent 191 frames (114 distinct), display 1 sent 6 frames (1 distinct, all dark).

Each text line was identified by pixel-exact comparison with the decoded fonts (a crop of the frame equals the crop of `arm_oled.draw_text`):

| Screen | Line | Font | Origin (x, y) of the text box | Width |
|---|---|---|---|---:|
| boot splash | `Nord Stage 4` | idx 0 (13 px) | (22, 10) | 83 px, centred: (128 - 83) / 2 = 22.5 |
| | `Compact` | idx 3 or crash font (8 px; identical glyphs here) | (43, 24) | 41 px |
| | progress bar | frame (1 px outline) | rows 41..46 | grows with load |
| | `OS v1.66` | idx 3 (8 px) | (43, 50) | 41 px |
| Program screen after the splash | `A:11` | idx 0 (13 px) | (1, 3) | |
| | `Write error` | idx 0 (13 px) | (1, 18) | |

So the location title is the **13 px text font at the top-left corner**, one line every 15 px; in the error state there is no name line and no section rows yet. `Compact` is the model name string at `0xC05296FC` (`Compact`, `73`, `88`), chosen from the key bed (the 73-note Compact model here).

## 6. Blockers left (exact)

1. **Program storage.** The Program screen shows `A:11` and the status text `Write error`, an entry of the result-code string table at `0xC02B8198` (`No action`, `Not found`, `Read error`, `Write error` = index 3, `Version error`, `Format error`, `Out of range`; strings `0xC02B7E50..0xC02B7EA0`). The operation that fails is the program load / store of the storage layer, which talks to a device the harness does not model (flash / SD / USB; the I2C EEPROMs `0x50..0x53` of [oled-strings-vs-firmware](oled-strings-vs-firmware.md) s5 are boot settings). Until that device answers, no program name, section rows or soft buttons are drawn. Next step: break on the writer of the status code (find who stores `3` for that table; the consumer of `0xC02B8198` has no literal pool reference, so look through the status object of `0xC01768xx`).
2. **No input.** The front-panel scan (`0xC024DA6C`, EMIFA CS2 `0x60000015` strobe, `0x60000010` read; each answer bit is a button / encoder state, ASSUMED) still answers `0`; menus, the Synth page and the Organ page need panel events through that read.
3. **Display 1 stays dark.** It belongs to the second panel board; its screen is only drawn once the layer it shows has content (ASSUMED).
4. **The widget draw path is not hand-callable.** The UI draws through widget methods (`0xC0079C18` item draw, `0xC007BBF4` text item, region compositor `0xC007EA10`); the standalone `0xC004064C` is used by the crash-text screen only (no call in a full boot). That is fine for capture (the bus carries the result) but a "render this string like screen X" call needs the widget object graph from the main loop.

## Implications

- The OLED geometry of the real unit is now measurable offline: 128 x 64 per panel, fonts and metrics of section 3, text origins of section 5. Compared with `bay_program/oled_paint.rs` / `oled_home.rs` (monospace `FontId`, big title + name line + section rows): the real big title is the **13 px proportional font at x = 1, y = 3**, the next line starts at y = 18 (15 px pitch); our monospace scale should use 128 x 64 as the reference grid with a 13 px line.
- The 8 px fonts have 11 px wide icons at `0x80..0xBF`; the `%c` of `Memory Protected` is a font icon, ASSUMED to be the 4 x 4 box of the 13 px font (section 3.1), not a padlock.
- `Write error` is a firmware status text; our GUI has nothing to compare for it. It does show that a status message replaces the name line and section rows (the screen has only the location and the message).
- The "titles in capitals" question (follow-up 2 of [oled-strings-vs-firmware](oled-strings-vs-firmware.md)) is still open: no menu screen was reached (blocker 2).

## Follow-ups

1. Model the program storage (blocker 1) so the load succeeds, then capture the Program screen with a name and compare rows with `oled_home.rs` (bean `ns4-2gdc`).
2. Model the panel scan read `0x60000010` / `0x60000020` as a button matrix and inject the MENU / PAGE buttons to capture a menu page (System page 1) and the Synth / Organ screens.
3. Decode which font index each widget class uses (callers of `0xC001C9EC` pass the font in the config struct `{x, y, w, h, font, ...}`, e.g. `0xC0154D7C` uses idx 3 with `(0, 0, 128, 8)`) to document the row structure of every screen offline, without running the firmware.

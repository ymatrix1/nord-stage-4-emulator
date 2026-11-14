# Rendering the real OLED screens from the ARM emulator

- **Checked:** 2026-11-14 (third pass, bean `ns4-pyqg`)
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66 ARM0 image, load base `0xC0000000`) and `bank0_SRAM.bin`; the ARM harness (`tools/arm_boot_full.py`, `tools/arm_steady.py`, `unicorn` 2.1); tools `tools/arm_panel.py` (run + capture) and `tools/arm_oled.py` (fonts, panel stream, controllers), `tools/arm_i2c.py` (I2C0 and the four EEPROMs), `tools/arm_keys.py` (key matrix, encoders), `tools/arm_store.py` (program store), `tools/arm_screens.py` (boot once, press keys, capture frames and widget text); `tools/arm_xref.py dis` (capstone), `tools/ns4_oled_strings.py`. Prior notes: [oled-strings-vs-firmware](oled-strings-vs-firmware.md) s5 (the blockers), [arm-fw-emulation](arm-fw-emulation.md), [arm-organ-messages](arm-organ-messages.md) s10.
- **Question:** the blockers of [oled-strings-vs-firmware](oled-strings-vs-firmware.md) s5 (display path stubbed, font undecoded, draw primitive unknown, panel link unidentified): can the ARM emulator draw the firmware's own screens so that our OLED layout can be compared with the real one? Second pass (bean `ns4-2gdc`): what device does the program load talk to, how do panel buttons reach the firmware, what do the real menu / Synth / popup screens look like, and what is the capitalisation of the 19 case-only strings?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Panel link | EMIFA CS2 (`0x6000....`): panel presence on `0x60000010` / `0x60000020`, a latch byte `0x60000025` (data / command line), one byte port per display `0x60000017`, `0x60000027`. Two 128 x 64 OLEDs. | VERIFIED (run) |
| Why the old harness saw no display | The board bring-up `0xC024B484` creates the display objects only if both panel boards answer the presence read; the harness answered `0`. | VERIFIED (run) |
| Text measure / refresh queue | `TEXT_MEASURE` `0xC001B9D0` and `EVENT_INSERT` `0xC001B67C` run **unstubbed**: once the displays exist their objects and fonts are real. Boot still reaches the main loop `0xC00A0248` (700 M instructions, 6 timer interrupts injected in one wait loop). | VERIFIED (run) |
| Font | Decoded (section 3); a 60-line reference rasteriser reproduces the firmware's own draw routine pixel for pixel on all 18 fonts. | VERIFIED (`arm_panel.py oracle`) |
| Frame buffer / draw | Frame buffer `[display + 0x18]`, 1024 bytes, flushed by `0xC0076924`; the controller stream is captured directly from the bus, so no draw routine has to be called by hand. | VERIFIED (run) |
| Screens captured | Boot splash and Program home (first pass) plus, second pass: Program home with section rows, the edited marker `E`, parameter popups, the Organ / Piano / Synth Preset screens, Section Edit, Morph assign, Program Organize, the AUX KB / MIDI / EXTERN settings menus and the Synth `OSC WAVEFORM` page on display 1 (section 8). | VERIFIED (`arm_screens.py`) |
| Real Program screen with a program name | **Not yet, but the error is gone**: the program store is four 64 KB I2C EEPROMs (`0x50..0x53`); modelled blank, the firmware formats them and the screen shows `A:11` + `<empty>` instead of `Write error` (section 6). A real name needs a populated store (record format not decoded). | partial |
| Panel input | The scan reads `0x60000010` / `0x60000020` after a strobe at `0x60000015`; a pressed key reads 0 (section 7). `arm_keys.KeyMatrix` injects presses; 98 key bits swept, Shift and Program 3 / 4 / 7 / 8 identified, four menu screens opened. | VERIFIED (run) |
| Case-only strings | Settled where a real screen exists: title bars upper-case mixed-case firmware strings through a per-widget flag, so `ORGAN PRESET` and `AUX KB SETTINGS` are right and `AUX KB` / `PROGRAM ORGANIZE` were wrong (section 10). | VERIFIED (hook) |
| Program store (third pass) | A Store writes a RAM cache that the storage task flushes to the EEPROMs; the record is a chain of 128-byte clusters, `size`, `crc32` of the 824-byte body, a 24-byte name (outside the CRC), `p4sn`. `arm_store.py` stores the default program, renames it in the image and boots with it: the home screen shows `A:11` / `Studio Grand`. | VERIFIED (run, CRC check) |
| Keys (third pass) | Program 1 / 2 / 5 / 6 = `(0x10, 0, 2 / 3 / 4 / 5)`, Store `(0x10, 0, 1)`, Page `(0x08, 0, 6)`, Layer Init = Shift + `(0x08, 0, 3)`; System, Sound, Output, Pedal menus and all 56 pages of the seven menus read; Synth pages reached through the Synth page. Encoders decode to counters, no screen reacts. Undo not found. | VERIFIED (sweeps) / EMPIRICAL |

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
| | `Write error` (first pass; with the EEPROM model of section 6 this line is `<empty>`, same font and position) | idx 0 (13 px) | (1, 18) | |

So the location title is the **13 px text font at the top-left corner**, one line every 15 px; in the error state there is no name line and no section rows yet. `Compact` is the model name string at `0xC05296FC` (`Compact`, `73`, `88`), chosen from the key bed (the 73-note Compact model here).

## 6. Program storage: four I2C EEPROMs (blocker 1, VERIFIED)

The first pass stopped at `A:11` + `Write error`. The text is not a write failing: it is the status string of a failed **read**.

**The failing call (VERIFIED, breakpoints in the boot, `tools/arm_screens.py boot`).**

| Step | Address | What happens |
|---|---|---|
| Home screen row provider | `0xC0169DE8` (vtable `0xC02C86A8` slot `+0x18`) | asks the program model for the name text and shows the message it gets back |
| Generic "read then map the status to text" | `0xC0106024` (`0xC0106540..0xC0106864`) | calls `0xC006E47C(fs, key, buf)` with the 8-byte entry key; maps the result code through `0xC02B810C[code]` (`0, 2, 2, 3, 4, 7, 4, 4, 4, 4, 4, 7, ...`) and then `0xC02B8194[index]` (the string table of the first pass) |
| Stat of the program entry | `0xC0077DF4` (file layer object `0x90C7F680`, vtable `0xC0446054` slot `+0x4C`; directory walk `0xC0096264` / `0xC0095C0C`, a cluster-chain layout, ASSUMED reading) | returns the entry size in `[sp + 0x60]` |
| The error | `0xC006E4B0` `movhs r4, #0xa` | size `0xFFFFFFDC` is `>= 0x2000`, code `0xA`, `0xC02B810C[10] = 4`, `0xC02B8194[4] = 0xC02B7E74` = `Write error` |

The size is garbage because the layer below returns garbage. **That layer is I2C0 (`0x01C22000`) with four EEPROMs at slave addresses `0x50..0x53`**: the boot issues 43,641 transactions with 16-bit addresses spread over `0x0000..0xFFFF` on all four slaves, with 8,192 two-byte address writes per device and 4,100 zero-length ack-polling probes (consistent with a scan of four 64 KB devices in 8-byte units, ASSUMED). The old harness left the I2C registers as plain memory, so the poll breaker fed `0xFFFFFFFF` into every read. The first pass had classified these slaves as boot settings; they hold the program memory (ASSUMED from the layer: the entry keys are program slots; 256 KB is the order of the program banks).

**The model (`tools/arm_i2c.py`).** `I2cBus` implements `ICSTR`, `ICCNT`, `ICSAR`, `ICDXR`, `ICDRR`, `ICMDR` as the driver `0xC005B09C..0xC005B2B0` uses them (`ARDY` `0x04`, `RRDY` `0x08`, `XRDY` `0x10`, `NACK` `0x02`, `BB` `0x1000`; `STT` `0x2000`, `STP` `0x0800`, `MST` `0x0400`, `TRX` `0x0200`); `Eeprom` is a 64 KB device with a 16-bit address pointer, sequential read and 128-byte page writes. A zero-length write completes at once: it is the ack poll the driver waits for in `0xC005B2A0`; without it the boot spins there.

**Result (VERIFIED, `uv run --with unicorn python tools/arm_screens.py boot ...`).** The boot reaches the main loop `0xC00A0248` after 800 M instructions with 90,086 I2C transactions (it scans all four devices, then formats). It writes one header to the blank device 0, bytes `79 B8 F8 99 01 00 00 00` (magic `0x99F8B879`, then `1`), and leaves every other byte `0xFF`. The Program screen shows `A:11` and **`<empty>`**; `Write error` is gone.

**What is not done.** A real program name needs records in the EEPROMs (all 7 sound sections of an `.ns4p`, its name, the entry keys of the directory layer). The record format is not decoded; the `.ns4b` restore image under `.reference/` writes them on a unit. The cheap way to get it is to capture the I2C writes of a Store (Shift + Program ..., Store menu, `Screen::StoreAs`) and replay them as an initial EEPROM image: follow-up 1 (done in section 13).

## 7. Front-panel input (blocker 2, VERIFIED)

**The scan.** The step routine in SRAM `0x800014E8` (ROM twin `0xC024DA6C`; installed as phase hook slot 2 by `0xC024DB74`) runs once per phase. Table `0xC05298D8` has 11 entries of 5 words `{column id, strobe, kind, mask, destination}`; the call strobes column `n`, waits, reads `word = byte[0x60000020] | halfword[0x60000010] << 16`, masks it, stores `~word` and, when it changed, posts `(column id, word)` to a queue (`0x80001D20` -> `0xC0083138` for keys, `0x80001D40` -> `0xC00829E0` for kind 1) before it writes the next strobe to `0x60000015`.

| Strobe | Column id | Kind | Mask | Bits (port `0x10` / `0x20`) |
|---:|---:|---|---|---|
| `0x01` | 0 | key | `0x003F007F` | 6 / 7 |
| `0x02` | 1 | key | `0x00FF007F` | 8 / 7 |
| `0x04`, `0x08`, `0x10` | 2, 3, 4 | key | `0x00FF00FF` | 8 / 8 |
| `0x20` | 5 | key | `0x00FF003F` | 8 / 6 |
| `0x40` | 6 | key | `0x00FF0000` | 8 / 0 |
| `0x80` (4 times per 11-step sweep) | 0 | kind 1 (encoders, not driven yet) | `0x000F003F` | 4 / 6 |

98 key bits. The inversion `~word` means **a pressed key reads 0**: the idle level is 1 (pull-ups). The old harness answered 0 everywhere, i.e. every key held down (ASSUMED polarity from the `mvn`; confirmed by the run: with idle `0xFF` the boot is quiet and a cleared bit produces exactly one screen event). `tools/arm_keys.py` `KeyMatrix` answers `0xFF`, clears the bits of held keys while their column is strobed, and `arm_panel.install_panel_link(emu, keys)` wires it to the reads and to the strobe stores; the presence read of the board bring-up keeps its `0x80` answer.

**Sweep (EMPIRICAL, `tools/arm_screens.py sweep`: every key bit alone, 40 ticks down, 40 ticks up).** Selected results; a key is `(strobe, port, bit)`:

| Key | Result | Note |
|---|---|---|
| `(0x02, 1, 4)` | none alone | **Shift**: with Program 8 it opens `EXTERN SETTINGS 1/13`, which the manual puts at Shift + Program 8 |
| `(0x08, 0, 2)` + Shift | `Program Organize` list | Program 3 |
| `(0x04, 0, 2)` + Shift | `AUX KB SETTINGS 1/3` | Program 4 (manual: Shift + Program 4) |
| `(0x08, 0, 4)` + Shift | `MIDI SETTINGS 1/10` | Program 7 (manual: Shift + Program 7) |
| `(0x04, 0, 4)` + Shift | `EXTERN SETTINGS 1/13` | Program 8 |
| `(0x10, 0, 3)`, `(0x10, 0, 4)`, `(0x10, 0, 5)`, `(0x08, 0, 2)`, `(0x04, 0, 2)`, `(0x08, 0, 4)`, `(0x04, 0, 4)` | location `A:12`, `A:15`, `A:16`, `A:13`, `A:14`, `A:17`, `A:18` | Program buttons; they only act after 40 to 60 ticks down (a 40 tick press shows nothing) |
| `(0x08, 0, 0)` / `(0x08, 0, 1)` / `(0x08, 0, 5)` | `ORGAN PRESET` / `PIANO PRESET` / `SYNTH PRESET` screens | Preset buttons of the three sections |
| `(0x08, 0, 3)` | `SECTION EDIT: Section editing affects all enabled layers` | |
| `(0x01, 0, 3)` | Program home with the section rows | |
| `(0x01, 1, 6)`, `(0x04, 1, 6)` | Synth `OSC WAVEFORM` page on display 1 | |
| `(0x02, 0, 0)`, `(0x02, 0, 1)`, `(0x04, 0, 1)` | `MORPH A.TOUCH / PEDAL / WHEEL ASSIGN` | |
| `(0x01, 0, 0)`, `(0x01, 0, 1)`, `(0x02, 0, 4)`, `(0x04, 0, 5)` | popups `Master Clock Tempo`, `Transpose`, `Octave Shift +1` / `-1` | |
| `(0x01, 1, 0..5)`, `(0x02, 1, 0..3, 5, 6)`, `(0x04, 1, 3, 4)` | effect popups (`Delay Feedback Effect`, `Mod 2 Type`, `Amp Type`, `Reverb Type`, `Compressor`, ...) | |
| `(0x10, 0, 0)`, `(0x20, 0, 3..6)`, `(0x10, 1, 7)` | popups `Keyboard Touch`, `Acoustics`, `Unison`, `Dynamic Compression`, `Equalizer`, `Amp/EQ` | need a longer press |

Not found in the first sweep (40-tick holds): Program 1, 2, 5, 6, the Page and Store keys and every encoder; section 12 finds them with longer holds and Shift staged.

## 8. Captured screens (VERIFIED: `arm_screens.py boot` then `press` / `sweep`)

Frames stay in `target/` (git-ignored, firmware font pixels). What each text widget was given is logged at `0xC007E43C` (after the upper-case pass of the set-text routine `0xC007E3AC`; widget `[+0x54]` text, `[+0x2C]` font, `[+0x88]` upper-case flag), so the strings below are exactly what the OLED draws.

| Screen | Strings the widgets got (font index; `^` = upper-case flag) | Reached by |
|---|---|---|
| Program home, empty program | `A:11` (0) / `<empty>` (0) / rows `As:`, `B3`, `!Piano not found`, `Pure Saw` (3) / drawbar digits `88AA09009` (nine digits, their own font object `0xC02C3BA8`: confirms the nine-digit text form of `section_rows`, bean `ns4-bjvn`) | `(0x01, 0, 3)` |
| Edited marker | the boxed `E` after `A:11` is drawn once the program counts as edited | seen after `(0x01, 0, 2)` and `(0x02, 1, 0)` |
| Parameter popup | label + value, 8 px (3), both mixed case: `Delay Tempo`, `250 ms`, `Octave Shift`, `Reverb Type` / `Hall`, `Master Clock Tempo` / `120 bpm`; a right-aligned graphic (4 boxes for the clock, `+1` slider for the octave) | `(0x01, 1, 2)` ... |
| `ORGAN PRESET` / `PIANO PRESET` / `SYNTH PRESET` | title ^3; `1:01` (5), `<None>`, soft buttons `Num` `Cat` `Cancel`, a letter column `A` `B` (`C` for Synth), row ` <empty>` | `(0x08, 0, 0 / 1 / 5)` |
| `SECTION EDIT` | `SECTION EDIT` ^3, `Section editing affects` / `all enabled layers` (3) | `(0x08, 0, 3)` |
| `MORPH A.TOUCH ASSIGN` | `Change param to morph` (3), `MORPH A.TOUCH ASSIGN` ^3 | `(0x02, 0, 0)` |
| `AUX KB SETTINGS 1/3` | `AUX KB SETTINGS<TAB>1/3` ^3 (the tab right-aligns the page counter), `Aux Keyboard Enable` (3), `Off` (0, 13 px) | Shift + Program 4 |
| `MIDI SETTINGS 1/10` | `MIDI SETTINGS<TAB>1/10` ^3, `Local Control`, `On` | Shift + Program 7 |
| `EXTERN SETTINGS 1/13` | `EXTERN SETTINGS<TAB>1/13` ^3, `Echo Global to Extern Ch`, `Off` | Shift + Program 8 |
| Program Organize | `Program<TAB>Organize` (4, not upper-cased), rows `A:11 <empty>` ... `A:14` (4 + 3), soft buttons `Swap` `Move` | Shift + Program 3 |
| Synth `OSC WAVEFORM` (display 1) | `OSC WAVEFORM` (3, literal capitals), `Pure Saw` (6, 11 px), `Analog` / `Pure` / `3 (7)` (8, 5 px), the three boxed soft labels (glyph codes `@` `B` `C` of the 7 px wide-glyph font `0xC029E01C`, drawn as `TYPE` `CAT` `WAVE`) and the waveform graphic | `(0x01, 1, 6)` |

The first screens after a key press can take 40 to 100 ticks to appear (debounce); `Session.press` runs 20 ticks before and the hold / after ticks around the press. **A latch detail the frames need:** after a snapshot restore a new `PanelLink` has to start from the stored latch byte `0x60000025` and from horizontal addressing, because the firmware skips latch and command stores that its shadow says are redundant; otherwise the data bytes are taken for commands and the frame is garbage (`Session.sync_panels`).

## 9. Real layout against the GUI (VERIFIED measurements, 128 x 64 grid)

Rows measured on the frames (bounding boxes of lit pixels; text boxes are the firmware's, glyph tops are 1 to 3 px lower).

| Element | Real screen | `crates/nord-gui` before | Action |
|---|---|---|---|
| Home title (`A:11`) | 13 px font idx 0, text box y = 3 (glyph rows 3 to 12), x = 1 | `h * 0.19` monospace at y = 1 | fixed: y = 3 |
| Home name line | **13 px, same font as the title**, text box y = 18 (15 px pitch) | `h * 0.16`, smaller than the title | fixed: same size, y = 18 |
| Home rule | 1 px, full width, y = 33 | at about y = 27 | fixed: y = 33 (`oled_paint::grid`) |
| Home rows | 8 px font idx 3, from y = 36, pitch 10, an 8 px icon column at x = 0 to 7 and text from x = 12 | pitch `1.15 * 0.12 h` = 8.8, no icons | pitch fixed: 10; icons: follow-up 3 |
| Section row text | `B3` / `!Piano not found` / `Pure Saw`: the source name (not `Organ B3`), a warning glyph when the sound is missing; the organ row ends with the mini drawbar digits (done in `oled_home::section_rows`, `ns4-bjvn`) | `Organ B3` ... | icons, source names, warning glyph: follow-up 3 |
| Popup | the lower half under the rule: label at y = 45, value at y = 55 (8 px), graphic right-aligned | none: ours replaces the whole screen (`TRANSPOSE`, ...) | follow-up 4 |
| Title bar | inverted: rows 0 to 10 fully lit, text 8 px unlit at y = 2, page counter right-aligned with a tab | plain text line | follow-up 2 |
| Menu body | label 8 px at y = 15, **value 13 px at y = 26** | `1/7 Control Pedal` sub line, rows | follow-up 2 |
| Soft buttons | rows 53 to 63 (11 px), outlined boxes `Num` `Cat` `Cancel`, the active one inverted | filled rectangles in the lowest rows | follow-up 2 |
| Synth page | bar 0 to 10 with the centred title; name 11 px at y = 14; `Analog` 5 px at y = 49 and boxed labels at y = 57 to 63 | similar structure | follow-up 2 |
| Program Organize | bar with the list icon, `Program` left / `Organize` right; selected row inverted (rows 12 to 21), others at 23 / 33 / 43 (pitch 10); soft `Swap` `Move` at 53 | `>A:12  Two` text rows | follow-up 2 |

The **fonts are proportional bitmaps** (glyph advance = width incl. the gap column, section 3), so only the heights above carry over to the monospace egui painter; widths need a proportional font with the metrics of `arm_oled.font_table` (follow-up 5).

## 10. Capitalisation of the 19 case-only strings (VERIFIED where a screen exists)

**The mechanism.** The text widget (`0xC007E3AC`) copies its text into a 0x40 byte buffer and, when the flag `[widget + 0x88]` is set, subtracts 0x20 from every `a..z` (`0xC007E3F4..0xC007E43C`). Title bars set the flag, so the **firmware keeps `Aux Kb Settings`, `Organ Preset` ... in mixed case in its string pool and the display shows capitals**. Popups, labels, values and list rows do not set it (mixed case). A few strings are literally capitals in the pool (`OSC WAVEFORM` `0xC02BE9B4`, `SAMPLE LIBRARY` `0xC02BE9D6`, `EXTERN SETTINGS` `0xC02BEA44`, `SYSTEM SETTINGS` `0xC02C1FEC`) and are drawn without the flag.

| Ours | Firmware string | Real screen | Verdict |
|---|---|---|---|
| `ORGAN PRESET`, `PIANO PRESET`, `SYNTH PRESET` | `Organ Preset` ... | title bar, flag set: capitals | ours right |
| `OSC WAVEFORM` | `OSC WAVEFORM` | capitals, flag clear | ours right |
| `SAMPLE LIBRARY` (x2) | `  SAMPLE LIBRARY` (literal) | not captured; literal capitals | ours right (pool) |
| `PROGRAM ORGANIZE` | `Program Organize` | `Program` left, `Organize` right, mixed case | **wrong, fixed** to `Program Organize` |
| `AUX KB` | `Aux Kb Settings` | `AUX KB SETTINGS 1/3` | **wrong, fixed** |
| `MIDI MENU`, `EXTERN MENU`, `PEDAL MENU`, `SYSTEM MENU`, `SOUND MENU`, `OUTPUT MENU` (not in the 19) | `... Settings` table `0xC02D3584..0xC02D35E4` | `MIDI SETTINGS 1/10`, `EXTERN SETTINGS 1/13` | **wrong, fixed** (`SYSTEM`, `SOUND`, `OUTPUT`, `PEDAL` follow the table: ASSUMED) |
| `UNDO` / `REDO` soft labels, `TRANSPOSE`, `Layer Init`, `FILTER`, `VIBRATO`, `OSC PITCH`, `LEFT` / `RIGHT` (arp), `Reset All Settings`, `G#`, `II` | mixed case in the pool | no screen reached (Undo, Layer Init, the other Synth pages, the Arp page) or the real screen is a popup (`Transpose` mixed case, 8 px) | partly settled in section 14 (Layer Init, Transpose, Filter, Vibrato, Osc Pitch); open: `UNDO`, `LEFT` / `RIGHT`. By analogy the soft labels are mixed case (`Num`, `Cat`, `Cancel`, `Swap`, `Move`), not applied without a screen |

## 11. Remaining blockers (exact, after the third pass)

1. **Program store beyond one program.** One record (the default program, location `A:11`) is stored and renamed in the image. Not done: other locations (the directory entry layout at device 3 `0xFE98` is not decoded), the 824-byte body, a name typed through the Store As screen (Shift + Store stays on the home screen with Memory Protect forced off: the Store As handler has its own gate, not traced) and editing Memory Protect through the System menu instead of the hook at `0xC0150540`.
2. **Encoders and dials.** The five A / B pairs of strobe `0x80` decode to +1 / -1 counters (`0xC0082A9C`, entries `[obj + 0x14] + index * 24`, indices 7, 6, 15, 14, 13), but nothing reads them within 60 ticks, so no menu value, dial value or name character can be edited. The consumer behind `0xC007AEBC` (thresholds `[obj + 0x5C / 0x60 / 0x64]`) and the SPI / ADC dial inputs are not reached.
3. **Undo and Page previous.** Undo is Shift + Solo (manual p.42), but none of the 97 matrix keys reaches the `Nothing to Undo` handler (`0xC0132868`), alone or with Shift staged; the 42 silent keys are probably LED-only buttons. Page previous is silent in every state.
4. **Not captured:** the `Store Disabled` multi-line message (shown through `0xC0080314`, not the text widget), the Arpeggiator page values (glyph codes), Master Clock / Octave Shift popup graphics, the real `LFO` page variants.
5. **Row texts `B3` and `Pure Saw`** of the home screen are set (and drawn in the unit) but absent from the final frame here: ASSUMED to need a loaded sound (no DSP).
6. **Display path and timing:** the widget draw path is still not hand-callable (`0xC004064C`); a tick is 200,000 instructions (ASSUMED 1 ms); popups time out in the firmware (not measured); a held state needs about 30 ticks for the scan to see it.

## 12. Third pass: keys, menus and popups (VERIFIED: sweeps with 100-tick holds and Shift staged)

Bean `ns4-pyqg`. Every key bit was pressed alone and with Shift held (100 ticks down, 60 up; `sweep_plain` / `sweep_shift` of a throw-away script around `arm_screens.Session`, 2 x 97 presses), then the menus were paged with the Page key. A key is `(strobe, port, bit)`.

### 12.1 Keys (VERIFIED, plain unless noted)

| Key | Function | Evidence |
|---|---|---|
| `(0x02, 1, 4)` | Shift | Shift + Program n opens the menus below |
| `(0x10, 0, 2)` / `(0x10, 0, 3)` / `(0x08, 0, 2)` / `(0x04, 0, 2)` | Program 1 / 2 / 3 / 4; with Shift: **System** (`SYSTEM SETTINGS 1/6`), **Sound** (first page `PROGRAM LEVELS 1/11`), Program Organize, `AUX KB SETTINGS 1/3` | location `A:11` .. `A:14`; menu strings |
| `(0x10, 0, 4)` / `(0x10, 0, 5)` / `(0x08, 0, 4)` / `(0x04, 0, 4)` | Program 5 / 6 / 7 / 8; with Shift: **Output** (`OUTPUT SETTINGS 1/6`), **Pedal** (`PEDAL SETTINGS 1/7`), `MIDI SETTINGS 1/10`, `EXTERN SETTINGS 1/13` | same |
| `(0x10, 0, 1)` | **Store**: first press `STORE PROGRAM TO` (`A:11`, `<empty>`, `As:Unnamed`), second press stores and shows the name on the home screen | hooks on the Store handler `0xC01504FC`; no other key reaches it |
| `(0x08, 0, 6)` | **Page** (next): `A:11` to `A:21` on the home screen, next page inside a menu (`1/6` to `2/6` ...); with Shift `B:11` (Bank) | menus paged to the last page |
| `(0x08, 0, 3)` + Shift | **Layer Init**: bar `LAYER INIT` (flag set, pool string `Layer Init`), `Press to initialize`, letters `AB` `A` `A`, soft buttons `All` `Organ` `Piano` `Synth` | frame + widget strings |
| `(0x08, 0, 3)` | Section Edit (`SECTION EDIT`, `Section editing affects` / `all enabled layers`) | as s8 |
| `(0x10, 0, 6)` + Shift | Paste: popup `Nothing to Paste` in the lower half, home header kept | frame |
| `(0x10, 1, 4)` + Shift | Sound Init: popup `Synth` / `Sound Init` (label y = 45, value y = 55) | frame |
| `(0x40, 0, 4)` / `(0x40, 0, 5)` + Shift | Rotary Type (`Rotary Type` / `Close`), `ROTOR STOP ANGLE` bar with `Position` / `Free` | strings |
| `(0x04, 0, 0)` + Shift | `Numeric Pad mode` / `On` popup (plain: `Live 1`) | strings |

**Not found:** Undo (Shift + Solo per the manual p.42): no key of the 97 bits, alone or with Shift held 30 ticks before, reaches the `Nothing to Undo` handler (literal load `0xC0132868`, 2 x 97 presses with a hook there); Page previous (all silent keys also stay silent with Shift); `STORE PROGRAM AS` (Shift + Store with Memory Protect forced off stays on the home screen, the Store As handler has its own gate: not traced). The 42 keys that are silent both alone and with Shift are probably LED-only buttons (Solo, Layer, FX focus), which have no display text.

**Synth keys (display 1, VERIFIED; only after the Synth page `(0x01, 1, 6)` is open, from the home screen they are silent):** `(0x08, 1, 2)` `Filter`, `(0x08, 1, 3)` `Filter Envelope` (with `---` and `0.5 ms` / `1.82 s` / `3.0 ms`), `(0x08, 1, 4)` `Amp Envelope`, `(0x10, 1, 1)` `Osc Envelope`, `(0x10, 1, 5)` `Osc Pitch` (`SEMI` / `CENT`), `(0x10, 1, 6)` `LFO`, `(0x04, 1, 1)` `Arpeggiator`, `(0x04, 1, 2)` `Vibrato` (`5.4 Hz` / `5.0` / `0.7 s`), `(0x10, 1, 0)` `  SAMPLE LIBRARY` (`Sample not found`, `Synth Mode` / `Samples`); popups `Voice Mode` / `Mono` `(0x08, 1, 0)`, `Vibrato` / `On` `(0x08, 1, 1)`, `Unison` `(0x08, 1, 5)`, `Arpeggiator Mode` / `Poly` `(0x04, 1, 0)`, `Keyboard Hold` `(0x20, 1, 2)`, `Arpeggiator/Gate` `(0x20, 1, 3)`, `Octave Shift` `(0x20, 1, 4 / 5)`.

### 12.2 Memory Protect gates the Store (VERIFIED)

The Store handler `0xC0150500` tests the System setting at `0xC0150540` (`cmp r0, #1`). With Memory Protect on (the default, `SYSTEM SETTINGS 1/6` shows `On`) it shows the multi-line message `Store Disabled` / `Memory Protected` / `in System menu` (pool `0xC02C0EC4`, shown by `0xC0080314`, not through the text widget hook) and stops. `tools/arm_store.py` forces r0 = 0 there (the value cannot be edited yet, see 12.4).

### 12.3 Menu pages (VERIFIED, all 56 pages of the seven menus read; widget strings)

Every page has a title bar `<NAME> SETTINGS<TAB>n/N` and **one** item: label (8 px) over its value (13 px), sometimes a second 8 px line (`Not connected`, `Adjust with Program dial`). The last page of three menus is an overview with a different bar title. The page labels of `nord-panel/src/menus` match the real labels nearly everywhere (the model was written from the manual); the differences are in the table of section 14.

| Menu | Pages (label, first value) |
|---|---|
| System 6 | Memory Protect `On`; Global Transpose `None`; Fine Tune `440.0Hz` / `0 cent`; LED Intensity `High`; Reset all settings `to default values`; Version `OS v1.66` |
| Sound 11 | **bar `PROGRAM LEVELS 1/11`**: `Adjust with Program dial` / Volume Level Offset `+0.0 dB`; then `SOUND SETTINGS 2/11` .. `11/11`: Piano Pedal Noise Level, Piano String Res Level, Organ Keyboard Trig `High`, B3 Organ Tone-Wheel `Vintage 1`, B3 Organ Click Level `Normal`, Rotary Bass/Horn level `50/50`, Rotary Rotor Speed / Rotor Acc. / Horn Speed / Horn Acc. `Normal` |
| Output 6 | Output Routing Mode `Global`; Bus Output Channels; Organ / Piano / Synth Output Channel (`12`, `Auto`); bar `OUTPUT OVERVIEW 6/6` |
| Pedal 7 | Control / Swell Pedal Type `Nord EP` + `Not connected`; Sustain Pedal Type `Nord SP-1`; Triple Pedal Function `Una Corda`; Rotor Pedal Type; Foot Switch Type `Single Open`; bar `PEDAL OVERVIEW 7/7` |
| MIDI 10 | Local Control; Global Channel `1` / `16`; Organ / Piano / Synth Channel `Off`; Extern Channel `13` `14` `15`; Control Change Type `CC & NRPN`; Program Change Mode `Send & Receive`; Transpose MIDI at `In`; bar `MIDI OVERVIEW 10/10` |
| Aux KB 3 | Aux Keyboard Enable `Off`; Aux Keyboard Layers `--` `--` `---`; Keyboard Curve `Medium` |
| Extern 13 | Echo Global to Extern Ch `Off`; Keyboard Velocity `Medium`; Program Change MSB / LSB / PC (`--` `0`); MIDI CC #1 / #2 (`2`); ModWheel / Aftertouch / Ctrl Pedal / Swell Pedal / Volume Pedal MIDI `On` (pages 8 to 12: the order the model had **ASSUMED** is now VERIFIED); Send On Load `Off` |

### 12.4 Encoders: decoded, not yet effective (EMPIRICAL)

Strobe `0x80` holds five A / B pairs (port 0 bits 0 to 3, port 1 bits 0 to 5). The decoder `0xC0082A9C` adds +1 / -1 per Gray step to a per-encoder counter (`[entry + 4]`, entry = `[obj + 0x14] + index * 24`; indices 7, 6, 15, 14, 13 for pairs 0 to 4; verified with a hook, `Session.turn` / `arm_keys.encoder_steps`). A state must be held about 30 ticks because the scan reads column `0x80` once per ~10 ticks. **No screen reacted** to any pair, either direction, on the home, Organ preset, Synth and System pages: the counter is not read within 60 ticks (no memory read hook hit), so the consumer (an acceleration / event stage behind `0xC007AEBC`, thresholds at `[obj + 0x5C / 0x60 / 0x64]`) is not reached. The menu values and the Store As name entry therefore stay unedited.

## 13. Program store: record layout and a real Store (VERIFIED, `tools/arm_store.py`)

**Answer to blocker 1.** A Store writes into a RAM write-back cache; the firmware's storage task flushes dirty pages to the I2C EEPROMs. The harness does not schedule that task, so `arm_store.flush` calls its step `0xC0240518` by hand (service object `[+8]` = cache object `0x90048600 + 0x3C`, budgets `[+0x24]` / `[+0x28]`; 60 calls of 16 pages). After Store + Store + flush the EEPROMs hold:

| Where | Bytes | Meaning |
|---|---|---|
| device 0 `0x00..0x07` | `79 B8 F8 99 01 00 00 00` | volume header (as after the format at boot) |
| device 0 `0x40 + 0x80 * i` | `01 00 <i> 00` + 124 bytes | **cluster** i of the record (7 clusters, `i = 0..6`); the payload stream is the concatenation |
| payload `+0` | `5C 03 00 00` | size = 860 = 36-byte header + 824-byte body |
| payload `+4` | `C1 80 D3 ED` | CRC-32 (standard, reflected, init / xorout `0xFFFFFFFF`) of the 824-byte body; verified `zlib.crc32(body) == crc` and against the firmware table `0xC053F494` (equal to the standard table) |
| payload `+8` | name, 24 bytes, NUL padded (`Unnamed`) | the **name is outside the CRC**, so it can be patched in place |
| payload `+32` | `70 34 73 6E` (`p4sn` = `ns4p`), 4 zeros, `11 00 00 00` (location `A:11`) | header tail |
| payload `+44` | 824-byte program body | not decoded (starts `00 00 01 39 2C 21 47 00 2B 42`) |
| device 3 `0xFE98..0xFEB7` | `B6 4A 8A 64 80 00 00 00`, 3 x `FF FF FF FF 80 00 00 00` | directory entries (key hash + flag), one per record; layout not decoded |

The fs layer call is `0xC006E688(fs, key, size = 0x338, ...)` from `0xC01052E8` (`fsWrite`; read twin `0xC006E47C`, directory walk `0xC0096264`); the cache layer is `0xC009EB20` (flush page), `0xC009EC3C`, `0xC009EEBC` (flush step, called by `0xC0240518`), `0xC009F140` (cache write).

**Result.** `python tools/arm_store.py store ... --name "Studio Grand"` stores the default program, renames the record in the image (the CRC is untouched) and writes a 256 KB image under `target/oled/`. `arm_screens.py boot --eeprom <image>` boots with it in 53 s (49,157 I2C transactions instead of 90,086: no format): the home screen shows `A:11` / `Studio Grand`, Program Organize lists `A:11 Studio Grand` and `A:12 <empty>`, and the sections are read from the record (`B3`, `Piano not found` with the warning glyph, `Pure Saw`, drawbar digits `88AA09009`). The image is firmware-derived data and stays local.

**Limits.** One program only (the default one); another location needs the directory entry format (key hash of the device 3 entry) or a Store with the Program key pressed first (not tried); naming by the dial / ABC soft button needs the encoders (12.4); the `B3` and `Pure Saw` row texts are set by the widgets but are not in the final frame (only the icon is drawn: the sound is not loaded without the DSP, ASSUMED), the piano row with the warning glyph is complete.

## 14. Real OLED text against ours (VERIFIED strings, case included)

Layout measurements are in section 9; strings below are the widget texts (`arm_screens.py`), upper-case flag `^` = the title bar upper-cased a mixed-case pool string.

| Screen | Real | Ours before this pass | Ours now |
|---|---|---|---|
| Home name line | `Studio Grand`, 13 px | same | same |
| Home rows | icon column, then `B3` + digits, `!Piano not found` (warning glyph `0x8A`), `Pure Saw` | `Organ B3 ...`, `Piano ...`, `Synth ...` | icon marker + source name, `!Piano not found` when no piano is loaded (`oled_icons`, `section_rows`) |
| Menu bar | `SYSTEM SETTINGS` left, `1/6` right, inverted rows 0 to 10 | `SYSTEM SETTINGS` + sub line `1/6 Memory Protect` | inverted bar with right-aligned counter; sub line is the page title |
| Sound page 1 bar | `PROGRAM LEVELS 1/11` | `SOUND SETTINGS` | `PROGRAM LEVELS` (`menu_screen::bar_title`) |
| Output / Pedal / MIDI last page bar | `OUTPUT OVERVIEW 6/6`, `PEDAL OVERVIEW 7/7`, `MIDI OVERVIEW 10/10` | `... SETTINGS` | the overview titles |
| Menu body | label 8 px (y = 15), value 13 px (y = 26), no marker | `>Label: value` rows | unchanged rows (open, section 15) |
| Soft buttons | outlined boxes rows 53 to 63, the focused one lit (`Type` `Func` `Gain`) | filled boxes | outlined; the focused item lit (`soft_on`) |
| Transpose | popup `Transpose` / `+1 semi` (mixed case, 8 px, y = 45 / 55) under the home header | full screen `TRANSPOSE`, `+1 st on` | popup, `Transpose` / `+1 semi` |
| Master Clock | popup `Master Clock Tempo` + 4 boxes / `120 bpm` | full screen `MST CLK` | unchanged (carries the KBS row; open) |
| Octave Shift | popup `Octave Shift` / `+1` + slider graphic | n/a | open |
| Paste (empty) | popup `Nothing to Paste` | `Nothing to Paste` message | unchanged |
| Layer Init | bar `LAYER INIT` ^, `Press to initialize`, letters `AB` `A` `A`, outlined `All` `Organ` `Piano` `Synth` | same text, no bar | bar added |
| Preset screens | bar `ORGAN PRESET` ^, `1:01` 13 px, `<None>` / row `<empty>`, soft `Num` `Cat` `Cancel` | same text, no bar | bar added |
| Store | bar `STORE PROGRAM TO`, `A:11`, `<empty>`, `As:Unnamed` | `STORE PROGRAM TO` text screen | unchanged (open) |
| Sound pages | label `Volume Level Offset` (ours `Program Levels`), values `+0.0 dB` | page title only | unchanged; label differs: **open** |
| Output pages | `Bus Output Channels`, `Organ Output Channel`, `Piano Output Channel`, `Synth Output Channel` | `Output Bus Channels (Per Program)`, `Organ/Rotary Output Channel/Bus`, ... | unchanged (model labels, **open**) |
| Pedal pages | `Control Pedal Type`, `Swell Pedal Type`, `Sustain Pedal Type`, `Rotor Pedal Type`, `Foot Switch Type` | page titles `Control Pedal` ... with items `Type` `Func` `Gain` | unchanged (**open**) |
| System page 6 | `Version` / `OS v1.66` | `Version and Model Info` | unchanged (**open**) |

**Case-only strings (section 10 list) settled by this pass**

| String | Verdict |
|---|---|
| `Layer Init` | real bar `LAYER INIT` (flag set on a mixed-case pool string): ours right |
| `TRANSPOSE` | real popup is `Transpose`, mixed case: **ours wrong, fixed** (popup) |
| `FILTER`, `FILTER ENVELOPE`, `AMP ENVELOPE` | real Synth titles `Filter`, `Filter Envelope`, `Amp Envelope` (flag clear): **ours wrong, fixed** (`SynthPage::title`) |
| `VIBRATO`, `OSC PITCH`, `OSC ENVELOPE`, `ARPEGGIATOR` | real `Vibrato`, `Osc Pitch`, `Osc Envelope`, `Arpeggiator`: **ours wrong, fixed** |
| `SAMPLE LIBRARY` | real `  SAMPLE LIBRARY`, literal capitals in the pool, drawn without the flag: ours right (now VERIFIED) |
| `Reset All Settings` | real page label is `Reset all settings` (System 5/6): the model already says `Reset all settings` |
| `LFO WAVEFORM` | the real LFO page is titled `LFO`; not changed (the page may be another LFO page): open |
| `UNDO` / `REDO`, `LEFT` / `RIGHT` (arp), `G#`, `II` | still open: Undo key not found; the Arpeggiator page values are glyph codes of the 7 px font, not words |

## Implications

- The GUI's Program home uses the measured grid (`oled_paint::grid`): title y = 3, name y = 18 at the same 13 px size, rule y = 33, rows from y = 36 every 10 px in the 8 px font; section rows now start with an icon column and use the source name (`oled_icons`, `section_rows`).
- Title-bar screens (menus, Preset, Layer Init) are painted as an inverted bar with the page counter right-aligned (`OledText::bar`, `oled_paint::bar`); soft buttons are outlined with the focused one lit; Transpose is a popup in the lower half. The Sound menu starts with `PROGRAM LEVELS`, the last pages of Output / Pedal / MIDI are `... OVERVIEW`.
- Titles are capitals only where the title bar upper-cases the pool string (menus, Preset, Layer Init, Section Edit, Morph assign) or the pool string is literal capitals (`OSC WAVEFORM`, `SAMPLE LIBRARY`); the Synth section pages are mixed case (`Filter`, `Vibrato`, `Osc Pitch`, `Osc Envelope`, `Filter Envelope`, `Amp Envelope`, `Arpeggiator`).
- The program memory is a write-back cache over 256 KB of I2C EEPROM with a cluster-chain file layer; a populated image (`arm_store.py`) is the way to get real names for the Program view, Organize and Store screens.
- The menu pages of `nord-panel/src/menus` match the real labels except the cases listed in section 14; the order of the five Device MIDI entries of the Extern menu (pages 8 to 12) is now VERIFIED.

## Follow-ups

1. Decode the directory entry (device 3 `0xFE98`) and the program body so that several named programs can be stored, and trace the Store As gate for a typed name.
2. Reach the encoder consumer (`0xC007AEBC` and its event stage) to edit menu values, the dial and the Store As name; then re-capture the menu pages with changed values.
3. GUI: menu body as one item per page (8 px label, 13 px value, optional 8 px hint) with the real labels of section 14 (`Volume Level Offset`, `Bus Output Channels`, `Control Pedal Type`, ...); Program Organize list with the inverted row; Master Clock and Octave Shift popups with their graphics (beans `ns4-9nj8`, `ns4-aivy`).
4. GUI: a proportional bitmap font with the metrics of `arm_oled.font_table` (derived metrics only, never the bitmaps).
5. Find Undo (Shift + Solo) and the remaining Synth / Arp pages to settle `UNDO`, `LEFT` / `RIGHT` and the real `LFO` title.

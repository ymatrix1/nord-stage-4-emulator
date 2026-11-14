# Rendering the real OLED screens from the ARM emulator

- **Checked:** 2026-11-14 (fifth pass, bean `ns4-2gdc`; s16)
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
2. **Encoders and dials (answered in section 15: the consumer is the UI event loop; pair 1 is the Program dial).** The five A / B pairs of strobe `0x80` decode to +1 / -1 counters (`0xC0082A9C`, entries `[obj + 0x14] + index * 24`, indices 7, 6, 15, 14, 13), but nothing reads them within 60 ticks, so no menu value, dial value or name character can be edited. The consumer behind `0xC007AEBC` (thresholds `[obj + 0x5C / 0x60 / 0x64]`) and the SPI / ADC dial inputs are not reached.
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
| Store | bar `STORE PROGRAM TO`, `A:11`, `<empty>`, `As:Unnamed` | `STORE PROGRAM TO` text screen | fifth pass: bar, `A:11` / `<empty>` 13 px, `As:Unnamed`; `STORE PROGRAM AS` with the lit cursor cell and `Cat: <None>` (s16.3, `oled_store`) |
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

## 15. Fourth pass: the encoder event chain, the Program dial, Undo and Store As (VERIFIED unless marked)

Bean `ns4-2gdc`. Hooks on a restored `Session`; reproduce with `uv run --with unicorn python tools/arm_events.py turn <CODE> <SRAM> --state target/oled/state.bin --pair 1 --detents 3` (`tools/arm_events.py`).

**Corrected by section 16:** the System menu `Global Transpose` row below ("pair 0 reaches the value widget ... empty container") is wrong: pair 0 is PIANO SELECT and its empty container is the piano list; menu values are edited by the Program dial once the modifier word is cleared (16.1, 16.2); pairs 2 to 4 are the Synth dials (16.4).

### 15.1 Answer to blocker 2 (section 11): the encoders are read

Section 12.4 concluded that nothing reads the counters. That was wrong: the consumer is the UI event loop, reached through an event queue, not through a memory read of the counter.

| Step | Address | What happens |
|---|---|---|
| decode | `0xC0082A9C` | Gray step: counter `[entry + 4]` +-1, then `0xC007AEBC(obj, index, entry, ..)` |
| limiter | `0xC007AEBC` | time check against `0x80001DC0` (thresholds `[obj + 0x5C / 0x60 / 0x64]`), then the listener `[obj + 8]`, vtable slot 0 |
| post | `0xC007B0BC` | writes a 64-byte entry `{tag 0xC053F9CC, source id, encoder index, delta}` to the ring `table[0xC05613F4 + [0x8000C000] * 16]` (task index 3 = UI: ring `0x90022840`, 256 entries, mask `0xFF`); an entry with the same `{tag, source, index}` is merged by adding the deltas (`0xC007B170`) |
| dispatch | `0xC00A0240..0xC00A03FC` | polls head / tail, calls every subscriber of the tag (`blx` at `0xC00A035C`); key events use tag `0xC053F9D0` (`{tag, 0, key id, 1 / 0}`) |
| route | `0xC018933C` | focused screen: widget tree first, then the screen's key table |
| value widget | `0xC0137CC4` | compares the encoder index with `[widget + 0x44]` and `[widget + 0x48]` (`7`, `6` on the menu pages), then calls the bound parameter's step method (vtable slot 6, `0xC00EEEB0`) with the delta |

The five event ids are 7, 6, 15, 14, 13 for pairs 0 to 4 (`arm_events.ENCODER_PAIR_OF_ID`). One detent produces no event unless the next scan sees the following Gray state, so `Session.turn` with 3 detents delivers merged deltas such as `+1, +2`.

### 15.2 What each pair does (VERIFIED by frames and widget strings, 3 / 6 / 8 detents)

| Pair (id) | Screen | Result |
|---|---|---|
| 1 (6) | System menu | pages `1/6` -> `2/6` -> `3/6` (`Global Transpose` / `None`, `Fine Tune` / `440.0Hz` / `  0 cent`): **the Program dial pages the menus** |
| 1 (6) | Organ Preset | the row steps `1:01` -> `1:02` ... (`ORGAN PRESET`, `1:04`); the list wraps |
| 1 (6) | Program home | opens the program list **`Import into A:11`** (see 15.3) and scrolls it: `A:11`, `A:12` ... `A:18`, `A:21` ... |
| 3 (14) | Organ Preset | re-sets the title only (`ORGAN PRESET`); no visible change |
| 0 (7) | System menu | reaches the value widget (ids `7` / `6` bound) but the step method returns 0 at `0xC00EEF58`: the container `[param + 0x34]` (begin == end, capacity `0x1E0`, built once at boot by `0xC00ED984`) is empty, so the current value is "not found". No menu value changes. ASSUMED cause: the parameter store behind the value (System settings) is filled by the DSP / SPI side that the harness does not run |
| 2, 4 (15, 13) | every screen tried | dispatched, no widget matches: no effect |

Not reached: dial pages of the Synth section (the knobs are ADC inputs, not encoders), name entry (Store As), menu value editing. The Store As name entry and the Memory Protect value therefore remain unedited; `arm_store` still forces Memory Protect off.

### 15.3 New screen: the program list (VERIFIED, `Session.turn(1, 3)` on the home screen)

Widget strings (font index `0xC029E07C` for the title and the location, `0xC029E5F0` for `<empty>`): `Import into A:11`, then one `A:nn` + `<empty>` pair per row. Frame of the last position (rows measured on the 128 x 64 grid): a list icon and the title `Import into A:11` in the top row (not an inverted bar), four rows of `A:38 <empty>` ... with the selected row inverted (full width, 10 px pitch), soft buttons `Ok` (left) and `Cancel` (right) as outlined boxes in rows 53 to 63. Display 1 is blank (all pixels off) while the list is open. (Program Organize, section 9, uses a different bar: list icon, `Program` left / `Organize` right.)

### 15.4 Undo (blocker 3): the dialog is found, its trigger is not

`Nothing to Undo` belongs to a dialog class whose pool strings are `Undo to before` / `%s?` / `Nothing to Undo` / `Ok` / `Redo` (`0xC02BFB28..0xC02BFB5C`). Its refresh method is `0xC01327BC` (vtable `0xC02D49B4`, slot 1): it tests the byte `[[this + 0x60] + 0x120]` (history present) and sets the three widgets to `Nothing to Undo` / `Ok` or the `Undo to before <item>?` form. No literal pool word, `add pc` form or data word of the 5.4 MB code image points at the vtable region `0xC02D4940..0xC02D49C0` (also not for the vtable of the encoder parameter class `0xC02D41F4`, which is live), so the objects are built through a path that does not name the vtable by address (a copy of the class tables, not traced). The earlier sweep with a hook on the literal load (section 12.1, 97 keys alone and with Shift) did not reach it. A sweep from the Store screen in this pass is **inconclusive** (every key was silent, see 15.6), and no sweep from Program Organize was completed. **Undo remains unreached**; the next step is a watch on writes of `0xC02D49B4 + 4` into the heap while the Program page is open, or tracing the constructor from the soft button `Undo` of Program Organize (`Swap` `Move` `Ok` `Undo`).

### 15.5 Store As (blocker 3 / follow-up 1): the gate is two flags, not Memory Protect

`0xC01504FC` (Store handler) after the Memory Protect test (`0xC0150540`, `cmp r0, #1`; `0xC0150660` shows `Store Disabled` / `Memory Protected` / `in System menu` through `0xC0080314`, `strd r6, r7 = 3, 0x11`) works on the object `[this + 0x120]`: byte `[+5]` ("first press seen") is set and the object is queued on the list `0xC0541CDC` (count word, entries from `+4`); a second press finds `[+5] = 1`, sets `[+6]` and queues it again (ASSUMED: the commit). The `STORE PROGRAM AS` name screen is not behind this handler (no Shift test in it); it belongs to the screen object that handles `As:` (the third row of `STORE PROGRAM TO`) and needs the name encoder, which is not wired (15.2). Not captured.

### 15.6 Display and timing notes

- A frame right after a turn can be a partial update (the controller model shows only the rectangle written, for example a small keyboard icon in the top left): wait about 100 ticks and read `link.frames(d)[-1]`.
- A key sweep that replays the opening keys costs about 25 s per key; a snapshot of an already opened screen (`state_bytes` then `load_state`) did not keep the screen interactive in the Store sweep (every key silent), so scripted sweeps must replay the opening keys.

## 16. Fifth pass: menu value editing, Store As, the Synth dials (VERIFIED unless marked)

Bean `ns4-2gdc`. Reproduce: boot once (`arm_screens.py boot`, section 8), then `uv run --with unicorn python tools/arm_menu_edit.py <CODE> <SRAM> --state target/oled/state.bin` (all seven menus, about 25 s per page), `tools/arm_synth_dials.py` (same arguments). Frames and strings stay under `target/oled/` (git-ignored).

### 16.1 Answer to blocker 1 (s15.2): the menu values were never blocked by a parameter store

The step method `0xC00EEEB0` of s15.2 is **not** the menu value path, and no DSP / SPI side is needed.

| Finding | Evidence |
|---|---|
| Pair 0 (event id 7) is the **PIANO SELECT** dial (manual p.13: Shift + PIANO SELECT opens the LIST view). The widget `0xC0137CC4` (vtable `0xC02D4D60`, ids 7 / 6 at `[w + 0x44 / 0x48]`) steps the piano list view `0x90E2FC60` (vtable `0xC02D41F4`, the only instance) | hook on `0xC0137D5C`: manager `r0 = 0x90E2FC60`, id `{0, 0, 0, 1}` |
| That view sits on a store of 6 x 20 x 4 = 480 cells of 0x40 bytes (`0x90E2FD10`, store name `Piano`); every cell reads `01 "<empty>"` (flag byte at `+0x10`, name at `+0x11`) | memory dump after the boot |
| Its step needs the sorted index vector `[view + 0x34]` (begin / end / capacity `0x1E0` words, built by `0xC00ED984`, **empty**). The rebuild is `0xC00ED3B4` (filter predicate `0xC00E9AD4`: a cell whose flag byte is set joins the view only if view flag bit 1 `[view + 0x30]` is set; the Piano view has `0x3C`, so no `<empty>` cell qualifies). It is reached through vtable slot 5 `0xC00ED4BC`, which the observer thunk `0xC017BC54` runs only when `[0x8000C008] == 6` (another task) | disassembly; write watch on `0x90E37CB0..CC` during the boot: only the constructor writes |
| So the "empty value container" means "no piano library is installed". Setting flag bit 1 and calling `0xC00ED3B4(view, 0)` fills 480 entries and pair 0 then steps the list (`Grand 1/80`, `Grand 2/80` ...) | `Session` experiment (EMPIRICAL, not a tool) |

### 16.2 Menu value editing works with the Program dial (blocker 1)

Chain for one detent (hooks): dial event id 6 -> menu screen handler `0xC014EEC8` -> page container `0xC012DEC0` (forwards `delta` to the item at `[this + 0x200]`) -> item adjust `0xC0134FA0` -> settings adjust `0xC00DFA00` (key `{id, index}` e.g. Memory Protect `{0x34, 0}`; range `min..max` from the table at `[0xC055B0A0 + 0x1C] + id * 4 -> [+0x8CC]`; `new = clamp(current + delta)`) -> set `0xC003B590` -> the item redraws its value text.

**Why it paged in s15.2:** the handler tests bit 0 of the flags word of the event (`[entry + 4]`). The flags word is the *modifier word* `0xC05846AC` (cleared at `0xC007B590`, set at `0xC007B79C`, copied into every encoder event by `0xC007B0BC`). With bit 0 set the dial moves the page / slot selection (`0xC00DE894(slot, +-1)`), with bit 0 clear it edits the value. After **Shift is released** the word reads 1 and stays 1 (`Shift` held: 0; after release: 1; `Shift + Program 1`, release both: 1; EMPIRICAL, the clearing event was not traced), so every dial test of s15 ran in the paging mode. `arm_screens.edit(session, pair, detents)` writes 0 to `0xC05846AC` before turning; the menus open with Shift + Program n as before.

Edited values (strings are the widget texts, three detents up then six down; `tools/arm_menu_edit.py`, full table in `target/oled/menus/menus.json`):

| Menu page | Base | Up | Down |
|---|---|---|---|
| System 1 Memory Protect | `On` | (at the limit) | `Off` |
| System 2 Global Transpose | `None` | `+1 semi`, `+3 semi` | `None`, `-3 semi`, `-6 semi` |
| System 3 Fine Tune | `440.0Hz` + tab + `  0 cent` | `440.3Hz` / ` +1 cent`, `440.8Hz` / ` +3 cent` | `439.2Hz` / ` -3 cent`, `438.2Hz` / ` -7 cent`, `437.0Hz` / `-12 cent` |
| System 4 LED Intensity | `High` | | `Low` |
| System 6 Version | `OS v1.66` | `HW Rev 0.0-00`, `Revision B` | `OS v1.66` |
| Sound 1 Volume Level Offset | `Adjust with Program dial` | | `-3.0 dB`, `-5.0 dB`, `-7.5 dB` |
| Sound 2 / 3 Piano Pedal Noise / String Res Level | ` 0 dB` | `+1 dB`, `+3 dB` | ` 0 dB`, `-3 dB`, `-6 dB` |
| Sound 4 / 5 / 6 | `High` / `Vintage 1` / `Normal` | `Vintage 2`, `High` | `Low`, `Clean`, `Normal` |
| Sound 7 Rotary Bass/Horn level | `50/50` | `40/60`, `30/70` | `60/40`, `70/30` |
| Sound 8 to 11 Rotary speeds / acc. | `Normal` | `High` | `Low` |
| Output 3 to 5 (Organ / Piano / Synth Output Channel) | `12` | `34`, `2` | `1`, `12` |
| Pedal 4 Triple Pedal Function | `Una Corda` | `Panel Assigned`, `Layer Scene Toggle` | `Layer Scene Hold`, `Una Corda` |
| Pedal 5 Rotor Pedal Type | `Nord SP-1` | `Normally Open`, `Normally Closed` | `Half Moon` |
| Pedal 6 Foot Switch Type | `Single Open` | `Single Closed`, `Dual Closed` | `Dual Open` |
| Aux KB 1 / 2 / 3 | `Off` / `--` / `Medium` | `On` / `A-`, `AB` / `Hard` | `-B` / `Soft` |
| Output 1 Output Routing Mode | `Global` | `Per Program` | `Global` |
| MIDI 2 Global Channel | `1` | `2`, `4` | `3`, `1`, `Off` |
| MIDI 3 to 5 Organ / Piano / Synth Channel | `Off` | `1`, `3` | `2`, `Off` |
| MIDI 6 Extern Channel | `13` | `14`, `(14)`, `16` | `10`, `6`, `1` (a bracketed value shares the channel with another section) |
| MIDI 7 / 8 / 9 | `CC & NRPN` / `Send & Receive` / `In` | `Out` (page 9) | `CC` / `Receive`, `Off` |
| Extern 1, 2 | `Off` / `Medium` | `On` / `Hard` | `Soft` |
| Extern 3 to 7 Program Change MSB / LSB / PC, MIDI CC #1 / #2 | `--`..., `2` | `0`, `2` / `3`, `5` | `1`, `--` / `4`, `2`, `Off` |
| Extern 8 to 13 (ModWheel .. Volume Pedal MIDI, Send On Load), MIDI overview 10 | `On` / `Off` | no change seen | no change seen (three detents up, six down; not investigated) |

Steps accelerate with the turn speed (`Fine Tune`: 1, 3, 7, 12, 17, 22, 27 cents for 3 + 6 detents), so a detent count is not a step count. The values show the same layout as s12.3: 8 px label (y = 15), 13 px value (y = 26); two-part values (`440.8Hz` / `+3 cent`) are one string with a tab between them (the second part right-aligned).

### 16.3 STORE PROGRAM AS / STORE PROGRAM TO (blocker 4, frames measured)

With Memory Protect switched off through the menu (16.2, `Off`), **Store pressed while the modifier word has bit 0 set** (a Shift tap before it) shows `STORE PROGRAM AS` while the key is down, and a plain Store release / press shows `STORE PROGRAM TO`. The gate of s15.5 (`Store Disabled` with Memory Protect on) stands. Frames (128 x 64 grid, ink rows):

| Screen | Rows |
|---|---|
| `STORE PROGRAM AS` | inverted bar rows 0 to 10 (title unlit, 8 px); name `Unnamed` in 13 px, glyph ink from y = 19, the **cursor cell** a lit block 9 px wide (x 0 to 8) and 16 px tall (y 17 to 32) with the glyph unlit; `Cat: <None>` in 8 px, ink rows 39 to 45, starting at x = 2; soft buttons `Abc` `Cat` `Del` `Ins` (rows 53 to 63, outlined) |
| `STORE PROGRAM TO` | inverted bar; `A:11` 13 px (ink y = 18); `<empty>` 13 px (ink y = 33); `As:Unnamed` 8 px (ink rows 51 to 57, **no space after the colon**) |
| `Abc` held | a character row in 8 px replaces `Cat:`: `KLMNOPQRSTUVWXYZ01234` with the selected glyph (`U`) underlined; the `Abc` button lit |

The four soft buttons are the Program keys 2 to 5 here: `(0x10, 0, 3)` Abc, `(0x08, 0, 2)` Cat, `(0x04, 0, 2)` Del, `(0x10, 0, 4)` Ins (lit-pixel count per button quarter; the lit state accumulates because the release does not redraw). Name editing itself (dial = character) was not driven further. A new program shows `Cat: <None>`; our `Undefined` is the last entry of the Cat list, not the caption (`oled_store::category_caption`).

### 16.4 Pairs 2 to 4 (blocker 2): the three Synth dials

| Pair | Event id | Widget | Effect on the Synth waveform page (key `(0x01, 1, 6)`, display 1) |
|---|---|---|---|
| 2 | 15 | left dial, soft label `TYPE` | steps the oscillator type: `Wave` / `Analog` / `Sub Osc` ... |
| 3 | 14 | middle dial, `CAT` | steps the category: `Bells/Tines`, `Pure`, `Shape` ... |
| 4 | 13 | right dial, `WAVE` | steps the wave inside the category: `1 (6)`, `3 (7)`; the page title line shows the wave name (`Bell`, `Pure Square`, `Shape Triangle`) |

On every other screen (home, menus, Organ / Piano) these events are dispatched and ignored. The page is `OSC WAVEFORM` centred (not an inverted bar), the wave name in 13 px at the top (`Shape Triangle` + `SHAPE: 0.0`), then three columns `WAVE | BELLS/TINES | 1 (6)` in 8 px above the inverted soft labels `TYPE` / `CAT` / `WAVE` (drawn as small inverted tabs, not outlined boxes), with a wave picture on the right for shape waves. Reproduce: `arm_synth_dials.py`. The other Synth pages (Filter, Envelopes, LFO, Arp) are knob pages: the knobs are ADC inputs, not encoders, and are not driven.

### 16.5 The Program dial on the home screen (the list `Import into A:11`, re-measured)

With the modifier word clear or set, turning the dial on the home screen opens the program list. Settled frame (controller RAM, not a flush snapshot): an **inverted title bar** rows 0 to 10 with a list icon and `Import into A:11` (corrects s15.3: the bar is inverted), four rows in 8 px at y = 13, 23, 33, 43 (10 px pitch) of the form `A:26 <empty>`, the selected row a lit block rows 22 to 31 with unlit text, soft buttons `Ok` (first) and `Cancel` (fourth) outlined in rows 53 to 63. The location steps `A:11` -> `A:12` ... -> `A:18` -> `A:21` ... (one bank of 8 per page, `A:19` / `A:20` do not exist: the digits run 1 to 8). Display 1 is blank. Not in the GUI yet (follow-up).

### 16.6 Undo (blocker 3): status

| Fact | Evidence |
|---|---|
| The dialog **object exists after the boot**: vtable `0xC02D49B4` at `0x90EFD4F0`, registered in the screen table at `ctx + 0xC0` (`ctx = 0x90DF98C0`, vtable `0xC02D3FB4`), entry 16 (`0x90DF99C0`) | heap scan (`Session`, 16M words) |
| Screens are shown by the manager's vtable slot 13 (`0xC00DC8E8`, `this = ctx`, `r1 = &{id, 0}`, `r2 = 1`); the key router is `0xC018ACDC` (modifier flags: bit 1 -> `0xC018A808`, bit 0 -> `0xC018BB08`) | disassembly, Layer Init stack trace `0xC0185370 -> 0xC00F221C -> 0xC018AD28 -> 0xC0189D64 -> 0xC0189324` |
| Calling slot 13 for ids `0x100` to `0x1EF` (state restored each time) shows no `Undo` text and does not reach the refresh method `0xC01327BC` | scan, 240 calls |
| Pressing every matrix key inside Program Organize (modifier word cleared, 96 keys): the Organize soft buttons (Program keys 2 to 5 as in 16.3) do not produce text and no key reaches `0xC01327BC` / the `Nothing to Undo` literal load `0xC0132868`; keys that do respond leave Organize for their own screen | `ex14`-style sweep (EMPIRICAL) |
| The manual (p.42) puts Undo on **Shift + Solo**; Solo is among the 42 matrix bits that never produce text, so it is probably a key of the front panel controllers (SPI), outside the matrix | ASSUMED |

**Undo remains unreached.** Next step: find the id of the Undo screen (`0xC02D49B4` objects carry no id; the id literals sit in the key router tables near `0xC018BB74`: `0x14A`, `0x1B9`, `0x1C6`) and call slot 13 with it, or post a key event (`0xC053F9D0`, `{tag, flags, key id, 1}`) with the Solo key id into the UI ring (`0xC007B5A8..0xC007B5B8` is the post sequence).

## Implications

- The GUI's Program home uses the measured grid (`oled_paint::grid`): title y = 3, name y = 18 at the same 13 px size, rule y = 33, rows from y = 36 every 10 px in the 8 px font; section rows now start with an icon column and use the source name (`oled_icons`, `section_rows`).
- Title-bar screens (menus, Preset, Layer Init) are painted as an inverted bar with the page counter right-aligned (`OledText::bar`, `oled_paint::bar`); soft buttons are outlined with the focused one lit; Transpose is a popup in the lower half. The Sound menu starts with `PROGRAM LEVELS`, the last pages of Output / Pedal / MIDI are `... OVERVIEW`.
- Titles are capitals only where the title bar upper-cases the pool string (menus, Preset, Layer Init, Section Edit, Morph assign) or the pool string is literal capitals (`OSC WAVEFORM`, `SAMPLE LIBRARY`); the Synth section pages are mixed case (`Filter`, `Vibrato`, `Osc Pitch`, `Osc Envelope`, `Filter Envelope`, `Amp Envelope`, `Arpeggiator`).
- The program memory is a write-back cache over 256 KB of I2C EEPROM with a cluster-chain file layer; a populated image (`arm_store.py`) is the way to get real names for the Program view, Organize and Store screens.
- The menu pages of `nord-panel/src/menus` match the real labels except the cases listed in section 14; the order of the five Device MIDI entries of the Extern menu (pages 8 to 12) is now VERIFIED.

- Fourth pass (section 15): encoder events are queued and dispatched to the screens; pair 1 is the Program dial (pages menus, steps presets, opens the program list `Import into A:11`).
- GUI: a single-item menu page shows its value alone in the 13 px font (`OledText::body_big`), as the frames of section 12.3 show.

## Follow-ups

1. Decode the directory entry (device 3 `0xFE98`) and the program body so that several named programs can be stored, and trace the Store As gate for a typed name.
2. Reach the encoder consumer (`0xC007AEBC` and its event stage) to edit menu values, the dial and the Store As name; then re-capture the menu pages with changed values.
3. GUI: menu body as one item per page (8 px label, 13 px value, optional 8 px hint) with the real labels of section 14 (`Volume Level Offset`, `Bus Output Channels`, `Control Pedal Type`, ...); Program Organize list with the inverted row; Master Clock and Octave Shift popups with their graphics (beans `ns4-9nj8`, `ns4-aivy`).
4. GUI: a proportional bitmap font with the metrics of `arm_oled.font_table` (derived metrics only, never the bitmaps).
5. Find Undo (Shift + Solo) and the remaining Synth / Arp pages to settle `UNDO`, `LEFT` / `RIGHT` and the real `LFO` title.
6. Fifth pass (done): menu values edited (the Program dial with the modifier word cleared, s16.2), Store As frame measured (s16.3), the three Synth dials identified (s16.4).
7. Undo (Shift + Solo): find the Undo screen id for the manager's slot 13 or the Solo key id for a posted key event (s16.6); drive name entry in Store As (dial = character); find what clears bit 0 of the modifier word `0xC05846AC` after a Shift release.
8. GUI: the program list `Import into A:11` (s16.5), the Synth `OSC WAVEFORM` page with the TYPE / CAT / WAVE columns (s16.4), two-part menu values (`440.8Hz` / `+3 cent`), the Master Clock / Octave Shift popup graphics.

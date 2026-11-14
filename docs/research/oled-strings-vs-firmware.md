# OLED and menu text against the firmware's own strings

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66 ARM0 image, load base `0xC0000000`, string offsets below are file offsets), `tools/ns4_oled_strings.py` (string runs, pointer tables, matcher), `tools/arm_display_probe.py` on the ARM harness (`tools/arm_steady.py`, `tools/arm_boot_full.py`, `unicorn` 2.1), `tools/arm_xref.py` (`literal`, `dis`, `mmio`). Prior notes: [arm-fw-emulation](arm-fw-emulation.md), [arm-organ-messages](arm-organ-messages.md) s9-10, [arm-edit-params.csv](arm-edit-params.csv). Derived data (our strings, the matching firmware wording and its offset, no other firmware bytes): [data/oled-strings-vs-firmware.csv](data/oled-strings-vs-firmware.csv).
- **Question:** the text our Program / Synth OLED, the menus and the type / category tables show was written from the manual and videos; the firmware holds the real strings. Which of ours match, which differ, which are absent, and can the ARM emulator render the real screens?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Does the image hold the user-visible text? | Yes: 14,994 NUL-terminated ASCII runs, 9,136 of them text; menu titles, item labels, enum choices, messages, program categories, waveform names, 128-step value strings (section 2) | VERIFIED (`ns4_oled_strings.py dump`) |
| Our menu / OLED / table text vs the firmware | 560 distinct literals on those three surfaces: **232 exact, 46 differ only by case, 26 only by spacing / punctuation, 115 are longer or shorter forms of a firmware string, 17 are format strings whose fixed words all occur, 124 not found** (section 3) | VERIFIED (`ns4_oled_strings.py check`) |
| Fixed in this PR | 37 strings where the firmware wording is unambiguous (section 4), plus the 26 program categories (a table we had invented) | VERIFIED (tests, section 4) |
| Render the real screens from the ARM emulator | **Not feasible today.** The boot reaches the main loop, but the display path is stubbed (text measure, refresh queue), no store to any display-capable peripheral happens in 100 M steady-state instructions, and the panel link is not identified (section 5) | VERIFIED (probe) |

## 2. The firmware string pool

`python tools/ns4_oled_strings.py dump target/fw_unpacked/bank0_CODE.bin` lists the runs. The text sits in a few compact clusters (file offsets; counts are runs that look like UI text):

| Offsets | Runs | Content | Class |
|---|---:|---|---|
| `0x283698..0x283A38` | 83 | layer names (`Piano`, `Synth`, `Live`, `Fx Mod1`, `Morph`), keypad letters, `Pasted`, `Swapped`, `Nothing to Paste`, `%c Memory Protected`, `Nothing to Swap with`, category words (`Accordion/Harm`, `Tonewheel`, `Loops`) | messages, labels |
| `0x2BE9AC..0x2BF3A8` | 270 | `EXTERN`, the 128-step time strings (`399 ms` .. `38 s`, `Inf`), `Osc Envelope`, the sample category names (full and short), `Amount:` | enum / value strings |
| `0x2BF3C8..0x2BF494` | 52 pointers | **program category table**: 26 `(full, short)` pointer pairs | enum names |
| `0x2BFAD8..0x2C01A4` | 74 | `Global`, `Aux KB`, `Redo`, `Nothing to Undo`, `Sample Library`, `Osc Waveform Wave`, pedal faults, `Tempo (MIDI)`, `LAYER INIT`, `Layer Init Organ AB`, `OS v%u.%02u`, speaker positions | titles, messages |
| `0x2C0700..0x2C0F14` | 72 | `KEYBOARD SPLIT`, `Note`, `XFade`, `Section Edit`, Master Clock subdivisions `1/2 .. 1/64`, `Reset all settings`, `Are you sure?`, `Cat:`, `As:`, `StoreName` | titles, labels |
| `0x2C1EE0..0x2C28E0` | 173 | **the System / Sound / Output / Pedal / MIDI menus**: titles, labels, choices, short forms (`Rotor Hld`, `Sust+Rtr.H`) | menu |
| `0x2C7574..0x2C761C` | 17 | `Please wait...`, `Cancel`, preset kinds (`B3`, `Vox`, `Farfisa`, `Pipe1`, `B3 Bass`) | messages |
| `0x2CA110..0x2CA2A0` | 31 | format strings (`%d bpm`, `%.1f dB`), `Piano not found`, `Sample not found`, `Level Organ A`, `Pan Synth A` | messages |
| `0x2CAE08..0x2CB21C` | 128 | `Octave Shift`, `Filter Resonance`, the 128-step `Hz` / `kHz` strings | value strings |
| `0x2CC364..0x2CC978` | 109 | effect names and variations (`Auto-Pan Soft`, `Twin Type I`, `Hall Chorale`, `Space 50 Wet`), `Low-pass`, `High-pass`, `Band-pass` | enum names |
| `0x2CD498..0x2CDD34` | 213 | synth / organ / piano parameter captions (`Arpeggiator Rate`, `Voice Priority`, `Keyboard Touch`, `Soft+Brill`, `Light/Slow`), 0.1 s steps, 0.03..30 Hz rates | labels, enum names |
| `0x2D3310..0x2D381C` | 75 | store targets (`Synth Preset`, `StoreSynth`), `System Settings`, `Sound Settings`, `Aux Kb Settings`, `Layer Organ A`, `Program Organize` | titles |
| `0x2D7D44..0x2D80D8` | 91 | **waveform names** (`Pure`, `Sine`, `Sub Square Saw`, `Saw 8ve 5th+`, `Chop Saw 1`, `FM-H`, `Harmonic %c`, `Bells/Tines`, `Flute 1`, `2nd Spectra`, `Jimmy Smith`, `EPiano 1`, `Clavinet 3`) | enum names |
| `0x2D3959..0x2D7C94` | many | `Edt_*` edit-parameter names, class and archive names (`CArchSynth snapshot`) | internal, not display |
| `0x444BC4..`, `0x5263BC..` | | piano types (`Grand`, `Upright`, `Electric`, `Clav`, `Digital`, `Misc`) and the sample-library categories (`Piano Acoustic`, `Wind/Brass Solo`, `Synth Pad`, `Seq/Arp`), outside the menu pool | enum names |

Three groups that are **not** in the pool as text: the filter type names (`LP M`, `LP 12`, `LP+HP`; only `LP24`, `HP24` of the delay filter and `Low-pass` / `High-pass` / `Band-pass` exist), the LFO shape names (`Sawtooth 1`, `Sample&Hold`) and the arpeggiator direction names (`UP`, `RANDOM`): the Synth page draws them as symbols, so there is nothing to compare (VERIFIED absent, `ns4_oled_strings.py check`).

## 3. Check of our text

`python tools/ns4_oled_strings.py check target/fw_unpacked/bank0_CODE.bin --csv docs/research/data/oled-strings-vs-firmware.csv` takes every non-test string literal of `nord-panel/src` (menus, inventory), `nord-gui/src/panel` and `nord-plugin/src/fx_selectors.rs`, drops ids, paths and prose, and looks each up (exact, case, spacing, word-boundary affix, fixed words of a format string). Surfaces: `menu` (menu pages), `oled` (Program / Synth OLED composers, store, list, overlays), `table` (type, category, waveform, effect name tables), `gui` (hover text, widget captions) and `legend` (panel silkscreen, control inventory: these are printed on the panel, not on the screen, so a difference is not an error).

Distinct literals per surface, after the fixes of section 4:

| Surface | Exact | Case only | Spacing only | Affix | Format | Not found | Total |
|---|---:|---:|---:|---:|---:|---:|---:|
| menu | 116 | 5 | 4 | 59 | 0 | 17 | 201 |
| oled | 58 | 19 | 18 | 20 | 17 | 71 | 203 |
| table | 58 | 22 | 4 | 36 | 0 | 36 | 156 |
| **menu + oled + table** | **232** | **46** | **26** | **115** | **17** | **124** | **560** |
| before the fixes (`origin/main`) | 178 | 55 | 28 | 132 | 17 | 133 | 543 |
| gui (information only) | 14 | 28 | 8 | 53 | 8 | 60 | 171 |
| legend (information only) | 12 | 32 | 5 | 108 | 6 | 192 | 355 |

1 to 3 character strings (`A`, `On`, `HP`) match by chance; the CSV marks them `weak` (`weak = 1`) and the strong-only counts are exact 187, case 38, spacing 8, affix 115, format 15, not found 83.

What the groups mean:

- **Exact (232):** the manual-derived menu text was already right for most titles and choices (`Memory Protect`, `Global Transpose`, `Aux Keyboard Enable`, `Output Routing Mode`, `Nord SP-1`, `Normally Open`, `Single Open`, `Triple Pedal Function`, ...).
- **Affix (115):** our longer descriptive labels (`Organ output (Global)`, `Piano A bus (Per Program)`, `Send On Load A/B/C`, `Fine Tune (cents)` before the fix) and the short forms of effect names (`Phaser` for `Phaser Type I`). The firmware menus are structured differently (one `Organ Output Channel` page whose value is `Global` or `Per Program`), so these are not wording errors; the structure is the open part (follow-up 3).
- **Not found (124):** our own composition text (`STORE PROGRAM AS`, `PAGE picks the layer`, `Org: B3`), progress and error text we invented (`Not supported yet`, `Preset store: not supported yet`), Rust format strings, panel button legends, and names that the firmware draws as symbols (section 2).

## 4. What was changed

All changes are firmware wording that is unambiguous: the same item exists in the firmware in the same menu list, and ours differs only in case, spacing, a unit suffix or an abbreviation. Each target string is asserted to occur in the image by `tools/tests/test_ns4_oled_strings.py::test_fixed_wording_is_in_the_firmware` (skipped without the firmware) and in Rust by `menus/tests.rs`, `oled_tests.rs`, `tables.rs` and `name.rs` tests.

| Where | Ours before | Firmware wording | Offset |
|---|---|---|---|
| Output page title | `Output Overview` | `OUTPUT OVERVIEW` | `0x2C22D4` |
| Pedal page title | `Pedal Overview` | `PEDAL OVERVIEW` | `0x2C2680` |
| MIDI page title | `MIDI Overview` | `MIDI OVERVIEW` | `0x2C27BC` |
| System page 5 | `Reset All Settings` | `Reset all settings` | `0x2C0E08` |
| MIDI page | `Transpose MIDI At` | `Transpose MIDI at` | `0x2C27A8` |
| Extern pages 1, 6, 7 | `Echo Global To Extern Ch`, `MIDI CC#1`, `MIDI CC#2` | `Echo Global to Extern Ch`, `MIDI CC #1`, `MIDI CC #2` | `0x2C27CC`, `0x2C2838`, `0x2C2844` |
| Extern pages 8..12 | `Device MIDI: Mod Wheel` / `Aftertouch` / `Control Pedal` / `Swell Pedal` / `Volume Pedal` | `ModWheel MIDI`, `Aftertouch MIDI`, `Ctrl Pedal MIDI`, `Swell Pedal MIDI`, `Volume Pedal MIDI` | `0x2C2850..0x2C2894` |
| Pedal page 6 labels | `SW A`, `SW B` | `Sw A`, `Sw B` | `0x2C2670`, `0x2C2678` |
| Expression pedal types | `Roland`, `Yamaha`, `Fatar` | `Roland EV7`, `Yamaha FC7`, `Fatar SL` | `0x2C22F0`, `0x2C22FC`, `0x2C2310` |
| Control pedal function | `Ctrl + Swell` | `Control+Swell` | `0x2C2350` |
| Sustain right pedal | `Sus+Rotor Hold`, `Sus+Rotor Toggle` | `Sustain+Rotor Hold`, `Sustain+Rotor Toggle` | `0x2C2530`, `0x2C2544` |
| Left / middle / foot switch functions | `Mst Clk Tap` | `MstClock Tap` | `0x2C2474` |
| Sound page 1 | `Program Levels (dB)` | `Program Levels` | `0x2C200C` |
| Sound pages 2, 3 | `Piano Pedal Noise Level (dB)`, `Piano String Res Level (dB)` | `Piano Pedal Noise Level`, `Piano String Res Level` | `0x2C201C`, `0x2C2034` |
| Sound page 4 | `Organ Keyboard Trigger Point` | `Organ Keyboard Trig` | `0x2C204C` |
| Sound page 5 | `B3 Organ Tonewheel Mode` | `B3 Organ Tone-Wheel` | `0x2C2080` |
| Sound pages 7, 9, 11 | `Rotary Bass/Horn Level`, `Rotary Rotor Acceleration`, `Rotary Horn Acceleration` | `Rotary Bass/Horn level`, `Rotary Rotor Acc.`, `Rotary Horn Acc.` | `0x2C2100`, `0x2C212C`, `0x2C2154` |
| System page 3 | `Fine Tune (cents)` | `Fine Tune` (the value prints as `%d cent`, `0x2CA120`) | `0x2C1FD0` |
| Undo screen | `Nothing to undo` | `Nothing to Undo` | `0x2BFB3C` |
| Paste with an empty clipboard | `Nothing copied` | `Nothing to Paste` | `0x28377D` |
| Store blocked by Memory Protect | `Memory Protect is on` | `Memory Protected` (the firmware string is `%c Memory Protected`, `0x2837A0`; what `%c` prints is not decoded, so the prefix is omitted) | `0x2837A0` |
| Waveform names, SUB OSC | `Square Sub Saw`, `Pulse Sub Saw`, `Sine Sub Saw` | `Sub Square Saw`, `Sub Pulse Saw`, `Sub Sine Saw` | `0x2D7DEC`, `0x2D7DFC`, `0x2D7E0C` |
| **Program categories** (`bay_program/name.rs`) | 16 invented names, `None` first (ASSUMED) | the firmware's 26 categories with their short names, `Undefined` last and the default (below) | `0x2BF3C8` |

The unit suffixes were dropped because the firmware prints values with its own formats (`%+.1lf dB` at `0x2C0DE0`, `%d cent`, `%d semi`).

**Program categories (VERIFIED table, ASSUMED use).** The pointer table `0xC02BF3C8` holds 26 `(full, short)` pairs, read with `python tools/ns4_oled_strings.py pointers target/fw_unpacked/bank0_CODE.bin 0x2BF3C8 52`: `Accordion/Harm` (`Accord/Harm`), `Bass`, `Brass Ensemble` (`Brass Ens`), `Brass Solo`, `Choir`, `Drums`, `Effects`, `Guitar/Plucked` (`Gtr/Pluck`), `Mellotron`, `Misc`, `Orchestral`, `Organ`, `Percussion Tuned` (`Perc Tuned`), `Piano`, `Rhythmic` (`Rhythm`), `Strings Analog` (`Strings Ana`), `Strings Ensemble` (`Strings Ens`), `Strings Solo`, `Synth Bass`, `Synth Classic`, `Synth Lead`, `Synth Pad`, `Wind Ensemble` (`Wind Ens`), `Wind Solo`, `User`, `Undefined` (`Undef`). The getter `0xC017B4C8(this, i, short)` returns `table[2 i + short]` (disassembly, one `ldr`/`add`/`ldr` sequence); a second list of the full names sits at `0x53E014`. That this is the table behind the STORE `Cat:` field (`0x2C0E7C`) and the LIST Cat view is **ASSUMED**: the getter is a virtual method of a list model (used at `0xC0133F58`), and the store screen at `0xC014D4EC` was not traced to it. Because the old list was itself ASSUMED and the new one is real firmware data with the catch-all `Undefined`, the editor now starts on `Undefined` instead of index 0; The short names are in the pointer table (CSV rows `0x2bf3cc`, ...) for a narrow LIST column; no code uses them yet.

## 5. Can the ARM emulator render the real screens? (VERIFIED probe, verdict: not yet)

`uv run --with unicorn python tools/arm_display_probe.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin` boots to the main loop (`0xC00A0248`, 433 M instructions, about 55 s), logs every store to the peripheral windows, counts the UI stubs of `arm_boot_full.install_ui_stubs`, then runs 100 M instructions with the 1 kHz timer.

| Observation | Result |
|---|---|
| Boot reaches the main loop | yes, same stop as [arm-organ-messages](arm-organ-messages.md) s10.1 |
| Stores during the boot, per window | EMIFA CS3 (DSP window `0x6200....`) 242,132; I2C0 228,432; GPIO 64,645; SPI1 1,792; SPI0 15; UART2 10; EMIFA CS2 (`0x6000....`) 4 |
| I2C0 | 43,641 transactions to slaves `0x50` (11,088), `0x51`, `0x52`, `0x53`: four 24xx-type EEPROM addresses (ASSUMED from the slave addresses and the 2-byte address + data pattern), not a display |
| GPIO | 30,733 + 30,507 stores of the set / clear registers by `0xC024B0C4..0xC024B0E0` in a bit-banged loop during the boot (ASSUMED: a serial configuration bit stream; not a display) |
| SPI1 / SPI0 | SPI1 carries the DSP peek / boot link (`0xC0066E64`, [arm-dsp-protocol](arm-dsp-protocol.md)); SPI0 only configuration |
| UART2 | MIDI input, 10 stores; UART1 none |
| Steady state, 100 M instructions after the main loop | EMIFA CS3 43,403 (DSP words), SPI1 2,356, GPIO 264, I2C0 61; **no store to EMIFA CS2 (the panel scan window), no UART1, no SPI0** |
| UI stubs in the harness | `TEXT_MEASURE` (`0xC001B9D0`, text width over the glyph table `[font + 8]`) called 714 times and `EVENT_INSERT` (`0xC001B67C`, the timer queue of the display refresh) 456 times during the boot, both replaced by "return 0" |
| Widgets exist | the boot builds named screens (`program_screen_base` `0x2C848C`, `MenuParameterScreen` `0x2BFC1C`, `ScreenSynBase`, `SynWaveScreen`, `ExternScreen`) and one `0xA0`-byte widget per drawbar ([arm-organ-messages](arm-organ-messages.md) s6) |

What blocks a real render:

1. **The pixel path is not identified.** No display-capable peripheral sees traffic at the steady state, and the three candidates are excluded by the probe (I2C is EEPROM-like, SPI1 is the DSP link, GPIO is a boot-time bit stream). The OLED is on the panel boards (`mainboard-photo-analysis.md`: "Panel 1 / Panel 2 ... OLED"); the link to them is most likely the EMIFA CS2 window (`0x6000....`; `0xC024DA6C` strobes `0x60000015` and reads `0x60000010` in an 11-row loop, a button / encoder scan, ASSUMED) or a link the harness does not drive. Nothing in the harness answers a panel, so the screen task that would push pixels never runs.
2. **The draw code is stubbed.** `TEXT_MEASURE` returns 0 and `EVENT_INSERT` drops the refresh events, because the font object (`[widget + 0x2C]`, glyph table `[font + 8]`, 8-byte entries read as `(data, advance)`) is null: the harness has no display driver object. The font data itself is in the image (the routine indexes `table[(c - first) * 8]`), but the font-descriptor layout is not decoded, so layout cannot be reproduced offline either.
3. **No input reaches the UI.** Selecting Prog View or a menu page needs a panel event through the same unmodelled link (the `Edt_*` message injection of [arm-edit-params.csv](arm-edit-params.csv) changes parameters, not screens).

Smallest next experiment (follow-up 1): remove the two stubs, give the widget a font object built from the image, find the draw primitive that fills a frame buffer (look for the code that reads the glyph rows and a `256 x 64` or `128 x 64` byte buffer; a memory-write hook on the largest SDRAM buffer written per refresh finds it), and call the screen redraw of `program_screen_base` from the main loop. No screenshot was captured: there is nothing to compare for fonts or row structure yet, so the layout of our OLED stays manual / video based. The strings of section 2 give the text of each screen, not its geometry.

## 6. Limits

- The matcher compares text, not behaviour: it cannot say which string a screen shows in which state, nor whether a title is drawn in capitals by the renderer (section 7 case list).
- `affix` is deliberately loose; every row of that class needs a human decision, and only unambiguous ones were changed.
- The ARM boot, I2C and GPIO readings are measurements of the emulator under the harness stubs, not of the device.

## 8. Messages adopted (bean `ns4-0u5u`)

- Strings and offsets: [visual-style-and-panel.md](../specs/visual-style-and-panel.md) section 4.5.1; code `crates/nord-gui/src/panel/bay_program/fw_messages.rs`; check `test_message_strings_are_in_the_firmware` (VERIFIED against `bank0_CODE.bin`, 10 strings).
- **`%c` decoded (VERIFIED):** the firmware strings are `Unable to Paste\n%c Memory Protected` (`0x283790`) and `Unable to Swap\n%c Memory Protected` (`0x2837FC`); both callers (`0xC003D33C`, `0xC003D324`) load `r3 = 0x89` as the `%c` argument before `bl 0xC0257AD4`. `0x89` is a font glyph code, not a letter. The glyph shape is not decoded (ASSUMED padlock); the emulator draws `Memory Protected` without it.
- Triggers are emulator-side: which firmware event raises `Piano not found` / `Sample not found` / `Please wait...` and the toast lifetime are `ASSUMED` (they need the ARM display path, `ns4-2gdc`).

## Implications

- 37 menu / OLED strings now carry the firmware wording; the menu tests pin them and the local check keeps them from drifting (`test_menu_surface_has_no_unexpected_case_or_missing_titles`).
- The program category list is real firmware data; a store flow can offer the 26 categories (the short names for a narrow LIST column are in the pointer table).
- The firmware has messages we did not show (`Piano not found`, `Sample not found`, `Please wait...`, `Pasted`, `Swapped`, `Nothing to Swap with`, `Are you sure?`). Bean `ns4-0u5u` adopted these (section 8); `Single Layer`, `Change param to morph`, `Clear Morph %s` stay open.
- Real screen geometry cannot come from the emulator yet; the OLED layout stays `ASSUMED` (manual p.37-46 and the demo videos).

## Follow-ups

1. Find the panel / display link: trace the stores of `0xC024DA6C` (EMIFA CS2) and the writers of `0x60000070`, un-stub `TEXT_MEASURE` and `EVENT_INSERT`, decode the font descriptor, hook the frame buffer, call the `program_screen_base` redraw; capture the Program view and one menu page as PBM and compare rows and fonts with `bay_program/oled_paint.rs` (bean `ns4-2gdc`).
2. Confirm whether titles are drawn in capitals by the renderer: 19 `oled` literals differ from the firmware only by case (`SAMPLE LIBRARY` / `Sample Library`, `OSC PITCH` / `Osc Pitch`, `UNDO` / `Undo`, `LAYER INIT` / `Layer Init`), while menu overview titles are capitals in the firmware itself; needs the real render (bean `ns4-2gdc`) or a video frame.
3. (messages done in `ns4-0u5u`, section 8) Adopt the page structure of the Output and MIDI menus (`Organ Output Channel` with `Global` / `Per Program`, `Rotary Output Channel`, `Output Routing Mode`, `Bus Output Channels`) in place of our descriptive per-item labels (bean `ns4-0u5u`).
4. ~~Decode which wave belongs to which category~~ Done in [synth-wave-categories](synth-wave-categories.md): the builder `0xC01ADBC0` gives 8 ANALOG categories (45 waves, `Chop *` and `Sine`..`Pulse 10` belong to the new `SYNC`), 5 WAVE categories (46) and the FM groups; our tables use them (bean `ns4-zoep`).
5. Trace the STORE `Cat:` field: done in part ([synth-wave-categories](synth-wave-categories.md) s5): `0xC014D4EC` is the caption, the value comes from a model whose class is open (OQ26); the link of section 4 stays ASSUMED.

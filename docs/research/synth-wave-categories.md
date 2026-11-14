# Synth waveform categories and the STORE `Cat:` field

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66 ARM0 image, load base `0xC0000000`), `tools/ns4_wave_cats.py` (new: symbolic walk of the oscillator-list builder, needs `capstone`), `tools/arm_xref.py dis`, `tools/ns4_oled_strings.py`, `factory_programs/` (384 `.ns4p` headers), the user manual v1.6X p.9 / p.29-31 / p.41-43. Committed derived data: [data/synth-wave-categories.csv](data/synth-wave-categories.csv) (91 rows: type, category, index, name, wave id; no other firmware bytes), cross-checked against [data/synth-wave-types.csv](data/synth-wave-types.csv). Prior notes: [oled-strings-vs-firmware](oled-strings-vs-firmware.md) s2, s4, follow-ups 4 and 5; [synth-wave-stages](synth-wave-stages.md).
- **Question:** which waveforms belong to which ANALOG / WAVE category on the Synth OLED, what are the FM category names, and does the STORE `Cat:` value come from the program's category field or is it derived?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured or read from the manual, **ASSUMED** = reading.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Is category membership a table? | No. One straight-line routine, `0xC01ADBC0` (code `0x1ADBC0..0x1AEB38`, literal pools `0x1AEB3C..0x1AEC60` and `0x1B07F8..0x1B0878`), appends one 20-byte node per entry to a list. The pooled strings are in code order, so the string pool order (`0x2D7D44..`) is not the membership | VERIFIED (`ns4_wave_cats.py`) |
| ANALOG tree | 8 categories, 45 waves: `Pure` 7, `Sub Osc` 3, **`Sync` 9**, `Shape` 5, `Shape Sine` 7, `Multi` 6, `Super` 5, `Misc` 3. The category index equals the `osc_category` field of the Synth sound block (`0..7`) | VERIFIED (the node carries it at `[sp+0x4C]`; ids 45 of 45 equal the DSP P-table, section 3) |
| WAVE tree | 5 categories, 46 waves: `Bells/Tines` 6, `Acoustic` 6, `Digital` 14, `Organ` 10, `Keys` 10 (sizes equal `WAVE_CATEGORY_SIZES`, `cbin/synth_osc.rs`) | VERIFIED |
| FM tree | FM-H: 5 groups `Harmonic A..E`, 25 items (partial `0.5, 1 .. 24`); FM-I: 5 groups `Inharmonic A..E`, 61 items (`%d` of `i - 12`, `-12 .. 48`) | VERIFIED (`ns4_wave_cats.py --fm`: loop bounds are `cmp` immediates, ratios are the double table at `0x2D82A8`) |
| STORE `Cat:` source | `0xC014D4EC` only sets the **caption** `Cat:`; the **value** is a virtual call on a model object whose class was not resolved. The 26-name category table is the name getter of a list-model class; the program header category id is a different numbering | caption VERIFIED, value source **open** (section 5) |

## 2. The builder routine

`0xC01ADBC0` (prologue `0xC01ADBC0..0xC01ADC50` zeroes list heads) runs straight line except for two loops. For each entry it does `obj = new(0x28); ctor(obj, id)`, then `node = new(0x14)` and `append(list, &key, &stack_words)` with `node`:

| Word | Content |
|---|---|
| `[node+0x00]` | callback pointer (`0xC02D80E0..0xC02D8240`, 16-byte records; not decoded) |
| `[node+0x04]` | the wave message list `obj`, built by `0xC01ACCF4(this, id)` (id in the high half-word), `0xC01ACBF4(this, id, aux)` (SUPER `Organ`, `Saw Bright`, `Square Bright`) or `0xC01ACCD8(this, table_id)` (WAVE, calls `0xC01ACBF4(this, 0x36, table_id)`) |
| `[node+0x08]` | type string: `Analog` (`0xC02BF238`) or `Wave` (`0xC02D766C`); FM nodes hold `FM-H` / `FM-I` (`0xC02D7ECC`, `0xC02D7EE4`) |
| `[node+0x0C]` | category string |
| `[node+0x10]` | entry name |
| `[sp+0x4C]`, `[sp+0x50]` | category index, entry index (arguments of `0xC0020AF0`) |

`tools/ns4_wave_cats.py` runs this on a small symbolic register machine (immediates, `sp` and allocated objects; `cmp` plus conditional branches are evaluated, so the FM loops and the loop counter `[sp+4] = 5` that the `Organ` ids read are exact). It stops at the first `pop {.., pc}`. The walk rejects any category whose entry indices are not `0..n-1`. Result (VERIFIED, `uv run --with capstone python tools/ns4_wave_cats.py target/fw_unpacked/bank0_CODE.bin`; the test `test_firmware_walk_reproduces_the_committed_table` compares it with the committed CSV):

| Cat | Name | Entries (entry index = position; `id` = wave id) |
|---:|---|---|
| 0 | Pure | Sine 0, Triangle 1, Saw 2, Square 3, Pulse 33 4, Pulse 10 5, White Noise 46 |
| 1 | Sub Osc | Sub Square Saw 10, Sub Pulse Saw 11, Sub Sine Saw 12 |
| 2 | **Sync** | Sine 27, Triangle 28, Saw 29, Square 30, Pulse 33 31, Pulse 10 32, Chop Saw 1 33, Chop Saw 2 34, Chop Square 35 |
| 3 | Shape | Triangle 7, Saw 8, Square 9, Half Sine 43, Parabolic 44 |
| 4 | Shape Sine | Saw 38, Square 39, Pulse 13, Squeeze 41, Fold 42, ESaw 36, ESquare 37 |
| 5 | Multi | Saw 16, Saw 8ve 17, Saw 5th 19, Saw 5th+ 20, Saw 8ve 8ve+ 18, Saw 8ve 5th+ 21 |
| 6 | Super | Saw 22, Square 23, Organ 24, Saw Bright 25, Square Bright 26 |
| 7 | Misc | Pink Noise 47, Red Noise 48, Bell 45 |

WAVE (table ids, not in category order): Bells/Tines `Bell 40, Bar 1 41, Bar 2 42, Tines 43, Marimba 44, Tubular Bells 45`; Acoustic `Flute 1 10, Flute 2 11, Clarinet 1 12, Clarinet 2 13, Alto Sax 14, Tenor Sax 15`; Digital `2nd .. 8th Spectra 16..22, Saw Random 23, Saw Bright 24, Saw NoFund 26, Square Bright 25, Ice 1 34, Ice 2 35, Triplets 39`; Organ `Second 0, Third 1, Jimmy Smith 2, Blues 3, Gospel 4, Church 5, Squabble 6, Full Organ 7, Full Organ+ 8, Principal 9`; Keys `EPiano 1..3 27..29, DX 1 30, DX 2 31, Full Tines 32, AcPiano 33, Clavinet 1..3 36..38`. Some names (`Tines`, `Second`, `Third`, `Organ`) are shared with other strings, so the string pool alone cannot show them.

## 3. Cross-check with the DSP side

The wave ids of the ARM tree are independent of `docs/research/data/synth-wave-types.csv` (derived from the SHARC P-table by `tools/synth_wave_probe.py`: `type` per `(cat, wave)`). All 45 ANALOG `(cat_index, wave_index) -> wave_id` pairs are equal, including the non-monotone Shape Sine order (Saw 38, Square 39, Pulse 13) and the `White Noise` id 46. Test: `tools/tests/test_ns4_wave_cats.py::test_analog_wave_ids_match_the_dsp_side_wave_type_table` (runs in CI, needs only the committed CSVs). The DSP table also fills the unused slots of each category with id 2 (`Saw`); the tree has no entries there.

## 4. What this changes in our tables (`crates/nord-gui/.../synth/waves.rs`)

| Item | Before | Now |
|---|---|---|
| ANALOG categories | 7, no `SYNC`, `SUB OSC` second (index != `osc_category`) | 8 in firmware order; GUI index equals `osc_category` |
| `SYNC` | absent | 9 waves, ids 27..35, nearest plain shapes (`exact = false`, the engine has no oscillator sync, ASSUMED) |
| `SHAPE SINE` order | Pulse, Saw, Square, Squeeze, Fold, ESaw, ESquare | Saw, Square, Pulse, Squeeze, Fold, ESaw, ESquare (firmware entry index) |
| `MULTI` | `Multi Saw 8ve` ... | firmware names `Saw`, `Saw 8ve`, ... ; the OLED text keeps `Multi ` as a category prefix |
| `SUPER` | 4 invented: `Super Saw`, `Super Square`, `Super Bright`, `Super Organ` | 5: `Saw`, `Square`, `Organ`, `Saw Bright`, `Square Bright` (prefix `Super `; `Saw` stays the only `exact` entry) |
| `MISC` | `Pink Noise`, `Red Noise`, `Bell` | same names, now with ids 47, 48, 45 |
| WAVE type | 8 invented groups `TABLES n` of 8 tables, label `Table n` | 5 firmware categories and 46 names; the engine bank index is the flat position (`table_flat_index`, the same order as `cbin` `wave_flat_index`, so the panel and a loaded program agree) |
| FM groups | `ALGO A..E` | `HARMONIC A..E` / `INHARMONIC A..E` (the manual's `FM Harmonic A` is the same set) |

OLED text for MULTI and SUPER: the firmware stores `Saw 8ve` under `Multi`, while the manual's OLED figure (p.9: `Multi Saw 8ve`, `ANALOG MULTI 2(6)`) and p.30 show `Multi Saw ...` and `Super Saw`, `Super Organ`. The third-party decoder lists every category as `<Category> <name>` (`Pure Sine`, `Sync Saw`). Our screen composes the prefix only for MULTI and SUPER (EMPIRICAL for those two, the manual figure); the rule for the other categories is **ASSUMED** to be the bare name. The routine that draws the title was not found (the pixel path is not decoded, [oled-strings-vs-firmware](oled-strings-vs-firmware.md) s5).

## 5. STORE `Cat:` and the program category

What the disassembly shows (VERIFIED, `uv run --with capstone python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC014D3D8 0xC014D700`):

1. `0xC014D3D8` is the refresh method of the store screen (its pointer is at `0x2C0F98`; the string pool next to it holds `StoreName`, `StorePos`, `StoreProg`, `Abc`, `Del`, `Ins`, `Store Disabled / Memory Protected / in System menu`). At `0xC014D4EC` it loads `0xC02C0E7C` (`Cat:`) and calls the text setter `0xC001BD0C` on the widget `[this+0x48]`: this is the **caption**. With `[this+0x2D] != 0` the same widget gets `As:` (`0xC02C0E84`, `0xC014D690`).
2. The **value** is written at `0xC014D5AC..0xC014D5D4`: `model = [this+8]`, `s = model->vtbl[0x20]()`, then `[this+0x4C]->vtbl[4](fmt "%s" (0xC02C0E9C), s)`. The class of `[this+8]` was not identified (its constructor was not found; slot `0x20` of the category list model, `0xC00E9BE4`, is an event dispatcher, not a name getter, so `[this+8]` is not that object itself).
3. The 26-name table `0xC02BF3C8` belongs to the **category list model** class (vtable `0xC02D4238`, built by the UI init at `0xC019EC70`, stored at `[root+0x178]`): slot `0x30` is `0xC017B4C8(this, i, short) = table[2 i + short]`, slot `0x38` is a count helper `0xC017B47C`. The LIST screen reads it (`0xC0133F44`: "if slot `0x30` of `[this+0x78]` is `0xC017B4C8` then `table[2 i + 1]`", the short names). The manual (Program chapter, STORE AS and CATEGORY, p.41-43) says the Cat dial shows an alphabetically sorted category list, which is this table's order (the last two entries `User`, `Undefined` are outside the sort).
4. The program category is **not** an edit parameter (no `Edt_*Category` string in the image) and is not in the program bitstream: program names and categories live in the CBIN entry ([cbin-and-presets](../specs/cbin-and-presets.md): "Program names are not in the payload"), so the value is the header field `category_id` at `+0x10`.

The header id is **not** the table index (VERIFIED, `factory_programs/**/*.ns4p`, header `u32` at `+0x10`): the 384 factory programs use 18 distinct ids `1, 2, 3, 6, 7, 9, 11, 13, 23, 30, 40, 42, 44, 45, 46, 47, 49, 52` (counts 8, 17, 9, 22, 33, 57, 31, 10, 60, 26, 13, 11, 4, 41, 26, 5, 5, 6), up to 52 against a 26-entry table (`0..25`). By program names the ids look like `1` Bass, `2` Brass, `3` tuned percussion, `6` lead, `7` organ, `9` piano, `11` strings, `13` choir, `23` electric piano, `40` effects, `42` guitar, `44..46` synth bass / classic / pad, `47` wind solo, `49` analog strings, `52` synth brass: a wider enumeration than the 26 display names (it has an electric-piano class the 26 names lack). This reading is **ASSUMED** (names only). Searches for a mapping that ran empty: strictly increasing `u8` / `u16` / `u32` tables of 20 or more entries that contain the 18 ids; byte / half-word / word arrays with `a[1] = 1, a[7] = 11, a[9] = 13` (the ids of bass, organ, piano against the table indices); the `CBIN` magic and `ns4p` tag are not built in `bank0_CODE.bin` where the header is parsed (`ns4p` appears once, in the file-type name list at `0x444B38`). The mapping from the header id to the 26 names is therefore **unresolved** and the store flow still defaults to `Undefined`.

Consequence for `bay_program/name.rs`: the 26 names are VERIFIED as a firmware category list model (getter, order, manual wording); the claim that the STORE `Cat:` value shows that list stays ASSUMED but is now bounded: caption VERIFIED, list model VERIFIED, value source open. `docs/specs/cbin-and-presets.md` line 59 listed ids `0x12 Organ` ... `0x2F Pluck/Acoustic`; the factory data above shows `Organ` is 7 for programs and no program uses `0x12`, so that list is corrected to the observed ids with the ASSUMED reading.

## Implications

- The Synth OLED category and waveform names of ANALOG and WAVE, the FM group names and the category index of the CAT dial are now the firmware's (45 + 46 entries, tests against the committed CSV).
- `SYNC` is a real category (9 waves, ids 27..35); the engine plays it as plain shapes until oscillator sync is modelled.
- The WAVE-type browser indexes our synthetic bank by flat position; the firmware wave-table ids (`0..45`, not in category order) are in the table for a future real bank.
- A STORE flow can offer the 26 names, but writing a category into an exported `.ns4p` header needs the id mapping, which is unknown.

## Follow-ups

1. Resolve `[this+8]` of the store screen: find the constructor of the class with the refresh method `0xC014D3D8` (scan for stores of the vtable address that holds it) and its model argument; then read how the model turns the header `category_id` into a name (probably a lookup into a larger table than `0xC02BF3C8`).
2. Fit the header id to the 26 names with a second source (a program the hardware displays with a known category, or the ns4b `Cat` sort order of the factory backup) and record it in `cbin-and-presets.md`.
3. Find the OLED title draw call for the wave name to settle the `Multi `/`Super ` prefix rule for every category (needs the real render, bean `ns4-2gdc`).
4. Decode the 16-byte callback records `0xC02D80E0..0xC02D8240` of the node (`[node+0]`); they probably select the per-wave Osc Ctrl behaviour (`Detune:`, `Shape:`, `PW:`, `Mix:` captions at `0x2BF2D8..`).

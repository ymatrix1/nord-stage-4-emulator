# Synth arpeggiator / gate pattern presets

- **Checked:** 2026-11-14 (first pass 2026-11-13: table not found; second pass: table recovered)
- **Sources:**
  - Local, never committed: `.reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b` (384 `.ns4y` synth presets), `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66), `.reference/ns4decode_pkg/ns4decode_pkg/ns4decode.py` and `ns4maps.py` (third party, MIT), `.reference/Nord Stage 4 User Manual v1.6X-Edition-N.pdf`, `.reference/DSoundman-course/*.srt`.
  - In repo: [arm-edit-params.csv](arm-edit-params.csv), [arp-preset-table.csv](arp-preset-table.csv) (result), `tools/arm_xref.py`, `tools/arm_emu.py`, `tools/ns4_arp_patterns.py`, `tools/ns4_arp_presets.py` (new, tests in `tools/tests/test_ns4_arp_presets.py`).
  - Web (text not copied): https://www.nordkeyboards.com/products/nord-wave-2/specifications/ ("30+ editable presets"), https://www.norduserforum.com/viewtopic.php?t=25610 (search snippet only: the Stage 4 arp pattern "cannot be edited" beyond the factory patterns), a search for the preset names (`Alien Font`, `Ringaracka`, `RWGate`, found by step 2 below) returned no page that lists them.
- **Question:** What are the real factory arpeggiator / gate pattern presets of the Nord Stage 4 synth (the engine's 8 presets in `synth/perf.rs` were `ASSUMED`), and how does a `.ns4y` store a pattern?

## 1. Result

**The table is recovered.** `Edt_Syn_kArpPtnPreset` (id `0x114`, range `0..28`) selects value 0 (default pattern) or one of **28 firmware table entries**. Each entry is a gate word and a pan word of 16 two-bit steps. They are in [arp-preset-table.csv](arp-preset-table.csv) (gate `E q Q .`, pan `C L R`, steps 8 or 16, accent always 0). Reproduce:

```bash
uv run --with unicorn python tools/ns4_arp_presets.py table target/fw_unpacked/bank0_CODE.bin \
    --sram target/fw_unpacked/bank0_SRAM.bin
python tools/ns4_arp_presets.py check ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
    docs/research/arp-preset-table.csv
```

## 2. How it was found (angle 1, the handler of `0x114`)

| # | Finding | Evidence |
|---|---|---|
| 1 | The registration of `0x114` passes `sl` as the third word of the info struct (`{1, 0, sl}` at `[sp,#0x30..0x38]`, `0xC01AA264..0xC01AA29C`); the other ids pass 0. In the registry CSV this is the last field (`1;5;1` for `0x114`, `0x115`, `0x10a`), probably a "not stored in the patch" flag. It is **not** a handler pointer (the first pass misread it). | EMPIRICAL (the flag meaning is inferred; the pointer reading is ruled out by the callee): `python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC01AA240 0xC01AA2D0` and the callee `0xC00F0A28` (stores the struct at `obj+0xC4`), [arm-edit-params.csv](arm-edit-params.csv) rows 327 to 342. |
| 2 | The consumer is the Glue object `GlueArpPtnPreset %c` (string `0xC02B75D8`, vtable `0xC02B7C10`). Its vtable method `0xC0102F30` is the change listener (ids `0x114`, `0x118`) and `0xC010FDAC` is the update method. With `[obj+0xEC] == 2` (preset changed) it reads the dial via `0xC00247E0` (param lookup), indexes a table and computes the words. | VERIFIED: vtable words at `0xC02B7C18..0xC02B7C24`; `python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC010FF50 0xC0110050`. |
| 3 | The table is **28 structs of 20 bytes at `0xC02BA860`** (`bank0_CODE.bin` file offset `0x2BA860`): `name*` (+0), `flag` (+4, byte), `steps` (+8, 8 or 16), `A*` (+12), `B*` (+16). `A` and `B` are strings with one character per step: `S` (short), `L` (long), other = rest (`_`). `B` is empty when `flag` = 0. The first pass missed it because it looked for the 32-bit words; the table stores them as text. | VERIFIED: `read_struct` in `tools/ns4_arp_presets.py` (test `test_read_struct_decodes_one_entry` on a synthetic image; real image: `table` command). |
| 4 | The routine turns the strings into three words: **Dur (`0x118`)**, **Pan (`0x119`)**, **Acc (`0x117`) = 0**, and writes Length (`0x116`) = steps - 1. Dur code per step: rest 3 (`.`); `S` 0 (`E`); `L` followed by a played step 2 (`Q`); `L` followed by a rest (or the last step) 1 (`q`). Pan comes from the merge of `A` and `B` (flag 1 only): 0 `C`, 1 `L`, 2 `R`. Re-implementing the merge was avoided: the tool **runs the real routine** from `0xC010FF50` to `0xC010FE00` on the ARM emulator (`tools/arm_emu.py`, `unicorn`) with the dial read stubbed, and reads r8 (Dur), r6 (Pan), r7 (Acc) and `[sp+4]`. | VERIFIED: the apply code `0xC010FE00..0xC010FEF4` stores r8 to `0x118`, r6 to `0x119`, r7 to `0x117`, `[sp+4]` to `0x116` (literal pools `0xC011069C..0xC01106A4`); emulation output in [arp-preset-table.csv](arp-preset-table.csv). |
| 5 | Dial value 0 is not a table entry. The parameter default is 0, the default pattern is 8 steps `E`, all centred (the factory backup holds it in 4 layers), so index 0 of the engine is that pattern. | ASSUMED for the identity (no firmware code path for 0 was traced: the emulated index -1 reads the preceding data and is meaningless); the pattern itself is EMPIRICAL (`EEEEEEEE`, 4 factory layers). |
| 6 | Preset names are the string just before each `A` (struct field `name*`). They are not copied into the repo (short Nord-authored titles); `python tools/ns4_arp_presets.py table ...` prints them. | Tool output (column `name`). |

## 3. Cross-check against the factory programs (angle 2)

A program stores the result (gate, accent, pan words and length), not the preset index (layer block offsets in section 4). Comparing the 41 enabled patterns of the factory backup with the recovered table, `python tools/ns4_arp_presets.py check ...` finds, with the same steps, gate and pan words and no accent ("exact"), or the same gate only:

| Preset | Exact | Gate only |
|---|---|---|
| 0 (default `EEEEEEEE`) | 4 layers (Mirror Image Pad C, Unclocked Pad A, Water Ripples A and C) | 4 |
| 2 | Ghost Pad B | 1 |
| 4 | Dual Gates A | 1 |
| 5 | none | 3 (the factory pan `RCCLCCRCCCLCCCRC` is a hand edit) |
| 11 | FM Keys 2 A | 1 |
| 13 | Glitter Whl A, Tri Pump Arp A | 2 |
| 25 | Moving Chords A | 1 |
| 27 | none | 1 |

Five table presets (2, 4, 11, 13, 25) equal a factory layer in steps, gate and pan; four of them (2, 4, 13, 25) have a non-trivial 16-step pan word, so a chance match is impossible (4^16 words per field). Presets 5 and 27 match in gate only (the factory pan was edited). **VERIFIED** for the whole table: the same routine produces all 28 entries, and the exact matches prove that it computes the words the firmware stores into a patch. The remaining factory patterns (12 steps `EEE.E.EEE.E.` with accent, 9 steps `E..E..E..`, 4 steps, ...) have no table entry: they are user-style edits. Names, lengths and rhythms are not published (angle 3: no pattern name appears in the course subtitle files; angle 4: web search found no list, see Sources).

## 4. Storage in `.ns4y` (unchanged from the first pass)

Five fields in the `0x32` Layer block (bits relative to the layer base `248 / 656 / 1064`): `pattern_on` `+138` (1), `length` `+206` (4, steps = raw + 2), `accent` `+210` (32), `gate` `+242` (32), `pan` `+274` (32); each 32-bit word is 16 two-bit steps, step n at bits `2n..2n+1` from the LSB. Gate `0 E`, `1 q`, `2 Q`, `3 .` (rest); accent `1` = accent; pan `0 C`, `1 L`, `2 R`. EMPIRICAL (ns4decode ids `424-3`, `432-7`, `433-3`, `437-3`, `441-3`; `tools/ns4_arp_patterns.py`); the exact matches of section 3 confirm the gate and pan digit order and the `E q Q .` / `C L R` letters (`Q` and `.` occur in exact matches; `q` only in firmware presets 10, 12, 22 and 24, never in an exact factory match).

## 5. Engine change

`synth/pattern_presets.rs` (new): `PATTERN_PRESET_COUNT` 8 to **29**, `PATTERN_PRESETS[0..=28]` built from the gate and pan strings of the CSV, `GatePattern` gains a per-step `pan`; `perf.rs` plays the preset pan; `CustomPattern::from_preset` copies it. Host parameter `synthg_pat_preset_{1,2,3}` range 0..7 to 0..28 (no id removed; the step count in `param-ids.snapshot` changes 7 to 28). GUI: dial and `PatternPreset` clamp 28, OLED shows the hardware value (0..28).

| Item | Status |
|---|---|
| Steps, gate code and pan of presets 1..28 | VERIFIED (firmware routine; 5 presets equal factory layers exactly and 2 more in gate, section 3) |
| Preset 0 = default pattern | ASSUMED (section 2 row 5) |
| Gate durations of `E`, `q`, `Q` in eighths (4, 6, 8) | ASSUMED: the firmware shows only that `Q` is a long step before a played step and `q` a long step before a rest; the durations are unmeasured |
| Accent of presets | VERIFIED 0 (`r7` = 0 in the apply code) |

## 6. Implications and follow-ups

- The preset dial now matches the hardware in count, order, length, rhythm and pan; the sound of the gate lengths is still approximate.
- The Pattern page display of the hardware draws the strings (`S`/`L`/`_`) and the merge of `A` and `B` (code `0xC010FDAC..0xC01106A8`, also a "preset dirty" comparison against the current words); the emulator does not model the dirty marker yet.
- Follow-ups: (1) measure the gate duration of `E`, `q`, `Q` (recording of presets 10, 12 or 24, which contain `q`); (2) decode the pattern words in `crates/nord-core/src/cbin/synth_preset.rs` and import factory patterns into `CustomPattern`; (3) model `Edt_Syn_kArpPtnPresetDirty` (the hardware shows a preset as edited once a step changes); (4) trace what the hardware does for dial value 0.

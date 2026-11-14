# Layer key ranges: the firmware map from `kKbZone` codes to keys

**Checked:** 2026-11-14
**Sources:** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (v1.66, local), the 384 factory programs of `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (local), `tools/arm_zone_map.py`, `tools/arm_zone_programs.py` (new), [keydown-zones](keydown-zones.md), [engine-routing](engine-routing.md). Only addresses, offsets, derived values and counts are recorded; no firmware bytes beyond the derived tables (D20).
**Question:** What key range does a layer accept for each of the 12 `Edt_*_kKbZone` codes, which code writes the range (the setters `0xC00C843C` / `0xC00C83FC` had no static callers), and does the engine's EMPIRICAL code table (0 lower, 6 upper half) match the firmware?

Tags: **VERIFIED** = reproduced by a command against the ARM firmware, **EMPIRICAL** = fitted, **ASSUMED** = reading.

## 1. Answer in short

| Question | Answer | Tag |
|---|---|---|
| Who calls the range setters? | `0xC010ABA4(store, flag, out)`: four `blx` through the layer's secondary vtable (`[vtable + 0x3C]` = `0xC00C843C`), one per virtual channel class (1, 6 / 7 / 8, layer channel, 16), reached from the store method `0xC011AED4` when a `kKbZone` or `Edt_Com_kSplit*` change alters the range. This is why no static `bl` reaches the setters | VERIFIED (write and call hooks) |
| Where is the map? | Two pure functions `0xC01B0DC4` (lower bound) and `0xC01B0C40` (upper bound) look the code up in a byte table at `0xC02CB230` (`[mask][code][sel]`, 8 x 12 x 2) and switch on the case 0..10; `0xC01B0A44` resolves the position setting of a split point against its active neighbours | VERIFIED (disassembly, 0 mismatches against the ARM, section 4) |
| Does the old EMPIRICAL table hold? | **No.** Only `0` (zone 1 / lower half), `5` (all) and, for a Mid-only split, `6` (upper half) were right. With four zones: `2` = zones 1-2, `3` = zones 2-3, `4` = zones 1-3, `6` = zones 2-4, `8` = zones 3-4, `9` = zone 3, `10` = zone 4 (`1` = zone 2). The old table read `2` as zone 3, `3` as zone 4, `4` as zones 1-2, `8` as zones 2-4 | VERIFIED |
| Does the split position table match? | **No, a decode bug.** The firmware table has 11 settings `36, 41, 48, 53, 60, 65, 72, 77, 84, 89, 96` (`Edt_Com_kSplitPos*` range `0..10`); the decode had 10 entries starting at 41 and read every setting one slot too high (default Mid `4` is key 60, not 65) | VERIFIED |
| Crossfade widths and curves | width setting `0 / 1 / 2` = 0 / 6 / 12 keys half width (the old engine used 0 / 3 / 6); the two bounds fade with the firmware curves (12 and 24 entries, 16.16) and a layer gets the **smaller** of the two bound gains | VERIFIED |
| All 384 factory programs | the ranges and the 88-note gains of all 7 layers equal the model: 0 mismatches (48 programs split, 28 distinct `(point mask, code)` pairs) | VERIFIED |

## 2. Call path

`0xC010ABA4(this = layer edit store, r1 = 0, r2 = out)`:

1. `fp = [layer object + 0x28](&{id, 0})`: the layer's `kKbZone` code (id `0x161` organ, `0x14D` piano, `0xF4` synth).
2. `0xC01B0DC4(cfg, fp, [store + 0x80], &lower)` and `0xC01B0C40(cfg, fp, [store + 0x80], &upper)`: `cfg` = the split state object `[0xC059B334]` (`0x90DC27D0` after boot). Each fills `{key, fade width in keys, curve pointer, curve length}`.
3. Four calls of the layer's vtable slot `+0x3C` (`0xC00C843C(this, 0, slot, class, lower, upper)`) for the classes in `[store + 0x6C / +0x70 / +0x74]` (section 6 / 7 / 8, layer channel 9..15, all 16) and class 1. The setter stores `lower` at zone `+4..+0x10` and `upper` at `+0x14..+0x20`.
4. `0xC00C859C` (note path): a note is accepted when `lower.key - lower.width <= note <= upper.key + upper.width` and the zone byte is set; the gain handed to the layer key object is `min(0xC00C84B8(lower), 0xC00C84B8(upper))` (16.16).

The split state object is written by `0xC01AC844(this, _, _, &id, value)` (vtable word `0xC02B78E8`): `0xE7` store `value == 1` at `+0xD0`, `0xE8..0xEA` at `+0xD1..+0xD3` (Low, Mid, High), `0xEB..0xED` the position settings at `+0xD4 + 4k`, `0xEE..0xF0` the width settings at `+0xE0 + 4k`, `+0xEC` / `+0xF0` hold the lowest / highest key (21, 108). The ranges are rewritten only when the computed range changes, so a repeated `kKbZone` dispatch with unchanged state does not reach the setters (observed); the tool calls `0xC010ABA4` directly after writing the code cell.

## 3. The tables

Case table `0xC02CB230` (`mask` bit 0 = Low point active, bit 1 = Mid, bit 2 = High; active = split enabled AND point flag). The column `sel = [store + 0x80]` (0 on every layer store; `1` only changes code 11 to case 10, an empty range `0 .. 1`, role open):

| mask (points) | code 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 (none) | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 9 |
| 1 (Low) | 0 | 0 | 0 | 0 | 0 | 9 | 8 | 8 | 8 | 8 | 8 | 8 |
| 2 (Mid) | 4 | 4 | 4 | 4 | 4 | 9 | 6 | 6 | 6 | 6 | 6 | 6 |
| 3 (Low, Mid) | 0 | 1 | 4 | 4 | 4 | 9 | 8 | 8 | 8 | 1 | 6 | 6 |
| 4 (High) | 7 | 7 | 7 | 7 | 7 | 9 | 3 | 3 | 3 | 3 | 3 | 3 |
| 5 (Low, High) | 0 | 5 | 7 | 7 | 7 | 9 | 8 | 8 | 8 | 5 | 3 | 3 |
| 6 (Mid, High) | 4 | 2 | 7 | 7 | 7 | 9 | 6 | 6 | 6 | 2 | 3 | 3 |
| 7 (all) | 0 | 1 | 4 | 5 | 7 | 9 | 8 | 5 | 6 | 2 | 3 | 3 |

Case to bounds (jump tables `0xC01B0E20` lower, `0xC01B0C9C` upper; `P0, P1, P2` = split key of the Low, Mid, High point, `min` 21, `max` 108):

| case | lower | upper | zones with all four points |
|---|---|---|---|
| 0 | min | P0 - 1 | 1 |
| 1 | P0 | P1 - 1 | 2 |
| 2 | P1 | P2 - 1 | 3 |
| 3 | P2 | max | 4 |
| 4 | min | P1 - 1 | 1-2 |
| 5 | P0 | P2 - 1 | 2-3 |
| 6 | P1 | max | 3-4 |
| 7 | min | P2 - 1 | 1-3 |
| 8 | P0 | max | 2-4 |
| 9 | min | max | all |
| 10 | 0 | 1 | none |

Codes with all four points: `0` zone 1, `1` zone 2, `2` zones 1-2, `3` zones 2-3, `4` zones 1-3, `5` all, `6` zones 2-4, `7` zones 2-3, `8` zones 3-4, `9` zone 3, `10` zone 4, `11` zone 4. With fewer points the zones next to an inactive point join (mask 2, Mid only: codes 0-4 lower half, 6-11 upper half, 5 and `9` in a no-split program all).

Split keys (data `0xC02D8370`): `36, 41, 48, 53, 60, 65, 72, 77, 84, 89, 96` for settings `0..10`; width settings (data `0xC02D839C`): `0, 6, 12` keys; curves `0xC0569018` (12 entries) and `0xC0569048` (24 entries): `0x72B .. 0xFDB2` and `0x560 .. 0xFEF9` (16.16, sigmoid with `0x8000` in the middle); curve setting 0 is the single entry `0x10000`. The values are in `tools/arm_zone_map.py` and `engine/zone_table.rs`.

Position resolution `0xC01B0A44(cfg, k)`: with at most one active point, or point `k` itself inactive, the setting is clamped to `0..10`; with two active points `a < b` the lower one is limited to `<= max(P[b] - 1, 0)`, the upper one to `>= min(P[a] + 1, 10)`; with three: Low `<= min(P1 - 1, 8)` (0 when negative), Mid in `[clamp(P0 + 1, 1, 9), clamp(P2 - 1, 1, 9)]`, High `>= 2` when `P1 + 1 <= 1` else `>= min(P1 + 1, 10)`. The `>= 2` branch (a jump that skips the `mov r1, #0` of the shared tail) was the one mismatch of the first port; the final port is equal to the ARM function over all `8 x 11^3 x 3` inputs.

## 4. Checks (all run)

| Check | Result |
|---|---|
| Table bytes (case table 192, split keys 11, widths 3, curves 36) read back from the firmware image / RAM | 0 differences |
| `0xC01B0A44` for 8 flag sets x 1331 position triples x 3 points (32 k calls) | 0 differences |
| `0xC01B0DC4` / `0xC01B0C40` for 8 masks x 7 position triples x 3 widths x 12 codes x 2 sel (key and curve length) | 0 differences |
| 384 factory programs: split state and the seven codes written into the split state object and the layer code cells, `0xC010ABA4` run, zone objects read (key, width, curve, length, `+4` / `+0x14`) and `0xC00C84B8` for all 88 keys x both bounds | 0 mismatches; 48 programs split (masks: 37 Mid, 4 Low+Mid, 5 Mid+High, 2 all; width sets include `(0, 1, 2)`, `(2, 2, 0)`) |
| Octave shift `-2, -1, 1, 2` (`Edt_*_kOctaveShift`) on every layer for the first 8 programs | ranges unchanged: the range depends on the physical key only (`0xC010ABA4` never reads the octave) |
| Rust port | `engine/zone_table.rs`: `digest()` (FNV-1a over the case table and 10 368 range samples with 8 gains each) equals `tools/arm_zone_map.py` `digest()` = `0x32825b100986beb1` |

```bash
C=target/fw_unpacked
uv run --with unicorn python tools/arm_zone_map.py --check $C/bank0_CODE.bin $C/bank0_SRAM.bin --cache target/keydown.snap
uv run --with unicorn python tools/arm_zone_programs.py $C/bank0_CODE.bin $C/bank0_SRAM.bin factory_programs --cache target/keydown.snap
uv run --with unicorn python tools/arm_zone_programs.py $C/bank0_CODE.bin $C/bank0_SRAM.bin factory_programs --cache target/keydown.snap --limit 8 --octave-check
```

## 5. Engine changes

- `engine/zone_table.rs` (new): the VERIFIED tables, `split_index`, `zone_range`, the two-bound gain.
- `routing.rs`: `zone_leds(code)` now reads row 7 of the firmware table; `ProgramRouting::zone_range / zone_gain` use `zone_table`; `zone_assign::zone_code` is the inverse (the KB ZONE panel buttons write the right codes).
- `cbin/presets.rs`: `SPLIT_NOTE_TABLE` has the 11 firmware keys (decode bug, section 1). `ProgramDecoded` split keys of factory programs therefore move (for example `B3 Chords n Solo` Mid 65 to 60). The `KeybedSplitRouter` default notes `48 / 60 / 72` and `GlobalProgramDecoded::default` `48 / 65 / 84` are ASSUMED placeholders and stay.
- Legacy path (`FirmwareRouting::Off`) is untouched apart from the corrected split keys; `render_hash` is unchanged (D22: the default stays Off).

## 6. What stays open

| Item | State |
|---|---|
| `[store + 0x80]` (the `sel` column) | 0 on all 14 layer stores; code 11 with `sel == 1` is an empty range. Likely the aux keyboard assignment; ASSUMED, no program stores code 11 |
| Whether the zone test sees the transposed note | the note handler `0xC00D1F48` shifts by the channel profile before the zone test; the engine still gates with the unshifted key. Needs a run with a program transpose |
| Who calls `0xC011AED4` in the program load | the edit-id dispatch calls it when the range changes (observed); the loader that fills the section table (OQ25) is still not executed |
| Vox / Farfisa programs without an enabled layer | OQ25, see [engine-routing](engine-routing.md) section 7 |
| Hardware clip of a 3 / 4-zone program | the clip comparison of [engine-routing](engine-routing.md) section 5 used Mid 65; with the corrected key (60) notes 60..64 move to the upper layer, a re-run needs the local video |

## Implications

- The firmware, not a naming guess, now decides which layers play which keys; `--firmware-routing` is VERIFIED on both criteria of D22 (algorithm and mapping) for the key range. The default flip is still blocked by OQ25 (section 7 of [engine-routing](engine-routing.md)).
- Crossfades at splits are the firmware's sigmoid curves over 12 or 24 keys with a min() of the two bounds, not the linear 6 / 12 key ramp; programs with a width setting 1 or 2 (19 of the 48 split programs) change.

## Follow-ups

1. OQ25: execute the program load, or find a hardware recording of one of the nine silent-in-scene-I programs.
2. Check the transposed-note question and the `sel` selector with a key-on through the real note path for a split program.
3. Re-run the B3 clip comparison with the corrected Mid key (needs the local video).

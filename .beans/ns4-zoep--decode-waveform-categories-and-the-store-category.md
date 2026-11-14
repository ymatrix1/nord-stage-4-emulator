---
# ns4-zoep
title: Decode waveform categories and the STORE category link
status: completed
type: task
created_at: 2026-11-14T04:01:14Z
updated_at: 2026-11-14T04:01:14Z
parent: ns4-xc5n
---

## Context

`docs/research/oled-strings-vs-firmware.md` s2, s4, Follow-ups 4 and 5. The waveform names sit in code order at `0x2D7D44..0x2D80D8` (`Pure`, `Sine`, `Triangle`, `Saw`, `Square`, `Pulse 33`, `Pulse 10`, `White Noise`, `Shape`, `Half Sine`, `Parabolic`, `Pulse`, `Shape Sine`, `ESaw`, `ESquare`, `Squeeze`, `Fold`, `Sub Osc`, `Sub Square Saw`, ..., `Saw 8ve`, `Saw 5th`, `Saw 5th+`, `Saw 8ve 8ve+`, `Saw 8ve 5th+`, `Super`, `Saw Bright`, `Square Bright`, `Chop Saw 1`, `Chop Saw 2`, `Chop Square`, `Pink Noise`, `Red Noise`, `Bell`, then the FM, WAVE and spectra sets); the category membership is the order of the init routine whose literal pools start at `0xC01AEB3C`, not a table. Ours: `Multi Saw 8ve` etc. against `Saw 8ve`, `SUPER` items `Super Saw / Square / Bright / Organ` (not in the firmware), `MISC` = `Pink Noise`, `Red Noise`, `Bell`. The program category table `0xC02BF3C8` (26 pairs) is used by the getter `0xC017B4C8` of a list model; its link to the STORE `Cat:` field (`0xC014D4EC`) is ASSUMED.

## Higher Goal

Waveform, WAVE-table and category names on the Synth OLED are the firmware's.

## Acceptance Criteria

- [x] Trace the routine that pushes the waveform names and write the category -> wave list with offsets in the note.
- [x] Rename the MULTI / SUPER / MISC items to the firmware names (keep the engine mapping and `exact` flags), with tests.
- [x] Trace `Cat:` (`0xC014D4EC`) to the category getter and mark the link VERIFIED or correct `bay_program/name.rs`. Traced as far as the data allows: caption and list model VERIFIED, value source and header-id map open (OQ26), `name.rs` tags corrected.
- [x] Read the WAVE (wavetable) and FM-algorithm name sets and compare with `tables.rs` (`ALGO A..E`, the eight invented WAVE categories).

## Out of Scope

Waveform DSP accuracy.

## Summary of Changes

- `tools/ns4_wave_cats.py` (+ `tools/tests/test_ns4_wave_cats.py`): symbolic walk of the firmware oscillator-list builder `0xC01ADBC0`; writes `docs/research/data/synth-wave-categories.csv` (91 rows: names and wave ids). Ids cross-checked against the DSP P-table `synth-wave-types.csv`: 45 of 45.
- Findings: ANALOG has 8 categories including `SYNC` (9 waves, ids 27..35; we had none), `SUPER` has 5 entries (`Saw`, `Square`, `Organ`, `Saw Bright`, `Square Bright`), WAVE has 5 categories (6/6/14/10/10), FM groups are `Harmonic A..E` (25 partials) and `Inharmonic A..E` (61 pitches). Note: `docs/research/synth-wave-categories.md`.
- `nord-gui` `ui_state/synth/waves.rs` (new, split from `tables.rs`): firmware tree with `fw_id`, `Multi ` / `Super ` OLED prefix, WAVE names, `fm_cat_name`, `table_flat_index`; test `tables_match_the_firmware_tree` reads the CSV. CAT dial index now equals `osc_category`.
- `Cat:` trace: `0xC014D4EC` is the caption; the value comes from `[this+8]` of the store screen (class unresolved); header `category_id` of factory programs is `1..52`, not the 26-table index. `name.rs` tags corrected, OQ26 opened, D27 proposed; specs `cbin-and-presets.md`, `synth-engine.md`, `control-inventory.md` updated.

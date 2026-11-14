---
# ns4-zoep
title: Decode waveform categories and the STORE category link
status: todo
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

- [ ] Trace the routine that pushes the waveform names and write the category -> wave list with offsets in the note.
- [ ] Rename the MULTI / SUPER / MISC items to the firmware names (keep the engine mapping and `exact` flags), with tests.
- [ ] Trace `Cat:` (`0xC014D4EC`) to the category getter and mark the link VERIFIED or correct `bay_program/name.rs`.
- [ ] Read the WAVE (wavetable) and FM-algorithm name sets and compare with `tables.rs` (`ALGO A..E`, the eight invented WAVE categories).

## Out of Scope

Waveform DSP accuracy.

---
# ns4-l0km
title: Recover the factory arpeggiator pattern preset table
status: in-progress
type: task
created_at: 2026-11-14T12:46:56Z
updated_at: 2026-11-14T12:46:56Z
parent: ns4-xc5n
---

## Context

The synth Arpeggiator/Gate pattern dial (`Edt_Syn_kArpPtnPreset`, edit id `0x114`, range 0..28) has 29 values on the hardware; the engine had 8 `ASSUMED` presets and the first research pass (`docs/research/synth-arp-patterns.md`) did not find the table in the firmware, backups or the web.

## Higher Goal

The pattern presets sound and look like the hardware (1:1 parity).

## Acceptance Criteria

- [x] Find the handler of `0x114` in the firmware and the table it indexes (`GlueArpPtnPreset` update method `0xC010FDAC`, 28 structs at `0xC02BA860`).
- [x] Reproducible tool that runs the firmware routine (`tools/ns4_arp_presets.py`) with tests; result in `docs/research/arp-preset-table.csv`.
- [x] Cross-check against factory programs (5 presets equal factory layers exactly, 2 more in gate).
- [x] Engine: `PATTERN_PRESET_COUNT` 29 with gate and pan per preset, deterministic tests; GUI clamp, OLED value and host range 0..28 (no id removed).
- [x] Update `docs/research/synth-arp-patterns.md`, `docs/specs/synth-engine.md`, `docs/README.md`.
- [ ] Measure the gate duration of the codes `E`, `q`, `Q` (still `ASSUMED` 4, 6, 8 eighths).

## Out of Scope

Importing factory patterns from `.ns4y` into `CustomPattern`; the "preset dirty" marker; hardware recordings.

## Summary of Changes

See the PR `research/arp-factory-patterns`: tool + CSV + research note, `synth/pattern_presets.rs`, GUI and plugin range changes.

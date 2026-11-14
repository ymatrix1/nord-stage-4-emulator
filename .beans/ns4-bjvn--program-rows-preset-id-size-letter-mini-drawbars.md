---
# ns4-bjvn
title: 'Program rows: preset id, size letter, mini drawbars'
status: in-progress
type: task
created_at: 2026-11-14T10:43:50Z
updated_at: 2026-11-14T10:43:50Z
---

## Context
Follow-up of ns4-ow85: hardware rows show preset id (1:05) or source name plus size letter (XL, M) and organ mini drawbars; the emulator has no source for ids and sizes.

## Higher Goal
Match hardware display behaviour.

## Acceptance Criteria
- [x] Piano size letter (`S`, `M`, `L`, `XL`) after the model name, from the `.npno` header name (`nord_core::piano::label`, `tools/npno_types.py`: 30 of 38 models carry a size, 0 mismatches with the file name)
- [x] B3 mini drawbars as nine digits on the Organ row, from the panel drawbar state
- [ ] Preset id (`1:05`) or source name: no data source. A program stores no preset link and the emulator does not load `.ns4o` / `.ns4n` / `.ns4y` presets; the printed number also differs from the header location (OQ27). Blocked until hardware footage of the Num list or a preset-load path exists

## Out of Scope
Engine changes.

## Summary of Changes
Size letter and drawbar digits in `bay_program/oled_home.rs::section_rows` with `Summary::{piano_size, organ_bars}`; the piano row also drops the `#detail` part of the header name. Text form of the drawbars and the letter gap are ASSUMED (docs/specs/visual-style-and-panel.md 4.5). Delivered with `ns4-r8t9`.

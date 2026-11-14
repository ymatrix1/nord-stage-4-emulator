---
# ns4-rt23
title: Preset Library STORE and real preset loading
status: completed
type: task
priority: normal
created_at: 2026-11-14T17:31:29Z
updated_at: 2026-11-14T17:32:18Z
parent: ns4-xc5n
---


## Context
`ns4-r8t9` (PR #297) organizes the Preset Library of an opened `.ns4b` but left two things out: storing a new preset into the Preset Library (manual OS v1.6x p.42, "Store Preset To") and loading `.ns4o` / `.ns4n` / `.ns4y` files into the engine (the PRESET LIBRARY browser loaded Organ models, `.npno` models and `.nsmp4` samples, so selecting a preset did not set the section's parameters). The decoders for the three preset blocks already existed; the payload of a preset file is the program's section block.

## Higher Goal
Selecting a preset in ORGAN / PIANO / SYNTH PRESET sets the section like the hardware does, a stored preset reaches the exported backup, and every untouched bit of a `CBIN` file stays bit-identical (AGENTS.md rule 5).

## Acceptance Criteria
- [x] Preset payload == program section block, decoded by `SectionPreset` (`cbin/section_preset.rs`) with `to_file`, bit-merging `to_file_like`, `resolve_sounds`; `ns4 preset-check` decodes, re-encodes and loads all 544 factory presets (0 problems) and `tools/preset_check.py` checks sizes, schema ids and checksum of 937 CBIN files
- [x] Header checksum found: CRC-32 of the bytes from `0x2C` (937 of 937 files); program encoders fixed; OQ3 answered (D51)
- [x] Engine: `load_section_preset(preset, single)` through `apply_organ_section` / `apply_piano_section` / `apply_synth_section` (also used by `apply_program`), Layer Scene rules of manual p.43, undoable
- [x] GUI: preset browser walks real presets (`bay_program/preset_browse.rs`), `ProgramEdit::LoadPreset` control operation, panel mirror (`panel/preset_sync.rs`), sounds scheduled like a program's (`library/preset_load.rs`)
- [x] STORE of a section preset: "Store Preset To" with dial, PAGE / BANK, Program buttons, STORE AS, Memory Protect, both button orders (`bay_program/preset_store.rs`, `preset_user.rs`); stored presets move in Preset Organize and are written by Export backup (`library/preset_export.rs`)
- [x] Specs (`cbin-and-presets.md` 5.6, `midi-and-control.md` 5.4), D51, OQ28, OQ29 written; OQ3 removed

## Out of Scope
Blocked, written down with offsets (OQ28, OQ29): Synth sound edits and effect chains of a stored preset, the meaning of the collapsed Piano / Synth bits, hardware acceptance of an edited backup (needs a Stage 4). Also: Cancel (revert to the previous sound) and Cat sorting in the preset browser, the inverse mapping `SynthLayer` to `SynthLayerDecoded` and `LayerFxChain` to `FxDecoded` (OQ28), the header category id of a new preset (OQ26), the preset id on the display (OQ27), hardware validation.

## Summary of Changes
- `nord-core`: `cbin/checksum.rs`, `cbin/section_preset.rs`, `engine/preset_load.rs` (+ `apply_program` refactor), `engine/preset_capture.rs`, `preset_cli.rs` and `ns4 preset-check`, CRC-32 in `to_cbin_file` / `to_cbin_file_like`.
- `nord-gui`: `ProgramEdit::LoadPreset` (control op, edit counted), `panel/preset_sync.rs`, `bay_program/{preset_browse,preset_store,preset_user}.rs`, `library/{preset_load,preset_export}.rs`, hooks in `nav.rs`, `actions.rs`, `list.rs`, `store.rs`, `catalog.rs` and two one-line OLED hooks (`oled.rs`: preset names, "Store Preset To" text).
- `tools/preset_check.py` with tests.
- Specs and decisions: `cbin-and-presets.md` 5.6, `midi-and-control.md` 5.4, D51, OQ28, OQ29.

---
# ns4-r8t9
title: Preset Organize, Synth dial bindings, piano size and drawbar rows
status: completed
type: task
created_at: 2026-11-14T13:25:54Z
updated_at: 2026-11-14T13:25:54Z
parent: ns4-xc5n
---

## Context
Three Unsupported spots at the OLED / Program bay remained after the OLED work (#263, #265, #266, #267): the Preset Organize page of the Organize menu (two dimmed items), the three Synth OLED dials (one dial edits many parameters by page) and the rows of the Program OLED Layers view (piano size letter, organ mini drawbars).

## Higher Goal
Every control the manual documents does something, or its gap is written down with a reason; the Program display shows what the hardware shows where a data source exists.

## Acceptance Criteria
- [x] Preset Organize page: Swap / Move / Ok / Undo over the Preset Library (Organ, Piano, Synth) with Memory Protect gate and Undo, section chosen with Shift + PROGRAM dial (`bay_program/organize_presets.rs`)
- [x] Preset locations read from the `.ns4o` / `.ns4n` / `.ns4y` headers (`cbin/backup_presets.rs`) and written by Export backup; untouched entries byte-identical (`library/export_presets_tests.rs`)
- [x] Menu model: both Organize pages `UiOnly`, 0 menu items `Unsupported` (133 items: 111 bound, 22 UiOnly)
- [x] `synth.dial.left/middle/right` `Bound("dial:<tail>")` with a per-page table (`inventory/synth_dials.rs`), a test that every named edit and host id exists and a GUI test that turns every dial of every page and compares the emitted edit with its cell
- [x] Program OLED Layers view: piano size letter from the `.npno` header name and B3 drawbar digits from the panel state
- [x] Inventory pins (Bound 153, UiOnly 58, Unsupported 64), menu pins and specs updated from test output; no host parameter id changed

## Out of Scope
The preset id (`1:05`) in the section rows (no data source, open question OQ27, stays in `ns4-bjvn`), storing a new preset into the Preset Library, loading `.ns4o` / `.ns4n` / `.ns4y` into the engine, the SAMPLE OPT dial (needs sample preset data), hardware validation of an edited backup.

## Summary of Changes
- `nord-core`: `cbin/backup_presets.rs` (`PresetSection`, `PresetIndex`, `PresetPut`, `preset_edits`), `piano/label.rs` (`PianoLabel`, `PianoSize`).
- `nord-panel`: `menus/organize.rs` page 2 (six `UiOnly` items), `inventory/synth_dials.rs` (45 dial cells: 34 edit, 10 UI navigation, 1 documented gap), dial bindings in `inventory/synth.rs`.
- `nord-gui`: `bay_program/organize_presets.rs` (+ tests), Shift + dial section step, `library/export.rs` `plan_presets`, `SynthPage::dial_key` and `dials_table_tests.rs`, `oled_home.rs` section rows.
- `tools/preset_slots.py` and the size check in `tools/npno_types.py` with tests.
- Specs: `control-inventory.md` 9b.1, `midi-and-control.md` 5.3, `cbin-and-presets.md` 5.5, `visual-style-and-panel.md` 4.5, `piano-types.md`, OQ27.

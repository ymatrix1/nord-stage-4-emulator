---
# ns4-xhbf
title: 'Phase 6b B3: host parameter-ID stability snapshot test'
status: completed
type: task
created_at: 2026-11-12T21:36:07Z
updated_at: 2026-11-12T21:36:07Z
parent: ns4-xc5n
---

## Context
Host parameter IDs are a stable contract once shipped; Phase 6b adds many parameters.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] Committed snapshot of every param ID with type, range and default
- [x] Test fails on remove or rename, passes on additive change
- [x] Test asserts ID uniqueness

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
- `crates/nord-plugin/src/param_snapshot.rs` and `crates/nord-plugin/param-ids.snapshot` (264 rows: `id|step_count|default_normalized`).
- Test `shipped_ids_are_stable` fails on a removed or renamed id and on a changed step count or default; added ids pass. Regenerate with `UPDATE_PARAM_SNAPSHOT=1 cargo test -p nord-plugin param_snapshot`.
- Checked by hand: appending a ghost id to the snapshot made the test fail with `removed or renamed: ghost_param`; restoring it passed. Unit tests cover removal, rename, type/default change, additive change and the text round trip. Duplicate ids fail while the rows are built.
- Range is covered by step count only (continuous params have no range check).

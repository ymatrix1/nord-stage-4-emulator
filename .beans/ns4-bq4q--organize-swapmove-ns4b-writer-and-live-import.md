---
# ns4-bq4q
title: Organize Swap/Move, .ns4b writer and Live import
status: completed
type: task
created_at: 2026-11-14T03:03:22Z
updated_at: 2026-11-14T03:03:22Z
---

## Context
Programs stored or reorganized in the GUI could not be saved back into a backup, and the Organize menu was a stub (Unsupported).

## Higher Goal
Round-trip a Nord Stage 4 backup: read, modify programs and Live slots, write a new .ns4b the hardware tooling can use.

## Acceptance Criteria
- [x] .ns4b writer keeps untouched entries byte-identical (real 3.14 GB archive: no-edit and replace-with-own-bytes copies are cmp-identical)
- [x] ns4 backup-store CLI
- [x] nord_core::library_ops swap/move/undo with boundary tests
- [x] Organize page (Swap/Move/Ok/Undo, Memory Protect gate), Export backup and Load Live slots in the library bar
- [x] Menu bindings and pins updated; specs updated
- [x] .ns4l import into Live slots with a test

## Out of Scope
Preset Organize (no in-memory preset bank), hardware validation of an edited backup, ZIP64 edits.

## Summary of Changes
See docs/specs/cbin-and-presets.md section 5.5 and docs/specs/midi-and-control.md section 5.2.

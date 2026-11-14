---
# ns4-qdzu
title: 'STORE flow details: default name suffix, program categories'
status: completed
type: task
created_at: 2026-11-14T03:34:18Z
updated_at: 2026-11-14T03:34:18Z
---

## Context
Store As on hardware proposes the current name with a number suffix (NS4 Palette -> NS4 Palette 2) and shows categories such as Piano Acoustic (piano-pad video 5:40-5:47). Our CATEGORIES list is ASSUMED and differs; need the real list. See docs/research/dsoundman-course-findings.md.

## Higher Goal
Match hardware display behaviour.

## Acceptance Criteria
- [x] STORE AS proposes `Name 2` (numbered) when the name exists in the library or opened archive
- [x] Category list is the 26 firmware categories (already adopted, VERIFIED provenance)
- [x] Behaviour implemented with tests
- [x] Spec and provenance tags updated

## Out of Scope
Engine changes unrelated to the display.

## Summary of Changes
- `name.rs`: pure `suggest_name(name, taken)` with unit tests.
- `store.rs`: `UserLibrary::name_exists`, `StoreFlow::begin_as` takes a `taken` predicate.
- `actions.rs`: `store_as` uses both and presets the category of the program at the current location.
- Spec `midi-and-control.md` and `dsoundman-course-findings.md` updated.

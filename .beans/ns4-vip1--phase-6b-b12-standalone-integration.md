---
# ns4-vip1
title: 'Phase 6b B12: standalone integration'
status: todo
type: task
created_at: 2026-11-12T21:36:11Z
updated_at: 2026-11-12T21:36:11Z
parent: ns4-xc5n
blocked_by:
    - ns4-q6p7
    - ns4-0sud
    - ns4-9h25
    - ns4-otuf
    - ns4-z6fi
    - ns4-irgv
    - ns4-hcuh
    - ns4-jpgx
---

## Context
nord-gui becomes a thin shell over nord-panel.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] PanelHost over AudioHandle
- [x] Panorama view (strip + draggable viewport, `P`), focused rack view (`Z`, `1`-`5`), library drawer (slides in, `L`); headless-tested, not seen on a real display

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes

- Panorama (2026-11-14): `panel/panorama.rs` adds `ViewState::panorama` (viewport rectangle in logical panel px) and a 56 px strip under the detailed view: drag or click pans, wheel and `+` / `-` zoom, arrows pan, `P` toggles, `Esc` leaves, `Z` / `1`-`5` / section header focus a section. `view.rs` now works on a region instead of focus only.
- Drawer: the library was already a docked `SidePanel`; it now slides (`show_animated`) and `L` toggles it. Library logic untouched.
- Tests: `panel/panorama_tests.rs` (keys, strip drag, wheel, click-through, default frame unchanged, frame-time probe) on `Sim::full_panel()`, `library/ui.rs` (slide, click-through, `L`), view math in `panorama.rs`.
- `--screenshot` unchanged (panorama off, drawer not drawn). No view persistence added: the app has no eframe storage.
- Still open for B12 in the roadmap: PanelHost over AudioHandle; the strip is a section-outline map, not a live thumbnail.

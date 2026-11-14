---
# ns4-yfht
title: 'Phase 6b: menu model and engine-audible global settings'
status: completed
type: task
priority: normal
created_at: 2026-11-14T00:02:25Z
updated_at: 2026-11-14T00:25:07Z
parent: ns4-xc5n
---

## Context
Manual ch.11 menus (p.57-63) have no data model. Engine-audible Sound/System/Output values (key click, string res, pedal noise, rotary speeds, tonewheel mode, global transpose, fine tune, mono) have no engine fields or host params.

## Higher Goal
Every menu page and item exists as toolkit-free data in nord-panel with a binding, and every setting the engine can honour is audible and host-automatable.

## Acceptance Criteria
- [x] nord-panel menus/ data module with tests (unique ids, pages 1..n, defaults in range, beans exist)
- [x] nord-core GlobalSettings, defaults equal to current hard-coded values (render bit-identical)
- [x] nord-plugin sys_* params, snapshot regenerated additively
- [x] Specs updated

## Out of Scope
- nord-gui menu rendering
- Per-program bus routing, MIDI CC/NRPN filters, organ trigger point, LED intensity

## Summary of Changes
- nord-panel `menus/`: 58 pages, 129 items (56 bound, 12 UiOnly, 61 Unsupported -> ns4-zb9o)
- nord-core `engine/settings.rs` + `organ/globals.rs`: global transpose, fine tune, piano pedal-noise/string-res dB, tonewheel mode, click level, rotary balance/speeds/accelerations, mono; defaults bit-identical
- nord-plugin `settings_params.rs`: 12 additive `sys_*` ids
- Specs: midi-and-control section 5, HOST_COMPATIBILITY, daw-plugin

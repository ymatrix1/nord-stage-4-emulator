---
# ns4-uzg7
title: Label legibility audit
status: in-progress
type: task
created_at: 2026-11-14T11:56:17Z
updated_at: 2026-11-14T11:56:17Z
parent: ns4-xc5n
---

## Context
The panel labels are cramped, glued together or clipped (Rotary Speaker box wider than its frame, OLED soft labels clipped, SOLO under the OLED, effects captions touching knobs and bezels, duplicate VIBRATO). The earlier audit only checked label/label and label/control overlap, so these passed.

## Higher Goal
B8 section renderers readable at the panel scale; every label shows next to the control it names, in the same place as on the instrument where the print allows it.

## Acceptance Criteria
- [x] Headless audit also records clip rectangles, section edges, group frames, knob rings and requires a 1 px gap
- [x] Audit prints its failures; allow-list shrinks to empty (30 before, 0 after)
- [x] Shared fit-aware label helpers (fit_size, label_fit, tag_fit, measured shift legend LED)
- [x] Rotary box widened leftwards, effects captions, OLED soft labels, SOLO, nudges fixed
- [x] Unit tests for every touched file; docs and ROADMAP B8 note updated
- [x] Before/after screenshots inspected per section

## Out of Scope
Golden images (B6), 2.5D look (B7), fader-cap versus drawbar-number overlap, focus-view specific layouts.

## Summary of Changes
See the PR (fix/label-legibility).

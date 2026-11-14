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

## Round 2 (fix/label-legibility-2)

### Context
The user still saw cramped parts after round 1: drawbar numbers (8', 16') hidden by the fader caps, LEDs close to the Rotary frame (ANGLE / CLOSE MIC), LAYER INIT over MON/COPY, a layer-strip LED over the meter without a name.

### Higher Goal
The audit catches what the eye catches, and the panel is compared with the real photos section by section.

### Acceptance Criteria
- [x] Rules F (fader cap travel), E (LED to frame), T (label to LED), V (stacked gap), M (knob caption distance) plus bezel 2.5 px; findings before, 0 after
- [x] Findings fixed (drawbar numbers, Shift legend LEDs before the text, line pitches, Program, Piano, rotary speed row, nudges)
- [x] FX FOCUS captions and amp RES caption added
- [x] Compared with all photos; remaining differences recorded in `docs/specs/visual-style-and-panel.md`
- [x] Docs (`gui-testing.md`, visual-style spec) updated

### Out of Scope
Group frames and scale numerals of the photos, per-drawbar names, a visible Piano Select box.

### Summary of Changes
`panel/legibility_caps.rs` (rules), `widgets/probe.rs` (cap travel and LED records), fixes in `ctl.rs`, `organ_drawbars.rs`, `bay_synth/controls.rs`, `bay_piano_select.rs`, `piano_widgets.rs`, `bay_master/rotary.rs`, `bay_effects/{ctx,amp}.rs`, `layout_tweaks.rs`.

## Out of Scope
Golden images (B6), 2.5D look (B7), fader-cap versus drawbar-number overlap, focus-view specific layouts.

## Summary of Changes
See the PR (fix/label-legibility).

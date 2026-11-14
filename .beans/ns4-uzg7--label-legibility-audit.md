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

## Round 3 (fix/panel-titles-frames)

### Context
A user cannot find or understand a control whose name, frame or scale the real panel prints and the emulator omitted: Piano ACOUSTICS / UNISON / KB TOUCH / DYN COMP titles and the PIANO SELECT box, TIMBRE listed in the wrong order, per-drawbar names and 1-8 numerals, the Synth and Effects boxes and 0-10 scales, the names of the three OLED dials, the Program-area boxes, the split LED letters, and dim legends (grey 150 on red) below a readable contrast.

### Higher Goal
Every control can be identified from what is printed on it, as on the instrument, without hovering; the legibility audit still passes with an empty allow-list.

### Acceptance Criteria
- [x] Frame table in `nord-panel` (`layout/frames.rs`, 29 frames: members plus pad, tests, control inventory section 7) and a painter in `nord-gui` (`panel/frames.rs`) that records boxes for rule `G`
- [x] Piano: four titles, always-visible PIANO SELECT box with type names, TIMBRE order BRIGHT, MID, SOFT (+ DYNO1/2)
- [x] Organ: ORGAN MODEL, VIB/CHORUS, B3 PERCUSSION frames, drawbar register names and 1-8 numerals
- [x] Synth: nine boxes, 0-10 scale ends (`widgets/scale.rs`), INFO / LIST / LIST under the dials; Effects: six boxes and scale ends
- [x] Program: six boxes; keybed L / M / H letters
- [x] Dimmed legends at least 4.5:1 against slate and red (`style::dim_legend`, tested)
- [x] Audit: 0 before (main), 28 on the first run with the new content (64 and 43 on later runs), 0 after; allow-list empty
- [x] Docs: visual-style spec (round 3 section), gui-testing, control inventory section 7, ROADMAP B8

### Out of Scope
Variation lists beside the effects rockers, numbered LEDs on UNISON / DYN COMP, the 1-9 numerals all around each knob, the SAMPLES / ANALOG and MONO / LEGATO LED names, golden images.

### Summary of Changes
`nord-panel/src/layout/frames.rs`; `nord-gui`: `panel/frames.rs`, `panel/bay_piano_names.rs`, `widgets/scale.rs`, `panel/layout_tweaks.rs` (group nudges), `style.rs` (contrast), edits in the organ, piano, synth, effects, program bays and the keybed.


## Round 4 (fix/panel-numerals-variations)

### Context
The real panel prints numerals around each knob, the variation names beside the Effects selectors and numbers on the Piano LED rows; without them a user cannot read a control's meaning.

### Higher Goal
Every control can be read from what is printed on it; the legibility audit stays at 0 with an empty allow-list.

### Acceptance Criteria
- [x] Numerals around every Synth, Layer Effects and Rotary DRIVE knob (0-10, -15..15, -10..10, 200..8K); none where the photos print none
- [x] Variation lists beside the Mod 1, Mod 2, Amp, Delay and Reverb selectors with the manual names
- [x] 1 2 3 under UNISON / DYN COMP LEDs, H M L under KB TOUCH
- [x] Program box clear of the PAGE / BANK buttons
- [ ] SAMPLES / ANALOG and MONO / LEGATO LED names, PROGRAM title position, Rotary title tab (not done)
- [x] Audit 0 before, 116 first run, 0 after; allow-list empty; docs and ROADMAP B8 updated; `bay_synth/widgets.rs` split (`pots.rs`)

### Out of Scope
OLED files, golden images, the `C2 V3 C3` vibrato matrix.

### Summary of Changes
`widgets/scale.rs`, `widgets/knob.rs`, `bay_effects/{lists,ctx,ids,amp,delay,modulation,reverb}.rs`, `bay_synth/pots.rs` (split from `widgets.rs`), `bay_master/rotary.rs`, `bay_piano_select.rs`, `layout_tweaks.rs`, `nord-panel` frame pads, `legibility_tests.rs` (own-numeral exemption).

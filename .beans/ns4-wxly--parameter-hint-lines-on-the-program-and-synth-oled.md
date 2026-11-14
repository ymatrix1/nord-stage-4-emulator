---
# ns4-wxly
title: Parameter hint lines on the Program and Synth OLEDs
status: completed
type: task
created_at: 2026-11-14T03:34:14Z
updated_at: 2026-11-14T12:00:00Z
---

## Context
Hardware shows hints while a control moves: synth OLED top row becomes LFO Rate 0.08 Hz / LFO Destination OscPitch / LFO Amount 7.1 (LFO video 0:27-0:45); Program display lower half shows hints (manual p.42). Not implemented in either OLED. See docs/research/dsoundman-course-findings.md and docs/research/oled-strings-vs-firmware.md.

## Higher Goal
Match hardware display behaviour.

## Acceptance Criteria
- [x] Data-driven table of inventory control id -> label + value formatter (Hz, dB, ms, %, bpm, panel units) in `panel/param_hint/`
- [x] Every `widgets::knob` drag raises the hint; Master Clock divisions, stored layer values and per-type mod rates refine it
- [x] Synth OLED header row shows the hint; Program OLED home screen shows it in the lower half
- [x] LFO DEST taps show `LFO Destination <name>`
- [x] Formatter constants tagged VERIFIED / EMPIRICAL / ASSUMED; firmware labels checked by `test_param_hint_labels_are_in_the_firmware`
- [x] Unit tests (formatters, table vs inventory, course-video readings, knob drag, synth pot hints, Program OLED); spec updated (`visual-style-and-panel.md` 4.5)

## Out of Scope
Engine changes unrelated to the display. Faders, drawbars, wheels and buttons other than LFO DEST. Replacing the emulator's LFO knob law by the hardware 128-step rate table. Measuring the hint hold time on hardware.

## Summary of Changes
- New `crates/nord-gui/src/panel/param_hint/` (`mod.rs` touch store and `active`, `fmt.rs` formatters, `table.rs` 28 rows plus shared ranges).
- `widgets::knob` records the touch; Synth pots, delay, modulation and compressor knobs refine the value text; Master knobs now use their inventory ids.
- Synth OLED paints the hint over the page title; Program OLED `oled_hint::apply` replaces the home rows.
- `fmt_time`, EQ and delay ranges now come from `param_hint` (one definition each).
- Python test pins the VERIFIED labels to the firmware pool; spec and findings updated.

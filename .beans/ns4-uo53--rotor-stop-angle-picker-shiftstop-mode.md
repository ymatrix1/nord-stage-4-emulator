---
# ns4-uo53
title: Rotor stop ANGLE picker (Shift+Stop Mode)
status: completed
type: task
created_at: 2026-11-14T03:34:17Z
updated_at: 2026-11-14T03:34:17Z
---

## Context
Hardware screen ROTOR STOP ANGLE / Position with Free, 1: Front Center, 2: Front Left, 4: Back Left, 6: Back Center (organ video 10:01-10:25). Neither the picker nor a per-angle brake exists in nord-core rotary. See docs/research/dsoundman-course-findings.md.

## Higher Goal
Match hardware display and sound behaviour: the rotor always stops at the picked angle (manual p.53).

## Acceptance Criteria
- [x] Engine: `OrganLayer::rotary_stop_angle` (Free, 1..6); horn and drum settle at the angle in Stop Mode, no allocation, deterministic test
- [x] Host param `organy_stop_angle_{1,2}`, additive; snapshot regenerated with no existing id changed
- [x] `OrganEdit::RotaryStopAngle` and its host mapping
- [x] Picker popup on Shift + STOP MODE listing `Free`, `1: Front Center` .. `6: Back Center`
- [x] Inventory entry `rotary.stop_angle`, pinned counts and `control-inventory.md` updated
- [x] Specs and provenance tags updated (`effects-and-routing.md` 2.7b)

## Out of Scope
Firmware Leslie path (`leslie_fw`) rest phase; measuring the real rest phases on hardware; per-layer pickers; OLED rendering inside the Program screen.

## Summary of Changes
- `nord-core`: `organ/rotor_stop.rs` (labels, rest phases, `ease_phase`); `rotary.rs` eases horn and drum phase to the angle in Stop Mode (Free no longer pulls to phase 0); setting mirrored to the TO ROTARY bus.
- `nord-plugin`: `organy_stop_angle_{n}` int param, edit mapping.
- `nord-gui`: `OrganEdit::RotaryStopAngle`, `panel/rotor_angle.rs` popup (also holds the former wide/narrow mic toggle), Shift + STOP MODE opens it.
- `nord-panel`: inventory `rotary.stop_angle` (275 controls), layout shared rect.
- Rest phases and entries 3 and 5 are ASSUMED.

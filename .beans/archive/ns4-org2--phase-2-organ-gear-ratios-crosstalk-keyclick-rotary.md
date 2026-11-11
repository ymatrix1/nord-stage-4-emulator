---
# ns4-org2
title: 'Phase 2: Complete Hammond Gear Ratios, Crosstalk, Key-Click Bandpass & Rotary Mic/Cabinet DSP'
status: completed
type: task
created_at: 2026-11-11T16:04:00Z
updated_at: 2026-11-11T16:07:00Z
---

## Context
Parent epic: `ns4-orgn` (Phase 2: Organ Engine DSP). `crates/nord-core/src/organ.rs` currently computes tonewheel frequencies using ideal equal temperament (`2^(n/12)`), adds an unfiltered scalar `click_env` directly into the dry sum, has no tonewheel transformer crosstalk matrix or `B3` vs `B3 Bass` pickup calibration curve, and implements Rotary Speaker amplitude panning without Doppler delay modulation, `Close Mic`, `Angle` spread, or cabinet early reflections. Meanwhile `OrganPresetDecoded` already decodes `layer_a_octave` and `layer_b_octave` which `Stage4Engine::apply_program` does not forward to `OrganLayer`.

## Higher Goal
Complete all remaining Phase 2 (`DSPO`) Organ Engine DSP milestones in `crates/nord-core/src/organ.rs` with zero heap allocations inside `render_frame()`, bringing Hammond B3, key click, and Model 122 Rotary Speaker audio accuracy in line with `docs/specs/organ-engine.md` and `docs/ROADMAP.md`.

## Acceptance Criteria
- [x] Replace equal-temperament tonewheel tuning with the exact Hammond 12-note integer drive/driven gear ratio table (`HAMMOND_GEAR_RATIOS`, 20 Hz synchronous shaft, 192-tooth top octave step-up) and per-tonewheel pickup calibration curves for `B3` vs `B3 Bass` mode
- [x] Implement tonewheel transformer bin crosstalk matrix (`-42 dB` generator bin bleed) and 1.8 kHz resonant bandpass palladium contact-bounce key-click synthesis
- [x] Implement Model 122 Rotary Speaker Doppler delay-line modulation, `Close Mic` / `Normal` distance voicing, configurable mic `Angle` spread, `Stop` mode braking, and early cabinet reflection taps with zero heap allocations in `render_frame()`
- [x] Add `octave_shift` support on `OrganLayer` and wire `layer_a_octave` / `layer_b_octave` in `Stage4Engine::apply_program`
- [x] Update `docs/specs/organ-engine.md` and `docs/ROADMAP.md` checkboxes in sync and pass all Cargo + Python + `tools/check_tests.py` checks

## Out of Scope
- Modifying `.ns4o` / `.ns4p` `CBIN` bitstream offsets (`ns4-bnct`, already complete)
- `crates/nord-plugin` `nih-plug` wrapper (`ns4-707f`)

## Summary of Changes
- Replaced equal-temperament tonewheel initialization in `crates/nord-core/src/organ.rs` with `HAMMOND_GEAR_RATIOS`, 20 Hz synchronous motor shaft, and 192-tooth top-octave step-up (`tonewheel_gear_spec` / `tonewheel_gear_frequency`).
- Added `b3_bass_mode` pickup calibration curves (`pickup_calibration`), bottom-octave complex-wheel harmonics, and `-42 dB` transformer bin crosstalk (`render_tonewheels`).
- Implemented zero-DC palladium contact-bounce key-click impulses through a 2-pole SVF bandpass filter (`process_key_click`), plus Model 122 Doppler delay modulation, `rotary_close_mic`, `rotary_mic_angle_deg`, `rotary_stop_mode`, and cabinet early reflections (`process_rotary`).
- Added `octave_shift` on `OrganLayer` and wired `layer_a_octave` / `layer_b_octave` in `Stage4Engine::apply_program`.
- Updated `docs/specs/organ-engine.md` and `docs/ROADMAP.md` (all Phase 2 items checked `[x]`).

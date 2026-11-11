---
# ns4-plsh
title: 'Polish .npno/.nsmp4 DSP playback, reverb diffusion, docked library panel, and non-motorized knob option'
status: completed
type: task
parent: ns4-trkb
created_at: 2026-11-11T19:53:00Z
updated_at: 2026-11-11T20:01:00Z
---

## Context
User reported three UX and audio-quality issues while auditioning `White Grand.ns4p`:
1. The floating library dialog sat directly in front of the Organ/Piano panel controls.
2. Piano/sample playback sounded unpolished compared to hardware (upper-key detuning, premature note choke, ignored damper-down recordings, linear upsampling, and dry/metallic FDN reverb).
3. Confusion over whether preset loading should rotate physical knobs on the front panel.

## Higher Goal
Match Nord Stage 4 hardware audio quality on `.npno` and `.nsmp4` banks, keep the panel unobstructed when browsing `.ns4b`, and support authentic non-motorized knob behavior.

## Acceptance Criteria
- [x] Fix `.npno` `tuning_cents` table byte offset (`0x01C8..0x0248` -> `0x01B8..0x0238`) so upper keys read stretch tuning instead of `+50` cents from `velocity_tiers`
- [x] Preserve natural `.npno` sustain (`v.env = 1.0` while held), select `NPNO_STROKE_DAMPER` (`kind = 1`) when sustain pedal is pressed, balance velocity scaling/headroom, and upgrade `.npno`/`.nsmp4` interpolation to 4-point cubic Catmull-Rom
- [x] Add 2-stage stereo allpass input diffusion and stereo excitation to `LayerFxChain::process_reverb`
- [x] Dock `.ns4b` archive browser as `egui::SidePanel::left` before `CentralPanel`, fix `fx_focus` on preset load, and add `Sync Knobs on Preset` option
- [x] Update specs/research docs and pass all 7 CI checks

## Out of Scope
- Custom OLED font atlas or hardware USB HID knob sync

## Summary of Changes
- Fixed `tuning_cents` table offset (`0x01B8..0x0238`) in `crates/nord-core/src/cbin/piano_sample.rs` with upper-key test guard.
- Added `damper_down` (`NPNO_STROKE_DAMPER`) selection, `has_velocity_layers` intra-tier dynamics, full held sustain (`v.env = 1.0`) + `1.15s` key-off release tail, and 4-point cubic Catmull-Rom interpolation in `crates/nord-core/src/piano.rs` and `crates/nord-core/src/synth.rs`.
- Added 2-stage stereo allpass input diffusion and stereo FDN excitation in `crates/nord-core/src/fx.rs`.
- Converted `BackupLibraryState::render_drawer` to a docked `egui::SidePanel::left` before `CentralPanel` (`crates/nord-gui/src/library.rs`, `main.rs`), fixed `fx_focus` selection on preset load, and added `sync_knobs_on_program_load` (`crates/nord-gui/src/panel.rs`).
- Updated `docs/research/nsmp4-and-npno-cnsp-codec.md`, `docs/specs/cbin-and-presets.md`, `docs/specs/piano-engine.md`, `docs/specs/effects-and-routing.md`, and `docs/specs/visual-style-and-panel.md`.

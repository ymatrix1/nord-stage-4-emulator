---
# ns4-pn4c
title: 'Phase 4: Piano SFZ/WAV Loader, CC64/66/67 Pedal Noise, Unison & Dyn Comp DSP'
status: completed
type: task
created_at: 2026-11-11T16:15:00Z
updated_at: 2026-11-11T19:16:00Z
---

## Context
Parent epic: `ns4-pno4` (Phase 4: Piano Engine DSP). `crates/nord-core/src/piano.rs` previously implemented `NpnoSound` 4-bit DPCM playback, 12-band modal sympathetic string resonance, `SOFT REL` felt damper envelopes, and basic CC64 pedal thump. Open SFZ/WAV multi-velocity and key-off release loading, Sostenuto (`CC66`) & Una Corda (`CC67`) pedal mechanics/noise, Piano `UNISON` (`1..=3`), and `DYN COMP` (`1..=3`) remained to be completed and wired from `Stage4Engine`.

## Higher Goal
Complete all remaining Phase 4 (`DSPF`) Piano Engine DSP items in `crates/nord-core/src/piano.rs` and `crates/nord-core/src/engine.rs` with strict real-time audio safety (zero heap allocations or locks inside `render_frame()`).

## Acceptance Criteria
- [x] Implement RAM-cached open SFZ + clean-room RIFF/WAV (16/24/32-bit mono/stereo) multisample loader (`NpnoSound::from_sfz_str` / `from_sfz_path`, `parse_wav_mono`) with multi-velocity layer zones and key-off release sample triggers (`trigger=release`)
- [x] Implement velocity-sensitive Dynamic Pedal Noise (`PED NOISE` thump + felt brush exciting sympathetic resonators) and authentic pedal mechanics for Damper (`CC64`), Sostenuto (`CC66`), and Una Corda / Soft (`CC67`), wired in `Stage4Engine::handle_midi`
- [x] Implement Piano `UNISON` (`0..=3` stereo hammer/string detune spread), `DYN COMP` (`0..=3` dynamic range compression), `octave_shift`, and distinct `Soft`/`Mid`/`Bright`/`Dyno1`/`Dyno2` Timbre spectral shaping, wired from `PianoLayerDecoded` in `Stage4Engine::apply_program`
- [x] Add deterministic unit tests for SFZ/WAV parsing & multi-velocity/release zones, CC64/66/67 pedal mechanics & noise, Unison stereo spread, and Dyn Comp dynamic leveling
- [x] Update `docs/specs/piano-engine.md` and `docs/ROADMAP.md` checkboxes in sync and pass all Cargo + Python + `tools/check_tests.py` checks

## Out of Scope
- Decoding `.nsmp4` `CNSP` audio frames (`ns4-trkb`)
- `crates/nord-plugin` `nih-plug` wrapper (`ns4-707f`)

## Summary of Changes
- `crates/nord-core/src/piano.rs`: Added `NpnoSound::from_sfz_str`, `NpnoSound::from_sfz_path`, `parse_wav_mono` (PCM 8/16/24-bit & IEEE float 32-bit mono/stereo), `PianoSampleZone` multi-velocity & `trigger=release` zone selection, `set_damper_pedal`/`set_sostenuto_pedal`/`set_soft_pedal` with velocity-sensitive thump + felt brush noise exciting sympathetic resonators, `unison` stereo detune, `dyn_comp` velocity/output compression, `octave_shift`, distinct `PianoTimbre` spectral filters, and unit tests (`test_sfz_wav_multivelocity_and_release_loader`, `test_sostenuto_soft_pedal_noise_unison_and_dyn_comp`).
- `crates/nord-core/src/engine.rs`: Wired `cc::SOSTENUTO_PEDAL` (`CC66`), `cc::SOFT_PEDAL` (`CC67`), and `PianoLayerDecoded` (`unison`, `dyn_comp`, `octave`).
- `docs/specs/piano-engine.md` & `docs/ROADMAP.md`: Updated specs and checked off Phase 4 checkboxes.
- Verified with full workspace CI suite (`cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` 78+5+18+3=104 passed, `ruff`, `pytest` 14 passed, `tools/check_tests.py` 24/24 covered).

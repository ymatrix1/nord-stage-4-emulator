---
# ns4-wav2
title: 'Phase 3: Complete 64-Bank Mipmapped Wavetables, 2/3-Op FM Router, and SAMPLES Player in SynthEngine'
status: completed
type: task
created_at: 2026-11-11T15:52:00Z
updated_at: 2026-11-11T15:55:00Z
---

## Context
`crates/nord-core/src/synth.rs` currently implements `PolyBLEP` VA waveforms (`Saw`, `SuperSaw`, `Pulse`, `Triangle`, `Sine`) and `ZdfFilter`, while `SynthOscMode::Wavetable` uses a hardcoded 2-wave sine sum, `SynthOscMode::Fm` uses a fixed 2:1 sine modulator without feedback or 3rd operator, and `SynthOscMode::Sample` falls back to `SynthOscMode::Analog`. Meanwhile `SynthLayerDecoded` already decodes `wave_id`, `osc_cat`, and `filter_env_amount` from `.ns4p`/`.ns4y` files, which `Stage4Engine::apply_synth_layer` does not yet forward to `SynthLayer`.

## Higher Goal
Deliver the complete Phase 3 (`DSPS` Nord Wave 2 derived) Synth oscillator suite in `crates/nord-core` with strict real-time safety (zero heap allocations or locks inside `render_frame()`), closing all three unchecked Synth DSP items in `docs/ROADMAP.md`.

## Acceptance Criteria
- [x] Implement 64 band-limited mipmapped single-cycle wavetables (`wavetable_index: 0..=63`) with octave mipmap anti-aliasing and `wavetable_position` spectral formant shaping in `crates/nord-core/src/synth.rs`
- [x] Implement 2/3-operator harmonic & inharmonic `FM` router (`fm_ratio`, `fm_feedback`, `fm_sub_ratio`, `fm_sub_amount`, `fm_env_amount`) in `crates/nord-core/src/synth.rs`
- [x] Implement lock-free zero-allocation `SAMPLES` multisample player (`SynthSampleSound`, `LoadedSynthSample`, loop crossfade, `SampleTimbre` `Off`/`Soft`/`Bright` damping) in `crates/nord-core/src/synth.rs`
- [x] Wire `wavetable_index`, FM parameters, `filter_env_amount`, and `Sample` timbre in `Stage4Engine::apply_synth_layer` (`crates/nord-core/src/engine.rs`) without breaking existing `Stage4Engine` or `CBIN` tests
- [x] Update `docs/specs/synth-engine.md` and `docs/ROADMAP.md` checkboxes in sync and pass all Cargo + Python + `tools/check_tests.py` checks

## Out of Scope
- Decoding proprietary `.nsmp4` compressed `CNSP` audio frames (`ns4-trkb`, tracked separately)
- `crates/nord-plugin` `nih-plug` wrapper (`ns4-707f`)

## Summary of Changes
- Implemented `WavetableBank` (`OnceLock`, pre-warmed in `SynthLayer::new`) with 64 Lanczos-smoothed Fourier wavetables across 8 octave mipmap levels (`MIP_MAX_HARMONICS`) and continuous spectral formant scan (`wavetable_position`).
- Implemented 2/3-operator harmonic/inharmonic FM router (`FM_RATIOS`, `fm_ratio`, `fm_feedback`, `fm_sub_ratio`, `fm_sub_amount`, `fm_env_amount`) plus per-voice `mod_env` modulation of filter cutoff (`filter_env_amount`).
- Implemented zero-allocation `SynthOscMode::Sample` zone playback (`SynthSampleSound`, `LoadedSynthSample`, pitch transposition, linear loop crossfade, and `SampleTimbre` `Off`/`Soft`/`Bright` 1-pole damping).
- Wired `Stage4Engine::apply_synth_layer` to forward `wave_id`, `osc_cat`, and `filter_env_amount`, added 4 unit tests, and updated `docs/specs/synth-engine.md` and `docs/ROADMAP.md`.

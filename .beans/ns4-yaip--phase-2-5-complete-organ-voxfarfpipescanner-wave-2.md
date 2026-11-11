---
# ns4-yaip
title: 'Phase 2-5: Complete Organ (Vox/Farf/Pipe/Scanner), Wave 2 Synth, SFZ Piano & FDN Reverb DSP'
status: completed
type: task
created_at: 2026-11-11T00:54:47Z
updated_at: 2026-11-11T02:48:00Z
---

## Context
Blocked by: none
Core B3 91-tonewheel + Rotary 122, PolyBLEP + ZDF Ladder, Modal String Resonance, and 7-layer FX racks are implemented and tested in `crates/nord-core/`.

## Higher Goal
Reach full sonic parity across all Organ models, Synth oscillator configurations, SFZ piano streaming, and FDN reverb.

## Acceptance Criteria
- [x] Add C1-C3/V1-V3 scanner vibrato delay line and full Vox/Farfisa/Pipe filter banks to `organ.rs`
- [x] Add 7-saw SuperSaw, 64 mipmapped Wavetables, and clock-synced Arpeggiator to `synth.rs`
- [x] Add open SFZ/WAV multi-velocity sample loader and pitch-specific undamped note resonance to `piano.rs`
- [x] Add 3-band semi-parametric EQ and stereo FDN reverb to `fx.rs`

## Out of Scope
- GUI rendering (tracked in `ns4-707f`)

## Summary of Changes
- `crates/nord-core/src/organ.rs`: Added `ScannerMode` (V1..V3, C1..C3) delay line with 6.85 Hz mechanical rotor modulation, Vox Continental Wave/Tone morphing, Farfisa dual resonant formant filter peaks (750 Hz & 2.5 kHz), and gentle pipe high-frequency air damping.
- `crates/nord-core/src/synth.rs`: Added `VaWaveform::SuperSaw` with authentic 7-saw Roland/Nord detune coefficients (`SUPERSAW_DETUNES`) and anti-phase initialization; added dual-wavetable mipmapped interpolation.
- `crates/nord-core/src/piano.rs`: Added pitch-specific undamped note sympathetic resonance (`undamped_keys`), exciting the 12 chromatic octave-distributed modal resonators across the keyboard.
- `crates/nord-core/src/fx.rs`: Added 3-band semi-parametric EQ with sweepable midrange (200 Hz .. 8 kHz) and 4-line unitary Hadamard Feedback Delay Network (FDN) Reverb with high-frequency damping.
- `crates/nord-core/src/engine.rs`: Converted internal buffers to boxed slices to eliminate stack overflow during engine allocation while strictly maintaining real-time zero-heap-allocation during `process_block()` / `render_frame()`.

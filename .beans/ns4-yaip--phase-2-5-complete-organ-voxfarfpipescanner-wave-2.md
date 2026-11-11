---
# ns4-yaip
title: 'Phase 2-5: Complete Organ (Vox/Farf/Pipe/Scanner), Wave 2 Synth, SFZ Piano & FDN Reverb DSP'
status: todo
type: task
created_at: 2026-11-11T00:54:47Z
updated_at: 2026-11-11T01:25:00Z
---

## Context
Blocked by: none
Core B3 91-tonewheel + Rotary 122, PolyBLEP + ZDF Ladder, Modal String Resonance, and 7-layer FX racks are implemented and tested in `crates/nord-core/`.

## Higher Goal
Reach full sonic parity across all Organ models, Synth oscillator configurations, SFZ piano streaming, and FDN reverb.

## Acceptance Criteria
- [ ] Add C1-C3/V1-V3 scanner vibrato delay line and full Vox/Farfisa/Pipe filter banks to `organ.rs`
- [ ] Add 7-saw SuperSaw, 64 mipmapped Wavetables, and clock-synced Arpeggiator to `synth.rs`
- [ ] Add open SFZ/WAV multi-velocity sample loader and pitch-specific undamped note resonance to `piano.rs`
- [ ] Add 3-band semi-parametric EQ and stereo FDN reverb to `fx.rs`

## Out of Scope
- GUI rendering (tracked in `ns4-707f`)

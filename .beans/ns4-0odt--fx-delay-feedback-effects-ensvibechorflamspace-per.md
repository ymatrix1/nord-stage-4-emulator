---
# ns4-0odt
title: 'FX: delay feedback effects (ENS/VIBE/CHOR/FLAM/SPACE) per manual; stereo chorus in delay path'
status: todo
type: task
created_at: 2026-11-12T03:26:33Z
updated_at: 2026-11-12T03:26:33Z
parent: ns4-trkb
---

## Context
Manual p.50-51: feedback effects ENS/VIBE/CHOR = Mod 2 counterparts (CHOR variation = vintage pedal, lush stereo widening); FLAM = ping-pong with 20/40 ms L/R offset; SPACE = diffusion 50/100% wet. Our delay has one modulated-feedback mode. Hardware Jazz Guitar II width 0.96 vs emulator 0.16 (docs/research/hardware-audio-reference-fx.md).

## Acceptance Criteria
- [ ] Stereo decorrelating chorus in the delay feedback path (CHOR/VIBE/ENS)
- [ ] FLAM and SPACE implemented per manual
- [ ] Band-wise width vs hardware clip improves (coherence at 1-2 kHz)
- [ ] Constants tagged per provenance rule

## Out of Scope
Exact SHARC algorithms (not in the firmware image).

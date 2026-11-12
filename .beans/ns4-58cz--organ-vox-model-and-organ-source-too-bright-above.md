---
# ns4-58cz
title: 'Organ: Vox model (and organ source) too bright above 3 kHz vs hardware clip'
status: todo
type: task
created_at: 2026-11-12T03:49:33Z
updated_at: 2026-11-12T03:49:33Z
parent: ns4-trkb
---

## Context
A:84 Vintage Vox J3 (organ Vox model, Twin amp): with the amp switched off the emulator is +4.3 dB (3..8 kHz) and +3.2 dB (8..20 kHz) above the hardware recording that includes the Twin (docs/research/hardware-audio-reference-fx.md). The manual says Twin only adds treble, so the organ source itself is too bright by at least that much.

## Acceptance Criteria
- [ ] Confirm on other organ clips (Tiny Transistor, B3 programs)
- [ ] Find the cause in organ.rs (Vox waveform harmonics / percussion / key click)
- [ ] Re-run A:84 comparison after the fix

## Out of Scope
Cabinet voicings.

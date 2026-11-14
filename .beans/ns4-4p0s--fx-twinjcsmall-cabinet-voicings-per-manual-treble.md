---
# ns4-4p0s
title: 'FX: Twin/JC/Small cabinet voicings per manual (treble boost, variations) and TO ROTARY routing'
status: todo
type: task
priority: normal
created_at: 2026-11-12T03:26:34Z
updated_at: 2026-11-12T03:49:34Z
parent: ns4-trkb
---

## Context
Manual p.51-52: Twin and JC boost treble (non-variation especially); variations reduce treble. Our voicings are low-pass cabinets (ASSUMED) and the emulator is 3-5.5 dB brighter than hardware above 3 kHz on Twin/Small programs. TO ROTARY should send the layer to the Rotary Speaker as the last unit.

## Acceptance Criteria
- [ ] Voicings match manual direction; fit to hardware clips (video_ref compare)
- [x] amp_variation modelled
- [ ] TO ROTARY routes into the rotary DSP or is documented unsupported


- [x] Voicings follow the manual direction (Twin/JC treble shelf, variations modelled); magnitudes ASSUMED. TO ROTARY routing still open.

---
# ns4-176b
title: 'Organ: Farfisa generator port and B3 Bass replay (OQ18)'
status: in-progress
type: task
created_at: 2026-11-13T18:01:05Z
updated_at: 2026-11-13T18:01:05Z
---

## Context
OQ18: the Farfisa (generator `0x122A71`, processor `0x122B85`) and B3 Bass (`0x122296`) PNO core 1 paths were never replayed in the oracle; the two event-2 chain objects and the `Edt_Org_kModel` value order were open.

## Higher Goal
Locate and port every organ tone source of the firmware (TD-04) with oracle evidence.

## Acceptance Criteria
- [x] Complete passes of models 2 and 5 with the ARM's own messages and a chord (`tools/organ_fb_pass.py`, `sharc-pass`)
- [x] Farfisa generator decoded; bit-exact Python model with oracle verification (`organ_farfisa_ref.py`, `organ_farfisa_oracle.py verify`)
- [x] Opt-in `ToneWheelSource::FarfisaWheels`, ring within 4.0e-5 / 104.5 dB of the oracle, default `render_hash` unchanged
- [x] Research note, spec 3.1.8, OQ18, TD-04, ROADMAP updated
- [ ] Farfisa processor and B3 Bass processor bit-exact models and ports (blocked: ARM weights and filter tables never captured, OQ19)

## Out of Scope
The Farfisa and Bass processors, the Farfisa vibrato mapping, the scanner stage.

## Summary of Changes
See PR (`feat/organ-farfisa-bass`).

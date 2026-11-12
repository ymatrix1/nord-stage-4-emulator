---
# ns4-j1kg
title: Wire missing Synth DSPS and 0x2F bitstream parameters
status: completed
type: task
priority: normal
created_at: 2026-11-12T01:51:14Z
updated_at: 2026-11-12T01:51:21Z
---

## Context
`apply_synth_layer` hardcoded the amp sustain and ignored filter ADR, LFO shape/target, unison, mono/legato/glide, octave and the oscillator type/category/wave.

## Higher Goal
Full Synth (`DSPS`) / `0x2F` parameter coverage from the `.ns4y` / `.ns4p` bitstream into the DSP.

## Acceptance Criteria
- [x] Filter env ADR, velocity-to-filter, filter on/off decoded, packed and wired
- [x] LFO shape (5) + target (4) decoded and wired
- [x] Unison, Mono/Legato, voice priority, Glide, Octave decoded and wired
- [x] Oscillator type/category/wave mapping (OQ4) decoded; WAVE flat index
- [x] Osc envelope, pitch fine/coarse, amp velocity, vibrato decoded and wired
- [x] `tools/ns4_bitscan.py`, `tools/ns4_synth_fields.py`, `tools/ns4_crosscheck.py` with tests
- [x] Provenance tags + `docs/research/synth-0x2f-ns4decode-crosscheck.md`
- [ ] Firmware/hardware confirmation of offsets and value scales (OQ7)

## Out of Scope
Master-clock LFO/arp sync rates, controller-driven vibrato modes in the GUI, SHARC disassembly.

## Summary of Changes
- New modules: `synth_params`, `synth_types`, `synth_sample`, `synth_wavetable`, `synth/{voice_alloc,arp,sample_osc}`, `cbin/{synth_preset,synth_osc,fx_preset,organ_preset,piano_preset}`.
- Offsets from the third-party ns4decode map (EMPIRICAL): 75/75 regression fields, 0 violations over 1152 factory layers; firmware confirms wave names/order only.
- Amp envelope is ADR: finite decay falls to silence (was an unsourced 0.6 sustain).

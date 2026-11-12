---
# ns4-xadj
title: 'Research: PNO Core1 oscillator bank in the oracle, ARM tables'
status: completed
type: task
created_at: 2026-11-12T22:00:00Z
updated_at: 2026-11-12T22:00:00Z
parent: ns4-trkb
---

## Context

PR #64 found a 92-step phase-accumulator wavetable bank at pc `0x122620` of the PNO Core1 image and an ARM writer for its increments. Its algorithm, lane layout, ARM-side numbers and role were open.

## Higher Goal

Replicate the organ tone generator 1:1 (`crates/nord-core/src/organ.rs`) from the firmware instead of textbook tonewheel data.

## Acceptance Criteria

- [x] Run the bank in the oracle (`sharc-bank`), SIMD on, and reproduce it word for word in Python (`tools/organ_bank_ref.py`, 29,440 words, 0 mismatches)
- [x] Spectrum check of the oracle output (x harmonics 0/-19/-36/-44/-48 dB; y 10 harmonics)
- [x] ARM side: default increment table, writer, rate-factor table, parameter array layout
- [x] `docs/research/organ-tonewheel-bank.md` with provenance tags and an `organ.rs` to-do list
- [ ] Decode the weight senders and the module-chain record (mix stage `0x12268F` driven faithfully)
- [ ] Decide the role (B3 tonewheel vs string resonance) from ARM parameter ids

## Out of Scope

- Editing `crates/nord-core` (TD-09 restructuring in progress) or `crates/nord-sharc/src/core`

## Summary of Changes

New `sharc-bank` harness, `tools/organ_bank_ref.py` with tests and the research note. Result: 184 fixed-pitch elements (92 semitones x 2 detuned copies), no per-note increments; weights, frame rate and role remain open.

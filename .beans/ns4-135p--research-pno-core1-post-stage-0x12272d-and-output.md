---
# ns4-135p
title: 'Research: PNO Core1 post stage 0x12272D and output stage 0x1229B3'
status: completed
type: task
created_at: 2026-11-13T12:30:18Z
updated_at: 2026-11-13T13:30:00Z
---

## Context

[organ-oracle-audio](../docs/research/organ-oracle-audio.md) ran the PNO Core1 post stage `0x12272D` in the oracle but could not say what it computes, what feeds it, or where the organ audio leaves the chain.

## Higher Goal

Replace the organ approximations in `crates/nord-core/src/organ/` by firmware-derived models, with bit-exact references for the Rust port.

## Acceptance Criteria

- [x] Full listing of `0x12272D` read and described (inputs, state, outputs, coefficients)
- [x] Bit-exact Python model of `0x12272D` against the oracle on random scenarios
- [x] Routine that writes the final audio identified (`0x1229B3`) and modelled bit-exactly
- [x] Chain bank, mix, post, output run in the oracle; vibrato on/off spectra compared
- [x] `docs/research/organ-post-stage.md` with equations, buffers, tags, Rust to-do list
- [x] Python checks and `tools/check_tests.py` green

## Out of Scope

Changes to `crates/nord-core`; the ARM writer of the `0x1229B3` record; the audio frame rate.

## Summary of Changes

New harness `sharc-chain`, `tools/sharc40.py`, `tools/organ_post_ref.py`, `tools/organ_post_oracle.py` with tests, the research note, OPEN-QUESTIONS and organ-oracle-audio follow-up updated.

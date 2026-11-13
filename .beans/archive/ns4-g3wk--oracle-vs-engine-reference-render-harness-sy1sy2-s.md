---
# ns4-g3wk
title: Oracle-vs-engine reference-render harness (SY1SY2 Stage B)
status: completed
type: task
created_at: 2026-11-12T23:08:01Z
updated_at: 2026-11-12T23:08:01Z
---

## Context
The SHARC oracle (`crates/nord-sharc`) can run firmware routines, but there was no reusable way to feed it a test signal, capture its output and compare it with a Rust engine render. The SY1SY2 sample resampler (Stage B, `0x1C10EE`) is the one module whose behaviour is already fully decoded, so it is the first case.

## Higher Goal
Oracle-versus-engine comparisons for every DSP module (organ, piano, FX) with one harness, one scenario format and one comparator, so the Rust engines can be matched to the firmware sample by sample.

## Acceptance Criteria
- [x] `sharc-ref` binary: image, scenario file (pokes, signal fills, voice records, handler runs, captures), CSV/WAV dump with metadata
- [x] `tools/ns4_oracle_compare.py` with tests: sample-wise error, SNR, delay, text plots, tolerances
- [x] Rust reference renderer (`nord-core` example) and integration test `tests/oracle_stage_b.rs` with documented tolerances
- [x] `docs/research/oracle-vs-engine.md`: method, results table, status of each assumption
- [x] Measured agreement: bit-exact with the oracle's table and 40-bit rules; engine within 1 ulp; phase floor and f32 cutoff differences logged in TD-08

## Out of Scope
Stage A, the second FIR pass, kernel selection by ARM, organ/piano/FX scenarios, changes to `sinc.rs` or `crates/nord-sharc/src/core/`.

## Summary of Changes
See `docs/research/oracle-vs-engine.md` and the pull request.

---
# ns4-kero
title: ORG DSP harness and parameter map (sharc-org)
status: completed
type: task
---

## Context
Foundation for 1:1 organ replication: drive the ORG DSP image in the SHARC oracle and map its parameters.

## Higher Goal
Compare `crates/nord-core/src/organ.rs` against the firmware's organ program.

## Acceptance Criteria
- [x] `sharc-org` harness: scripted pokes, per-pass diff, store sites, calls, audio buffers to WAV/CSV
- [x] `docs/research/org-dsp-map.md` with VERIFIED / ASSUMED map
- [x] Unit tests with a synthetic image
- [ ] Audio buffers located (blocked: module configuration is not in the image)

## Out of Scope
Float/multiplier/ALU lanes of the oracle.

## Summary of Changes
New `sharc-org` binary, `Core::watch_len/watch_addrs/watch_limit`, research doc. LFO module layout VERIFIED; oracle faults on `R13 = R13 * R12 (SSI)` at `0x12032E`.

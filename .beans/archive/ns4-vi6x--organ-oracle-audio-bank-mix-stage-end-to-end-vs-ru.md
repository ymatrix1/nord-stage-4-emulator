---
# ns4-vi6x
title: 'Organ oracle audio: bank + mix stage end to end vs Rust engine'
status: completed
type: task
created_at: 2026-11-13T10:13:04Z
updated_at: 2026-11-13T10:13:04Z
---

## Context
The organ tone source (PNO Core1 bank 0x122620 + mix 0x12268F) had never been run as parameters -> DSP -> audio.

## Higher Goal
First end-to-end oracle parity test for the organ engine.

## Acceptance Criteria
- [x] sharc-organ runs bank + mix for N frames and captures the audio lanes
- [x] Mix-stage weight/gain/level layout decoded by poking, Python model agrees (1.8e-5)
- [x] Rust organ compared per partial; VERIFIED items fixed behind existing switches with tests
- [x] docs/research/organ-oracle-audio.md and docs/specs/organ-engine.md updated

## Out of Scope
ARM weight writer, post stage input, absolute pitch.

## Summary of Changes
See docs/research/organ-oracle-audio.md.

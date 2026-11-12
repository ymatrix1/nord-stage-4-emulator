---
# ns4-vnp2
title: Decode ORG DSP receive path and inject ARM messages in sharc-org
status: in-progress
type: task
created_at: 2026-11-12T21:41:39Z
updated_at: 2026-11-12T21:41:39Z
parent: ns4-trkb
---

## Context
sharc-org runs the ORG passes but no ARM data reaches the DSP.

## Higher Goal
Organ audio from the real ORG DSP code for 1:1 comparison.

## Acceptance Criteria
- [x] docs/research/dsp-rx-protocol.md with tagged findings
- [x] sharc-org --msg / --msg-script / --msg-base / --rx-irq with tests
- [x] ARM sender cross-check
- [ ] Audio produced (still blocked, see doc section 6)

## Out of Scope
core/float*.rs, multiplier.rs, alu.rs, sharc_run.rs.

## Summary of Changes
See PR.

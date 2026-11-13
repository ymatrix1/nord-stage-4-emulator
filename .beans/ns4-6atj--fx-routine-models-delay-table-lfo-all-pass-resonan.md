---
# ns4-6atj
title: 'FX routine models: delay, table LFO, all-pass, resonant filter, Rust parity'
status: in-progress
type: task
created_at: 2026-11-13T11:36:40Z
updated_at: 2026-11-13T11:36:40Z
parent: ns4-trkb
---

## Context
Reference models of the remaining FX effect routines, checked word for word against the sharc-fx oracle (follow-up of the compressor model, PR #107).

## Higher Goal
Bring crates/nord-core/src/fx to parity with the firmware routines.

## Acceptance Criteria
- [x] Pair-register machine and oracle harness (tools/fx_pe.py, tools/fx_oracle.py, sharc-fx --trace and @file)
- [x] Delay 0x1206B0 model, >= 1,000 random scenarios identical
- [x] Table LFO 0x1228CF model, >= 1,000 random scenarios identical
- [ ] All-pass 0x122C39 model
- [ ] Resonant filter 0x1243F4 / 0x124497 model
- [ ] docs/research/fx-routine-models.md with equations and provenance tags
- [ ] Opt-in Rust ports with golden tests

## Out of Scope
Wiring the ports into the default signal path; ARM parameter mapping of the context words.

## Summary of Changes
(in progress)

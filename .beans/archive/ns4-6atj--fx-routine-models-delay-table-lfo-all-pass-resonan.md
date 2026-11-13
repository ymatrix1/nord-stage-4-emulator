---
# ns4-6atj
title: 'FX routine models: delay, table LFO, all-pass, resonant filter, Rust parity'
status: completed
type: task
priority: normal
created_at: 2026-11-13T11:36:40Z
updated_at: 2026-11-13T12:33:51Z
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
- [x] All-pass 0x122C39 model
- [x] Resonant filter 0x1243F4 / 0x124497 model
- [x] docs/research/fx-routine-models.md with equations and provenance tags
- [x] Opt-in Rust ports with golden tests

## Out of Scope
Wiring the ports into the default signal path; ARM parameter mapping of the context words.

## Summary of Changes
Pair-register machine (tools/fx_pe.py), oracle harness (fx_oracle.py, fx_models.py), five instruction-level models checked against sharc-fx word for word (1,100 fresh scenarios each in the final run, 2,200 to 4,600 over the work) and register for register on the new sharc-fx --trace; closed forms (fx_closed_forms.py, fx_closed_delay.py), a symbolic extractor (fx_symbolic.py), golden generator (fx_golden.py), docs/research/fx-routine-models.md, and opt-in Rust ports in crates/nord-core/src/fx (fw_delay, fw_lfo, fw_allpass, fw_resonant, fw_words, fw_short_float) with golden tests. sharc-fx also gained @file arguments and a pushed return address for RTS (DB).

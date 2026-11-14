---
# ns4-582e
title: 'FX reverb types 2..11: model, Rust port, opt-in wiring'
status: completed
type: task
created_at: 2026-11-14T12:30:00Z
updated_at: 2026-11-14T12:30:00Z
---

## Context
Types 2..11 of the reverb (module 0xC02BB44C: wrapper 0x120FDB, packed multi-tap core 0x120D8A) had their instance words (task ns4-tc25) but no model or port; the engine kept the FDN for every non-Spring model.

## Higher Goal
1:1 firmware reverb parity for all 12 types (D22: opt-in until verified end to end).

## Acceptance Criteria
- [x] Instruction model of core and wrapper generated from the listing (`tools/fx_rvbmulti_gen.py`), checked against `sharc-fx --trace` word for word (9,660 of 9,660 steps for each of the nine types; 3,200-call output runs for types 2 and 11)
- [x] Generated Rust steps, words and golden data (`tools/fx_rvbmulti_rust_gen.py`, `--check`), `FwReverbMulti` with no allocation after `new`
- [x] Golden test against `fx-rvbmulti-golden.csv`: 18 of 18 cases bit exact on the 40-bit path
- [x] Opt-in `FirmwareFx::reverb` for Booth, Room, Stage (variation), Hall, Cathedral with the `kRvbAmt` pair
- [x] Docs: fx-reverb-leslie s14, OQ21, TD-10, ROADMAP, effects-and-routing spec, D26
- [ ] Hardware comparison (emu_vs_hw segments with a known multi-tap type) and the D22 default decision (follow-up)

## Out of Scope
Organ rows (zero in the image), type 6 (no ARM capture), UI names of the type pairs, changing the default.

## Summary of Changes
tools/fx_rvbmulti_{gen,ops,check,rust_gen}.py, tools/fx_model_rvbmulti.py, tools/tests/test_fx_rvbmulti_gen.py, crates/nord-core/src/fx/fw_rvbm*.rs, tests_fw_rvbm_golden*.rs, fw_leslie_core.rs (blank constructor), reverb.rs and mod.rs (wiring), docs.

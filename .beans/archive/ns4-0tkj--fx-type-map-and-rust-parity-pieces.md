---
# ns4-0tkj
title: FX type map and Rust parity pieces
status: completed
type: task
priority: normal
created_at: 2026-11-13T10:29:33Z
updated_at: 2026-11-13T10:59:32Z
---

## Context
Open item of docs/research/fx-chain-install.md: map Edt_Effects_kFx1Type/kFx2Type/kRvbType/kDlyFbFxType to DSP routines; verified reference models for FX routines; opt-in Rust parity paths.

## Higher Goal
Bit-faithful FX chain in crates/nord-core/src/fx/.

## Acceptance Criteria
- [x] Module to routine inventory for all 100 FX code records (tools/fx_type_map.py)
- [x] Type id to routine map: no static table; blocker named (part constructor chain), Amp unit switches tabulated
- [x] Python reference model verified against the oracle: compressor gain computer 0x120630 (delay, LFO, phaser all-pass, resonant filter moved to follow-up bean)
- [x] Verified Rust opt-in path with tests (FwCompressor, MasterBus::set_fw_compressor), render_hash unchanged cb8d83b7...
- [x] docs/research/fx-type-map.md, effects-and-routing spec, TD-10, OPEN-QUESTIONS updated

## Out of Scope
Booting the full application in the ARM harness.

## Summary of Changes
tools/fx_type_map.py (inventory, modules, probe) + csv; tools/fx_routine_models.py (compressor model, check, golden); fx/fw_compressor.rs, fw_stage.rs, golden tests; docs. See the PR for details.

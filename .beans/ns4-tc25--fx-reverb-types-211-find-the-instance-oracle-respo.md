---
# ns4-tc25
title: 'FX reverb types 2..11: find the instance, oracle responses'
status: completed
type: task
created_at: 2026-11-14T11:52:56Z
updated_at: 2026-11-14T11:52:56Z
---

## Context
Reverb types 2..11 (module 0xC02BB44C, wrapper 0x120FDB, core 0x120D8A) were blocked on line pointer words believed zero in the image and never sent (fx-reverb-leslie s11, s12, OQ21).

## Higher Goal
1:1 firmware reverb parity for all 12 types.

## Acceptance Criteria
- [x] Find where the instance words come from (static image rows of the ten piano/synth units; organ rows zero)
- [x] Run 0x120FDB in the oracle on the real instance for types 2, 3, 4, 5, 7, 8, 9, 10, 11
- [x] Commit derived instance table, golden fingerprints, response table, tests
- [ ] Python model of the packed core and Rust port (follow-up)

## Out of Scope
Rust port, organ rows, UI name mapping.

## Summary of Changes
tools/fx_rvbmulti.py, tools/tests/test_fx_rvbmulti.py, docs/research/data/fx-rvbmulti-*.csv, fx-reverb-leslie s13, OQ21, ROADMAP, TECH_DEBT.

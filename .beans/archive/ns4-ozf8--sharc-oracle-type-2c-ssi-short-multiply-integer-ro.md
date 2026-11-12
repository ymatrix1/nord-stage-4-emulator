---
# ns4-ozf8
title: 'SHARC oracle: Type 2c SSI short multiply, integer-round multiplier forms, ADI-exhaustive compute sweep'
status: completed
type: task
priority: normal
created_at: 2026-11-12T22:29:47Z
updated_at: 2026-11-12T22:33:53Z
parent: ns4-trkb
---

## Context

The ORG image faults in strict mode on Type 2c words c7dc (0x12032E) and c701 (0x124C9A), the short Rn = Rn * Rx (SSI).

## Higher Goal

Run the ORG parameter-unpack routines (and any other pass) in the oracle without skipped operations.

## Acceptance Criteria

- [x] Every compute field ADI elfdump knows is decoded to ADI's text and executed (exhaustive sweep test)
- [x] Type 2c op 7 (SSI) executes; integer-round multiplier forms execute (ASSUMED)
- [x] SAT/RND signedness and fixed triple text match ADI (Rust and Python)
- [x] New first-unmodelled table for all images in docs/research/sharc-type2c.md

## Out of Scope

Peripherals (core/periph), SHP/SHF/FTA/FTB/FTC/FR register transfers (no image uses them).

## Summary of Changes

Type 2c OP 7 (SSI) added to the short-compute table; integer-round multiplier forms execute (ASSUMED); SAT/RND sign and fixed triple text match ADI in Rust and Python; ADI-exhaustive sweep test `core/compute_adi.rs` with `docs/research/sharc-type2c/` data; `docs/research/sharc-type2c.md` has the new first-unmodelled table (no compute gaps left).

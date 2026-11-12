---
# ns4-98f3
title: 'Firmware: decode EQ index math at 0x1be108 and SHARC biquad table form (0x2f730c)'
status: todo
type: task
created_at: 2026-11-12T01:46:47Z
updated_at: 2026-11-12T01:46:47Z
parent: ns4-trkb
---

## Context
ARM sender at 0x1be108 derives a table index with soft-float double math and ships rows to the SHARC. No DSP image found in bank0/bank1 CODE (docs/OPEN-QUESTIONS.md).

## Higher Goal
Replace ASSUMED LP24/HP24 and shelf topology with firmware-derived coefficients.

## Acceptance Criteria
- [ ] Decode double constants (65.0, 24.0, ...) into idx formula
- [ ] Determine row groups A,B,A',B',C,C' meaning (LP24/HP24 sections?)
- [ ] Locate DSP image or SPI-flash source

## Out of Scope
Full SHARC emulation.

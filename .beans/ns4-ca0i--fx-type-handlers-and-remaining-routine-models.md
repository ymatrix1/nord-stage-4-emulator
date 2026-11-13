---
# ns4-ca0i
title: FX type handlers and remaining routine models
status: todo
type: task
created_at: 2026-11-13T11:02:29Z
updated_at: 2026-11-13T11:02:29Z
---

## Context
Follow-up of ns4-0tkj. docs/research/fx-type-map.md s6 and s7.

## Higher Goal
Close the Edt_Effects type to handler map and port the rest of the FX routine library.

## Acceptance Criteria
- [ ] Build the part constructor chain (0xC01177AC, unit 0xC01155EC, 0xC0014068) in the ARM harness or reach the boot steady state, sweep kFx1Type, kFx2Type, kRvbType, kDlyFbFxType
- [ ] Reference models verified against sharc-fx: delay 0x1206B0, table LFO 0x1228CF, phaser all-pass 0x122C39, resonant filter 0x1243F4
- [ ] Map kCmpAmt and kCmpResponse to the 0x120630 context words

## Out of Scope
GUI work.

## Summary of Changes
(todo)

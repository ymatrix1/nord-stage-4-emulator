---
# ns4-6dmx
title: 'PNO module chain: ARM installers and module map'
status: completed
type: task
created_at: 2026-11-13T00:42:47Z
updated_at: 2026-11-13T00:42:47Z
parent: ns4-trkb
---

## Context
PNO Core1 runs an 18-module chain with an 8-slot call table that is empty in the image; the ARM installs the routines at run time.

## Higher Goal
Piano audio parity with the hardware.

## Acceptance Criteria
- [x] ARM installer of the PNO Core1 call table identified (0xC022F060 / 0xC022C524) and run in the ARM harness
- [x] Module id -> DSP routine map for PNO core 1 (45 routines) and Core2 (74 + 7 ids)
- [x] docs/research/pno-module-chain.md with module/routine/algorithm/Rust/status table and to-do list
- [x] tools/pno_module_chain.py with tests

## Out of Scope
Changing crates/ source; decoding kind/event semantics.

## Summary of Changes
Research note, derived CSV, tool and tests.

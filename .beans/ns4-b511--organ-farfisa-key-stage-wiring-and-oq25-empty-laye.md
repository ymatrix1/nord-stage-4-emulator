---
# ns4-b511
title: 'Organ: Farfisa key stage wiring and OQ25 empty layer enables'
status: in-progress
type: task
created_at: 2026-11-14T11:48:57Z
updated_at: 2026-11-14T11:48:57Z
---

## Context
PR 147 left the Farfisa processor unwired, the B3 Bass table unported and 9 factory programs silent under --firmware-routing (OQ25).

## Higher Goal
1:1 audio parity of the organ models and firmware layer routing.

## Acceptance Criteria
- [x] OQ25 analysed with the ARM program routing tool and the section table decode; rule implemented opt-in
- [x] Farfisa key stage (key-down weights x measured line gain) wired behind --farfisa-processor
- [ ] B3 Bass table and wiring (not ported, documented)
- [x] Unit tests, docs and ROADMAP in sync

## Out of Scope
Bit-exact 0x122B85 / 0x122296 models, B3 Bass key-down weights.

## Summary of Changes
See docs/research/engine-routing.md s7 and docs/research/organ-farfisa-bass.md s9.

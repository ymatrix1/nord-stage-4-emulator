---
# ns4-zxn8
title: 'FX: wire firmware closed forms (FirmwareFx), rename drive and tone-stack stages'
status: completed
type: task
created_at: 2026-11-13T15:34:15Z
updated_at: 2026-11-13T15:34:15Z
parent: ns4-trkb
---

## Context
fx/ has verified firmware ports (FwDelay, FwLfo, FwAllpass, FwResonant2/4, FwCompressor) that were exported but not wired. fx-contexts.md decodes the context words behind kDlyMix, kDlyFbkAmt, kCmpAmt, kAmpDrive, kAmpTreble/Bass, kFx1RateClkDiv/Amt.

## Higher Goal
Drive the firmware routines from the engine's FX parameters with measured scalings.

## Acceptance Criteria
- [x] FwLfo renamed FwAmpDrive, FwResonant2/4 renamed FwToneStack2/4 with doc corrections
- [x] fw_map closed forms tested against the captured series words
- [x] FirmwareFx opt-in switch (default off): delay mix law and firmware compressor (layer and master)
- [x] Stability, silence and latency tests; render_hash unchanged
- [x] Spec effects-and-routing 4.0c and TD-10 updated

## Out of Scope
Delay time words, kCmpResponse, mid band words, drive curve tables, tone-stack and all-pass base words, Fx1 type to Mod1Type map.

## Summary of Changes
See PR feat/core-fx-wiring: fx/fw_map.rs, fw_params.rs, firmware_fx.rs, master.rs, renames, tests, docs.

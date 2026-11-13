---
# ns4-wzyz
title: 'Organ: apply the 92-wheel bank to the Pipe models, B3 back to Legacy'
status: in-progress
type: bug
created_at: 2026-11-13T13:58:38Z
updated_at: 2026-11-13T13:58:38Z
---

## Context
Research (PR #116, docs/research/organ-model-paths.md) VERIFIED that the PNO core 1 bank 0x122620 and mix stage 0x12268F are installed for Edt_Org_kModel Pipe1 / Pipe2, not B3.

## Higher Goal
Keep the Rust organ attribution consistent with the firmware.

## Acceptance Criteria
- [x] ToneWheelSource::ModelDefault resolves Firmware for Pipe1/Pipe2, Legacy for B3/Vox/Farfisa
- [x] B3 default render_hash back to 25baa387...
- [x] Docs, TD-04, OPEN-QUESTIONS corrected

## Out of Scope
Vox 84-oscillator bank, B3/Farfisa/B3 Bass tone source, Pipe1 vs Pipe2 mix input word.

## Summary of Changes
See PR.

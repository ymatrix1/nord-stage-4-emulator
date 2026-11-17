---
# ns4-suv0
title: 'DSP peripherals for live audio: EP-DMA range registers, census, PNO lane (D120)'
status: in-progress
type: task
created_at: 2026-11-17T00:54:33Z
updated_at: 2026-11-17T00:54:33Z
parent: ns4-trkb
---

## Context
D119 ran the real ARM stream through `sharc-sys` for ORG Core2 with the external-port block preset (`--pre 0x3e021=1`) and a silent PNO lane.

## Higher Goal
Audio 1:1 with the hardware through the SHARC oracle: know which peripherals a live DSP pass needs.

## Acceptance Criteria
- [x] Census of every peripheral register and DMA touched by ORG Core2 and PNO core 1 passes (`--census`)
- [x] Opt-in `--epdma` replaces `--pre 0x3e021=1` with 0 differing words; `--sec-id` pass-start variant
- [x] PNO lane capture (`--lane`, `--wav`): non-silent and follows the keys (boot log replayed)
- [x] Hardware comparison tool `tools/dsp_lane_vs_hw.py`, numbers in the research note
- [x] Research note, spec section 7, D120, OQ34, tests for every new file

## Out of Scope
JIT, AOT, multi-core scheduling, SPI flash / sample DMA, FX chip, default behaviour changes.

## Summary of Changes
See `docs/research/dsp-peripherals-audio.md` and D120.

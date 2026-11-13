---
# ns4-vry5
title: 'SMP compressed voice: real CNSP page on the oracle vs Rust model'
status: completed
type: task
priority: normal
created_at: 2026-11-13T01:25:10Z
updated_at: 2026-11-13T01:25:16Z
---

## Context
The SMP image plays piano samples with the loop at 0x1C07D8 (fixed-LPC residual decode, 6-tap Q31 kernel). The record layout and the equality with the Rust sample path were not verified.

## Higher Goal
1:1 piano sampler path: engine equals the firmware, bit for bit.

## Acceptance Criteria
- [x] Record fields of the loop decoded and verified by poking
- [x] Real .npno page (mono and stereo) played in the oracle, output compared with the Rust model
- [x] Rust model and tests (smp_voice.rs, smp_page.rs, interpolate_hw), docs/research/smp-voice.md
- [x] Oracle B-register write loads I

## Out of Scope
Second loop (0x1C0865), page boundaries, ARM-side record writer, nsmp4 layout.

## Summary of Changes
See docs/research/smp-voice.md.

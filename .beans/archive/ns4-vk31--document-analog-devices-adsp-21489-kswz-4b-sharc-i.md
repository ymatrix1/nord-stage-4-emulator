---
# ns4-vk31
title: Document Analog Devices ADSP-21489 KSWZ-4B SHARC identification
status: completed
type: task
created_at: 2026-11-11T18:35:03Z
updated_at: 2026-11-11T18:35:03Z
parent: ns4-3387
---

## Context
Close-up board photo (.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg) and ADI datasheet assets (.reference/sharc/) identify the SHARC DSP IC as Analog Devices ADSP-21489 KSWZ-4B.

## Higher Goal
Keep hardware research docs accurate with exact silicon markings, clock grade (400 MHz), memory, accelerators, and peripherals.

## Acceptance Criteria
- [x] Document ADSP-21489 KSWZ-4B markings breakdown (#1823, 4259498.1B-0.2) and technical specs in docs/research/hardware-architecture.md
- [x] Cross-reference sibling NP4 vs NS4 board evidence accurately in hardware-and-dac-analysis.md and mainboard-photo-analysis.md

## Out of Scope
- Code changes to DSP or CBIN crates

## Summary of Changes
- Updated docs/research/hardware-architecture.md, docs/research/hardware-and-dac-analysis.md, and docs/research/mainboard-photo-analysis.md.

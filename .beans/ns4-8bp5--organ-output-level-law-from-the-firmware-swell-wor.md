---
# ns4-8bp5
title: Organ output level law from the firmware (swell word, layer level, bus gain)
status: completed
type: task
created_at: 2026-11-14T02:16:03Z
updated_at: 2026-11-14T02:16:03Z
---

Context: B3, Vox and Pipe landed 9 to 14 dB off the hardware level. Higher Goal: one derived organ output law. Acceptance Criteria: - [x] swell word and layer level law from the firmware - [x] organ/output_level.rs on Pipe, B3Wheels, Vox - [x] rotary drive unit fixed - [x] emu_vs_hw re-run - [x] D22 decided (Pipe flipped). Out of Scope: location of K in the firmware. Summary of Changes: docs/research/organ-output-law.md, D25.

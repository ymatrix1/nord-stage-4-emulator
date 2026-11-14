---
# ns4-8gje
title: 'Organ WeightMode::Firmware: ARM per-wheel weights and smoothing'
status: completed
type: task
created_at: 2026-11-13T12:21:43Z
updated_at: 2026-11-13T12:21:43Z
---

## Context
PR #111 verified the ARM tonewheel weights (wB formula, d7 curve, a = 0.0096 / 0.0043 smoothing). The Rust organ applied instant key gains with an ASSUMED 0.08 bus scale and pickup calibration.

## Higher Goal
Replace ASSUMED organ drawbar gains by the verified firmware weights and mix stage (TD-04, TD-10).

## Acceptance Criteria
- [x] WeightMode::Firmware (opt-in): targets from keys, drawbars, velocity; per-wheel one-pole smoothing at the engine rate
- [x] Absolute scale checked against the oracle (level 1.0000x at L = 1)
- [x] Chord, key-on and key-off transients compared with the oracle (tools/organ_weight_compare.py)
- [x] Tests: key-wheel map 128 x 9, wB formula, step responses, oracle partials
- [x] Spec section 3.1.5, TD-04, TD-10 updated

## Out of Scope
Default switch (L, velocity source, key-up a unknown); percussion second engine family; post stage.

## Summary of Changes
organ/weights.rs, weight_state.rs, weight_tests.rs; drawbar_sum.rs and tonewheel.rs wiring; organ_ref flags; tools/organ_weight_compare.py; docs.

---
# ns4-7spc
title: 'Research: FX DSP map and biquad routine check'
status: completed
type: task
created_at: 2026-11-12T22:37:18Z
updated_at: 2026-11-12T22:37:18Z
parent: ns4-trkb
---

## Context

The FX DSP image (Core1, Core2) holds the effect rack that `crates/nord-core/src/fx.rs` models. Its pass structure, buffers and routine shapes were not mapped.

## Higher Goal

Guide the Track B replication: know what each DSP image computes before porting it.

## Acceptance Criteria

- [x] `docs/research/fx-dsp-map.md`: pass structure, call graph, buffers, per-effect routine classes, parameter and instance layout, install path
- [x] One routine run in the oracle against a reference model (`sharc-fx`, `tools/fx_biquad_check.py`)
- [x] `tools/fx_dm_tables.py` with tests
- [x] All AGENTS.md checks green

## Out of Scope

- Changing `crates/nord-sharc/src/core/*` or `crates/nord-core`
- Decoding the ARM side message senders for the FX instances

## Summary of Changes

New `docs/research/fx-dsp-map.md` (pass structure, 35-stage chain, element layout, routine classes, tables). New `sharc-fx` routine harness and `tools/fx_biquad_check.py` (routine `0x120FDB`, 40/40 words bit-identical to a register/truncation model; Butterworth fs/4 coefficients in PM), `tools/fx_dm_tables.py`. EQ frequency map not recovered.

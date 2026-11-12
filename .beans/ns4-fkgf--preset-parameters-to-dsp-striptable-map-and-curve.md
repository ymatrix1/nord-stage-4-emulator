---
# ns4-fkgf
title: Preset parameters to DSP strip/table map and curve-fitting tool
status: completed
type: task
created_at: 2026-11-12T23:04:51Z
updated_at: 2026-11-12T23:04:51Z
parent: ns4-trkb
---

## Context
Relate Nord Stage 4 preset fields to the DSP strips, packed words and conversion tables.

## Higher Goal
1:1 replication of preset-driven parameter conversion in crates/nord-core.

## Acceptance Criteria
- [x] docs/research/preset-dsp-map.md with VERIFIED/EMPIRICAL/ASSUMED tables
- [x] tools/ns4_param_curves.py (table scan and closed-form fits) with tests
- [x] tools/ns4_param_inventory.py (preset field offsets and observed ranges) with tests
- [x] Rust curves vs firmware tables listed

## Out of Scope
Changes to crates/nord-core or crates/nord-sharc.

## Summary of Changes
New research note, two Python tools with tests, TD-10.

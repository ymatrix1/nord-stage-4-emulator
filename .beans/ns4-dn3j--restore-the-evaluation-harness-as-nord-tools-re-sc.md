---
# ns4-dn3j
title: Restore the evaluation harness as nord-tools; re-score the knob laws; refresh the program sweep
status: completed
type: task
priority: normal
created_at: 2026-11-14T20:14:06Z
updated_at: 2026-11-14T20:27:54Z
---

## Context

Commit de45f6de deleted the harness examples (render_hash, program_sweep, ag_synth, organ_ref, sinc_ref, smp_voice_ref, farfisa_ref, sample_dump, synth_strip_e2e) while fixing a Windows test-thread stack overflow; tools/*.py and docs depend on them.

## Higher Goal

Keep the 15-clip synth validation, the program sweep and the oracle cross-checks runnable without building examples in debug on Windows CI.

## Acceptance Criteria

- [x] Binaries restored in crates/nord-tools (publish = false), same CLI, references updated
- [x] render_hash whole hash equals tests/golden_render.rs WHOLE
- [x] 15 clips re-scored with and without --synth-knob-laws; default flipped only if the D22 bar holds
- [x] program-coverage.md and program-sweep.json regenerated for the current defaults
- [x] All AGENTS.md checks pass

## Out of Scope

Cutoff knob law, new hardware captures.

## Summary of Changes

- nord-tools crate with the nine restored binaries (render_hash includes the golden test segments), tested csv_cols module.
- Knob-law envelopes default (D57) after the 15-clip re-score; GUI checkbox, --no-synth-knob-laws.
- program-coverage.md and program-sweep.json regenerated.

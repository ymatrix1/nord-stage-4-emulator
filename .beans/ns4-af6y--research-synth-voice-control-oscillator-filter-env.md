---
# ns4-af6y
title: 'Research: synth voice control (oscillator, filter, envelope placement)'
status: in-progress
type: task
created_at: 2026-11-13T14:30:39Z
updated_at: 2026-11-13T14:30:39Z
---

## Context

The SY1SY2 image has no oscillator, filter or envelope ([sy-dsp-map](../docs/research/sy-dsp-map.md)). The synth edit ids and the ARM voice machinery decide where they run.

## Higher Goal

Bring `crates/nord-core/src/synth/` to firmware parity.

## Acceptance Criteria

- [x] Destinations of every `Edt_Syn*` id captured (strip words on ORG and PNO, SY pair blocks)
- [x] Synth oscillator helper located, modelled and checked against the oracle
- [x] SY voice gain smoother measured
- [ ] Key-down, hold and release message series captured
- [ ] Envelope law and filter equations extracted
- [x] `docs/research/synth-voice-control.md` with tags and a Rust to-do list
- [x] Python checks and `tools/check_tests.py` green

## Out of Scope

Edits to `crates/nord-core` or `crates/nord-sharc/src/core`.

## Summary of Changes

Open: see the note, sections 7 and 9.

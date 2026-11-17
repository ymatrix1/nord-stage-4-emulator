---
# ns4-wdo6
title: 'Research: synth filter chain links (kFltType, resonance word, stage rate) and --synth-fw-filter'
status: completed
type: task
priority: normal
created_at: 2026-11-17T01:03:49Z
updated_at: 2026-11-17T01:35:07Z
parent: ns4-trkb
---

## Context
D108 left three captures open before the EMPIRICAL per-type cutoff factors could be replaced: kFltType link, resonance word W28, stage call rate.

## Higher Goal
Audio 1:1 with the hardware; replace fitted synth filter factors by firmware words only when verified end to end (D22).

## Acceptance Criteria
- [x] A: kFltType to stage measured on the real firmware (18 layers)
- [x] B: resonance word to table position measured on the oracle, with voice, env and LFO sweeps
- [x] C: stage calls per pass counted for every stage
- [x] Opt-in --synth-fw-filter scored on the 12 filter clips
- [x] Research note, specs, OPEN-QUESTIONS, decision record

## Out of Scope
Default behaviour changes, render_hash changes, the 0x1257B2 stage model, aot/jit/system runner files.

## Summary of Changes

- Measured the three links (research note `docs/research/synth-filter-links.md`, D121): kFltType is an identity stage table with no factor (18 of 18 layers), settled resonance position is `128 knob / 127`, 16 stage calls per pass.
- Opt-in `--synth-fw-filter`; fixed `fw_filter_law` to the settled position. 12 clip mean 3.36 against 2.18 (fitted factors stay).
- Follow-up: model stage `0x1257B2` (raw type 3, 61 factory layers).

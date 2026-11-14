---
# ns4-xo2v
title: 'OLED: show the arp pattern dial value (0 = custom) and step ties'
status: todo
type: task
created_at: 2026-11-14T17:16:09Z
updated_at: 2026-11-14T17:16:09Z
parent: ns4-xc5n
---

## Context

The synth pattern edit pages now carry the engine's `pattern_dial_value()` (0 = the custom pattern plays) and step ties (`SynthLayerUi::step_tie`, GATE dial detent 9 = `Q`), but the OLED (`panel/bay_program`, `panel/ctl.rs`, lane of another agent) does not show them: the arp dial still reads the preset index and the gate readout shows `n/8` without a tie mark.

## Higher Goal

The OLED mirrors the hardware arpeggiator pages.

## Acceptance Criteria

- [ ] Arp pattern page shows `0` (edited / custom) after a gate edit, as the hardware does (research note section 7.1).
- [ ] The gate readout and the step grid mark a tied step (`Q`).

## Out of Scope

Engine or edit-path changes (done in ns4-5mix).

## Summary of Changes

(open)

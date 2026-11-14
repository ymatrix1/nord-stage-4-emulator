---
# ns4-g06y
title: Piano types select models (TYPE, PIANO SELECT, LEDs, LIST/INFO)
status: completed
type: task
created_at: 2026-11-14T02:53:50Z
updated_at: 2026-11-14T02:53:50Z
---

## Context
Standalone GUI: TYPE only cycled the engine category (an LED), never the sound; users reported that piano types do not change.

## Higher Goal
Piano SELECT behaves like the hardware (manual p.24-26): TYPE picks a type, the dial steps through the models of that type.

## Acceptance Criteria
- [x] Model type + hardware position read from the .npno CBIN header (VERIFIED on 38 factory models)
- [x] Library lists (entry, type) in hardware order; TYPE loads a model of the type; dial wraps inside the type; LEDs follow the current model
- [x] LIST/INFO show Type n/N; 'No piano library' without a library
- [x] Engine category drives Electric timbre set; Dyno steps Electric only
- [x] Tests: mapping, state machine, click, headless OLED

## Out of Scope
Clav pick-up EQ engine timbre; piano.select_dial inventory binding; nord-plugin.

## Summary of Changes
See PR.

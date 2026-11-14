---
# ns4-zb9o
title: Menu settings the engine cannot honour yet
status: todo
type: task
created_at: 2026-11-14T00:02:26Z
updated_at: 2026-11-14T00:02:26Z
parent: ns4-xc5n
---

## Context
Menu items marked Unsupported in crates/nord-panel/src/menus/ that need new engine behaviour.

## Higher Goal
Full menu parity with the OS v1.6x manual.

## Acceptance Criteria
- [ ] Output bus routing (Global / Per Program) with multi-output host ports
- [ ] MIDI CC/NRPN/Device send-receive filters, Program Change modes, Transpose MIDI at In/Out
- [ ] Triple pedal assignments (Pump, Mst Clk Tap, Program Up/Down, Synth Vibrato)
- [ ] Organ keyboard trigger point High/Low
- [ ] Organize Swap/Move on the program bank

## Out of Scope
- Panel rendering of the menus (nord-gui)

## Summary of Changes

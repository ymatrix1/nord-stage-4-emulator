---
# ns4-vshb
title: 'ARM emulation: boot to the main loop, inject the timer ISR, capture organ/piano/effects messages'
status: in-progress
type: task
created_at: 2026-11-13T10:38:08Z
updated_at: 2026-11-13T10:38:08Z
parent: ns4-trkb
---

## Context
The ARM firmware emulation (tools/arm_boot_full.py) stopped at an arbitrary instruction budget and never ran the RTOS timer, so the vibrato, percussion, model, Leslie and Effects clients never sent anything.

## Higher Goal
Capture every ARM to DSP message of the organ, piano and effects layers from the real firmware and decide whether the PNO core 1 oscillator bank (0x122620) is the organ tone engine.

## Acceptance Criteria
- [x] Boot stops at the main event loop (0xC00A0248), not at an instruction budget
- [x] Timer interrupt (AINTC irq 22, handler 0x800007D8) injected; the initial DSP push settles
- [x] Edit-id sweep through the real layer clients with the value written to the layer value table
- [ ] docs/research/arm-organ-messages.md section 10 with the tables and the organ vs piano verdict
- [ ] Drawbar and vibrato burst replayed into the PNO core 1 image with sharc-bank

## Out of Scope
Preemptive scheduling (the firmware has none), SPI and DSP download models, audio output.

## Summary of Changes
See the pull request.

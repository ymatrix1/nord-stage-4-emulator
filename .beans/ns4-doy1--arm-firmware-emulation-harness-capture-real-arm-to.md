---
# ns4-doy1
title: 'ARM firmware emulation harness: capture real ARM to DSP messages'
status: in-progress
type: task
created_at: 2026-11-12T23:41:20Z
updated_at: 2026-11-12T23:41:20Z
---

## Context
PR #70 recovered the ARM->DSP message format and sender families but not the organ drawbar/vibrato/percussion/rotary messages; the UI-to-sender chain runs through virtual calls.

## Higher Goal
Capture real parameter messages from the firmware itself and replay them into the DSP images.

## Acceptance Criteria
- [x] tools/arm_emu.py: unicorn ARM926 harness with stubs and port-level message capture, unit tests with synthetic ARM snippets
- [x] tools/arm_capture.py: strip setter 0xC0017088 and controller-word senders captured to JSONL/CSV, JSONL -> sharc-org --msg-script converter
- [x] Replay into the ORG image shows DM state changes
- [x] docs/research/arm-fw-emulation.md with method, memory map, stubs, limits
- [ ] Organ drawbar/vibrato/percussion/rotary bursts captured (needs the full boot, see doc s5)

## Out of Scope
RTOS scheduler, timers, edit framework boot.

## Summary of Changes
See docs/research/arm-fw-emulation.md.

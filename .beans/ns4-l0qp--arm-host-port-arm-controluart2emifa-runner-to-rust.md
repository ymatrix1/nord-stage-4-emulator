---
# ns4-l0qp
title: 'arm-host: port ARM control/UART2/EMIFA runner to Rust'
status: todo
type: task
created_at: 2026-11-16T22:04:11Z
updated_at: 2026-11-16T22:04:11Z
parent: ns4-zp6a
---

## Context
tools/arm_emu.py / arm_keydown.py / arm_steady.py run ARM firmware via Python Unicorn hooks, which adds Python hook overhead and sits outside the Rust multi-core scheduler.

## Higher Goal
Run the booted ARM firmware in Rust (unicorn-engine) with UART2 MIDI, I2C/EEPROM stub, and lock-free EMIFA CS3 -> SHARC message queue.

## Acceptance Criteria
- [ ] Load post-boot ARM snapshot in Rust and tick the control/MIDI loop in real time.
- [ ] Forward EMIFA CS3 writes directly into the lock-free SHARC parameter ring.

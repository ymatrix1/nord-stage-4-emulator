---
# ns4-3gqr
title: 'bridge: TDM/SPORT audio buffer DMA routing between SHARC cores'
status: todo
type: task
created_at: 2026-11-16T22:04:08Z
updated_at: 2026-11-16T22:04:08Z
parent: ns4-zp6a
---

## Context
L2 DMA descriptors (0x20004000..0x20004160, dsp-rx-protocol.md) move 40-word pass buffers (0xB0000/28/50/78/C8 and 0xC0000 twins) between Core1 and Core2, driven by P4I (Core1) and mailbox 0xB00E2 (Core2) at 21,701 Hz.

## Higher Goal
Connect ORG/PNO/SY SHARC outputs to FX inputs and audio output inside the multi-core scheduler.

## Acceptance Criteria
- [ ] Model L1/L2 pass-buffer copy between Core1/Core2 and between engines -> FX at pass boundaries.
- [ ] Trigger P4I on Core1 and mailbox 0xB00E2 on Core2 on each pass.

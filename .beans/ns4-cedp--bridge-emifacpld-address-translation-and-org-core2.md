---
# ns4-cedp
title: 'bridge: EMIFA/CPLD address translation and ORG Core2 stage install'
status: todo
type: task
created_at: 2026-11-16T22:04:09Z
updated_at: 2026-11-16T22:04:09Z
parent: ns4-zp6a
---

## Context
ARM writes 2-word EMIFA CS3 frames while the SHARC DMA write port (0x20004190..A8) takes 3-word (addr, x, data) frames. Unlike PNO (--msg-base 0), ORG Core2 stage pointers (0xC10B8) are not updated by writing DM(0xE0000)=0x8012082f alone.

## Higher Goal
Decode the CPLD/EMIFA -> SHARC L1 address mapping and ORG Core1/Core2 stage-table install path so ARM parameter writes drive all engines automatically.

## Acceptance Criteria
- [ ] Document and implement the EMIFA CS3 -> SHARC system-address mapping.
- [ ] Verify an ARM key-down / program load activates ORG Core2 oscillator/filter stages without manual --msg pokes.

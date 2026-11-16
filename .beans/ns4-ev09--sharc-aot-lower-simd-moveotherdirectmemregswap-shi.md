---
# ns4-ev09
title: 'sharc-aot: lower SIMD, Move(Other/DirectMem/RegSwap), ShiftImm, Multifunction'
status: todo
type: task
priority: normal
created_at: 2026-11-16T21:59:21Z
updated_at: 2026-11-16T22:14:00Z
parent: ns4-zp6a
blocked_by:
    - ns4-tycr
    - ns4-cedp
---


## Context
Ceiling study (#465) shows 40.8% simd_only + 56.7% other ops (Move(Other) 1703, DirectMem 739, RegMove(with_compute) 302, RegSwap 249, ShiftImm 104, Multifunction 96) in 8-voice ORG Core2.

## Higher Goal
Eliminate the 97.5% fallback rate in sharc-aot so the entire pass lowers to Rust.

## Acceptance Criteria
- [ ] Lower SIMD (PEY) compute + dual-word DAG loads/stores in sharc-aot.
- [ ] Lower Move(Other), Move(DirectMem), Move(RegSwap), RegMove(with_compute), ShiftImm, ModifyImm, and Multifunction.
- [ ] Unit tests compare every newly lowered op against Core::run oracle.

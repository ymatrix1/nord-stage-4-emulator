---
# ns4-9a0g
title: 'jit: check block cache before Vec alloc/scan + negative cache'
status: todo
type: task
created_at: 2026-11-16T21:59:20Z
updated_at: 2026-11-16T21:59:20Z
parent: ns4-zp6a
---

## Context
Profile (#464) shows 0.43-0.64 ms/pass spent in pre-lookup Vec alloc + DecodeCache scan before jit.blocks.get(&key).

## Higher Goal
Restore JIT pass throughput on real firmware from 3.0-3.7 Msteps/s to ~10 Msteps/s (parity with Core::run).

## Acceptance Criteria
- [ ] Check jit.blocks.get(&key) before allocating Vec or scanning DecodeCache.
- [ ] Add negative cache for ineligible PCs (invalidated on code_gen/invalidate_range).
- [ ] 300-pass warmed-up ORG Core2 diff passes 1:1 without --lenient.

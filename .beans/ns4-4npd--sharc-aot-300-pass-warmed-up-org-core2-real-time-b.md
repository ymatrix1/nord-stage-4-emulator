---
# ns4-4npd
title: 'sharc-aot: 300-pass warmed-up ORG Core2 real-time benchmark'
status: todo
type: task
created_at: 2026-11-16T21:59:23Z
updated_at: 2026-11-16T21:59:23Z
parent: ns4-zp6a
---

## Context
Real-time budget for ORG Core2 is 46.08 us/pass (92.6 / 97.1 / 129.2 Msteps/s).

## Higher Goal
Verify whole-pass AOT meets the single-core real-time budget.

## Acceptance Criteria
- [ ] Measure 300 warmed-up passes on idle, 1 voice, and 8 voices (mean + worst pass).
- [ ] Verify 8-voice pass <= 46.08 us/pass (>= 129.2 Msteps/s) on 1 host thread.

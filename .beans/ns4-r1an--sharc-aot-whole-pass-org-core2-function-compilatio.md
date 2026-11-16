---
# ns4-r1an
title: 'sharc-aot: whole-pass ORG Core2 function compilation + 1:1 diff'
status: todo
type: task
priority: normal
created_at: 2026-11-16T21:59:22Z
updated_at: 2026-11-16T22:14:00Z
parent: ns4-zp6a
blocked_by:
    - ns4-ev09
---


## Context
1,971 dynamic blocks (4,208 PCs) execute per 8-voice pass; single-block dispatch cannot hit 46.08 us/pass.

## Higher Goal
Compile the entire ORG Core2 pass (0x125DAD..0x125DDF) as a single Rust function with local registers and inlined Flow.

## Acceptance Criteria
- [ ] Lower Flow (JumpDirect, CallDirect, JumpIndirect, Return, DoUntilCounter) in sharc-aot whole-pass mode.
- [ ] Keep Core registers in local variables across the pass, syncing at idle_pc/fallback.
- [ ] 1:1 diff vs Core::run passes on ORG Core2 idle, 1 voice, and 8 voices without --lenient.

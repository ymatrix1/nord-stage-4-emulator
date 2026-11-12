---
# ns4-wq9u
title: 'Bug: ns4 debug build overflows main-thread stack in render-patch (reverb buffers)'
status: todo
type: bug
created_at: 2026-11-12T01:57:08Z
updated_at: 2026-11-12T01:57:08Z
parent: ns4-trkb
---

## Context
`target/debug/ns4.exe render-patch` aborts with 'main has overflowed its stack' (Box::new of large arrays such as fdn_bufs [[f32;8192];4] is built on the stack in debug). Release works.

## Acceptance Criteria
- [ ] Construct large FX buffers on the heap without a stack temporary (vec![..].into_boxed_slice() or boxed_array helper)
- [ ] Test constructing 7 LayerFxChain in a thread with a small stack

## Out of Scope
Audio changes.

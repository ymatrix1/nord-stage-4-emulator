---
# ns4-tycr
title: 'sharc-aot: measured prototype of one hot function vs interpreter'
status: todo
type: task
created_at: 2026-11-16T22:13:38Z
updated_at: 2026-11-16T22:13:38Z
parent: ns4-zp6a
---

## Context
Before committing to full-pass AOT lowering (ns4-ev09/ns4-r1an), we need a measured (not projected) throughput and compile-time check on a real ORG Core2 hot function (e.g. 0x12501F saw helper or 0x1215FB).

## Higher Goal
Measure actual rustc -O speedup, compile time, binary size, and 1:1 bit-exactness against Core::run on one hot ORG Core2 function.

## Acceptance Criteria
- [ ] Compile one hot ORG Core2 function (0x12501F or 0x1215FB) via sharc-aot + rustc -O.
- [ ] Measure 300-pass warmed-up throughput vs Core::run, compile time, and 1:1 state match without --lenient.

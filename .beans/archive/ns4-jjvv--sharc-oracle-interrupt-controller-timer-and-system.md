---
# ns4-jjvv
title: 'SHARC oracle: interrupt controller, timer and system registers'
status: completed
type: task
priority: normal
created_at: 2026-11-12T20:42:20Z
updated_at: 2026-11-12T20:42:31Z
parent: ns4-trkb
---

## Context
crates/nord-sharc executed the firmware's start-up and polling loops but had no interrupt controller, timer, IDLE, status/PC/loop stack limits, ASTAT/STKY/FLAGS or secondary register sets (docs/research/sharc-sequencer.md).

## Higher Goal
Run the DSP sections' host-protocol paths (milestone M5) in the oracle: Core1 mailboxes are filled by the P4I service routine.

## Acceptance Criteria
- [x] IRPTL/LIRPTL/IMASK/IMASKP, priorities, NESTM, IRPTEN, IVT dispatch (ISA vector slots), RTI, IDLE with unit tests
- [x] Timer (TCOUNT/TPERIOD/TIMEN), EMUCLK, ASTAT/STKY/FLAGS/PCSTK/LADDR/PX registers, stack depth limits
- [x] Secondary register sets, CBUFEN, BR0/BR8, Type 19a MODIFY/BITREV
- [x] sharc-run --irq N@STEP and --flag N=LEVEL
- [x] Idle loops of FX/ORG Core1 and Core2 explained from the images

## Out of Scope
PUSH/POP STS, JUMP (CI), CJUMP/RFRAME, CB7I/CB15I, IICDI, BDCST1/9 broadcast loads, cycle-exact pipeline stalls.

## Summary of Changes
New core/interrupts.rs, vectors.rs, timer.rs, sysregs.rs, stacks.rs, banks.rs, irq_scenarios.rs; wiring in exec/branch/dag/moves/memory/mod; docs/research/sharc-sequencer.md.

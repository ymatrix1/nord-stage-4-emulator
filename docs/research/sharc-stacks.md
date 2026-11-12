# SHARC PC, loop and status stacks on the simulator

Checked: 2026-11-12.
Sources: CCES 2.12.1 ADSP-21489 simulator, probe `docs/research/sharc-seq/gen_stack_probe.py` (fixture `stack_fixture.txt`, dump `stack_measured.txt`, word names `stack_names.json`), harness of [sharc-loops.md](sharc-loops.md); PRM rev 2.4 4-12..4-14, 4-75.
Question: what do PCSTK, PCSTKP, LADDR, CURLCNTR and STKY read through calls, nested loops, `PUSH`/`POP` and status-stack pushes, and how does the model compare?

## Findings (all in `stack_measured.rs`, 0 mismatches except the listed gaps)

* **Empty stacks:** PCSTK reads `0x00FFFFFF` (24 bits of ones; the model used `0x03FFFFFF`), LADDR and CURLCNTR read `0xFFFFFFFF` (the model 0). Fixed.
* **PC stack:** PCSTKP counts 1..30 along a call chain of 30 levels, PCSTK is the return address of the innermost call, STKY bit 21 (PC stack full) is set at 30 entries and bit 22 (PC stack empty) after the chain is popped: all matched. STKY bits 24 and 26 (status and loop stack empty) read set at the start.
* **Status stack:** 15 `PUSH STS` and 15 `POP STS` with STKY readback (bit 24 clears on the first push, sets again after the last pop): matched.
* **Nested counter loops** (depth 1..6): PCSTKP, PCSTK, LADDR, CURLCNTR matched; STKY reads empty-bits of the first loop of a nest as still set (`nest1.stky`): the simulator updates them late (not modelled).
* **`PUSH LOOP` / `PUSH PCSTK` / `POP`:** a fresh `PUSH LOOP` entry reads LADDR 0 and CURLCNTR 1 (the model had a garbage LADDR and 0); `PUSH PCSTK` pushes the address of the next instruction (the model pushed 0). Fixed; sequences `push loop, push pcstk, pop pcstk, pop loop` and the variants matched.
* **LADDR / CURLCNTR inside loops** (loop counter 3, bodies of 1 to 5 instructions, read by the first body instruction): the loop leaves the stack when its last instruction is *fetched*, three instructions before it runs, so the reads in the last passes of short loops show an empty stack:

| body | CURLCNTR per pass | LADDR per pass |
|---|---|---|
| 1 | 3, 1, empty | `0x4F..`, same, empty |
| 2 | 3, empty, empty | `0x8F..`, empty, empty |
| 3 | 3, 2, empty | `0xCF..`, same, empty |
| 4 | 3, 1, empty | `0xEF..`, same, empty |
| 5 | 3, 2, 1 | `0xEF..` on every pass |

  (empty = `0xFFFFFFFF`). Not modelled; the firmware never reads these registers.
* **Simulator limits:** the simulator stops with an error on a 31st PC stack entry ("PC stack has overflowed"), so overflow behaviour cannot be measured there; operations on PCSTK or the loop stack inside a hardware loop (the assembler warns about them) corrupt the loop stack, so `PUSH`/`POP` inside a loop were dropped from the probe.
* **Not measured:** `CJUMP`/`RFRAME` (the first probe version jumped into IOP space and was removed, see `gen_stack_probe.py frame`), `JUMP (CI)`, status stack overflow, loop stack overflow, LADDR type bits above 5 instructions.

## Implications

Model changes: `PCSTK_EMPTY`, empty LADDR/CURLCNTR, `PUSH LOOP` entry values, `PUSH PCSTK` value. The firmware does not use any of these instructions (sharc-sequencer.md section 2), so the remaining gaps are low priority.

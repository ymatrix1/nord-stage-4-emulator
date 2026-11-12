# SHARC arithmetic loops on the simulator (and a probe harness for the oracle)

Checked: 2026-11-12.
Sources: CCES 2.12.1 ADSP-21489 simulator (full licence), probes in `docs/research/sharc-seq/`; PRM rev 2.4 4-53..4-59.
Question: where is the condition of `DO ... UNTIL cond` evaluated, and how many passes does a loop run, for bodies of 1 to 6 instructions?

## Method (reusable for the other sequencer probes)

* `gen_loops_probe.py <dir> [loops|entry]` writes an assembly function `_probe(out)` with one loop per case; the body decrements R0 once per pass, so the final R0 is the number of passes. Each case stores R0 by absolute address twice after the loop (`dm(_out+i) = r0`), the second store one instruction later, so an instruction that did not execute shows up as an unwritten word. The C wrapper (`loops.c`) zero-initialises `out`.
* Build `cc21k -proc ADSP-21489 -si-revision any -g -O0 -o loops.dxe loops.asm loops.c`, run in the CCES simulator, Memory Browser > Dump Memory of `_out` (`elfsyms` gives the address) -> `loops_measured.txt`.
* `dxe2fixture.py` cuts the probe function (short-word parcels) out of the .dxe into `loops_fixture.txt`; `crates/nord-sharc/src/core/probe_fixture.rs` loads it into a `Core`, runs from the entry to the exit address and returns the output array. **The model therefore executes exactly the machine code the simulator ran**, and `loops_measured.rs` compares the arrays word by word. Unsupported instructions show up as faults.
* Conditions that never become true loop forever: `eq`, `ge`, `gt` start at K = 4 (a smaller start can pass the true state before the first sample); the first run hung on `ge`, N = 2, K = 0.

## Findings

* The existing model (condition sampled after the instruction at body index `(N - 4) mod N`, then three more instructions, leaving after the last body instruction) matched **all** counter-loop cases (`LCE`, 189) and all arithmetic-loop cases where the condition becomes true during the loop (`eq`, `lt`, `le`, every body length and flag-setting position, and `ne`, `ge`, `gt` for bodies of 2, 4, 5, 6 instructions).
* New, for bodies of **1 or 3 instructions** (the lengths where the sample of the previous pass is the `DO` instruction itself): a condition that already holds when the loop is entered ends it after three body instructions: a one instruction body runs three passes, a three instruction body one pass (the old model gave four and two). The flag-setting instruction position does not matter because the flags sampled are those from before the loop.
* New: after a **one instruction** loop that ended this way the instruction that follows the loop is **not executed** (its store left the initial zero while the second store ran). Modelled with `LoopFrame::skip_after`.

## Implications

* `arith_loop.rs` provenance is now VERIFIED for these cases; `refresh_check` unchanged, `exec_do_until` samples the condition at entry for bodies of 1 and 3 instructions.
* The probe harness is the pattern for the next sequencer probes (stack operations, cycle counts, interrupts): build the probe as a C-callable assembly function, run it on the simulator, commit fixture and dump, assert on a `Core`.

## Follow-ups

* Not measured: SIMD loops, branches inside the loop body, loops with conditions on flags other than AZ/AN, nested loops, the first-run cycle cost.
* Stacks (PUSH/POP, PCSTK, LADDR), cycle counts and interrupts: next probes on the same harness.

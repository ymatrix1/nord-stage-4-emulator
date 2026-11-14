# Routine-level diff: PNO Core1 routine 0x122620 on the simulator and the model

Checked: 2026-11-12. Sources: `dsp_image_1_PNO.bin` core 1 code (ADI `elfdump` listing), CCES 2.12.1 ADSP-21489 simulator, `docs/research/sharc-seq/routine_diff.py`.
Question: does the model run the real oscillator-bank routine (pc 0x122620, mix stage 0x12268F) bit for bit like the simulator?

## Method
`routine_diff.py` copies the routine's code (`--code 122600:122900`, free in the probe) and DM ranges of the image into both machines, then calls it from the census probe's slot (`CALL (M8, I8) (DB)` with I8 = entry). Per case a random core state (MODE1 `0xE10000`: SIMD, BDCST, RND32) and random words in `--rand` ranges. `core/census.rs` and `sim_batch.py` gained `code`, `dm` and `watch` header lines and ` | ADDR WORDS` per-case DM chunks. Debug aids: `CENSUS_STOP=hex` (and `CENSUS_HIT=n`) make `sharc_census_run` print R, S, I, M at the first n-th arrival at an address; the simulator side sets a breakpoint and reads the registers.

## Result (routine 0x122620, 12 random cases)
* Registers and index registers match the simulator at every instruction boundary from 0x122623 to 0x122689 (41 addresses, first arrival), including the whole first outer iteration with the SIMD fixed multiplies, `FEXT` by register, swaps, dual-memory accesses and `MODIFY`.
* The final state differs. With the inner count patched to 1 the simulator's state at the outer-loop end (0x12268c) has `I3 = 0xC24E5 + 0xB8` (one outer iteration), the model `I3 = 0xC24E5 + 2 * 0xB8` (two, as `LCNTR = 2` says). Breakpoints on the first instruction after a loop fire on the prefetch at the first pass, so breakpoint comparisons right after a loop exit are not reliable.
* Simple nested counter loops agree: `nested.asm` (outer `LCNTR = 2` around inner `LCNTR = 3`, inner loop ending on a plain compute and on a compute with a memory access, outer 3 around inner 1) gives identical results on the simulator and the model (`loops_measured.rs`, `nested_*.txt`).
* In the routine the simulator's outer loop runs 4 passes (`I3` ends at `0xC24E5 + 4 * 0xB8`) for outer counts 1, 2 and 3 patched into `LCNTR`, while the model runs exactly the count (1, 2, 3). So the simulator does not use the immediate that ADI's listing and the model decode. Not resolved: it may be the way the routine is entered (`CALL (M8, I8) (DB)` from a slot, probe artefacts such as the PC stack or the loop stack left by the C runtime), or a real difference in how `DO (PC, 0xFF800055)` is encoded (offset bit 23 set). Next step: assemble the routine's first 20 instructions with `cc21k` (`lcntr = 2, do ... until lce` as written by the assembler) and compare.

* The firmware's `lcntr = 2, do (pc, 0xff800055)` has bit 7 of the second parcel set (`0c00 0280 0055`); the ADSP-21489 assembler writes the same loop as `0c00 0200 000d`. The images were built for SHARC+ (ADSP-2157x), so this bit may be a SHARC+ extension of `DO` that the 21489 simulator does not honour: the ADSP-21489 functional simulator is then the wrong reference for this routine. `sim_batch.py --target 21571` drives the ADSP-21571 cycle-accurate simulator instead (probe built with `cc21k -proc ADSP-21571`, label `probe.`), The runs did not finish at first because the cycle-accurate simulator needs about 140 s for the C runtime start-up and ignores writes to PC; the batch runner now loads once and the `--plus` probe ends in `jump _restart` so each case resumes from the exit breakpoint (see `sim_batch.py`). Status of the 21571 comparison: see the PR description.

**Decision (coordinator, 2026-11-12):** keep the model's `DO` semantics (they match the ADI listing and the exact count). The divergence on `lcntr = 2, do (pc, 0xff800055)` is tagged *SHARC+ `DO` extension not honoured by the 21489 simulator; the 21489 simulator is not a valid reference for this routine*.

## ADSP-21571 (SHARC+) cycle-accurate simulator
`sim_batch.py --target 21571` works (one 140 s start-up, then about 0.3 s per case; the PC cannot be set, so the `--plus` probe loops back to its loader). First comparison: the 40 condition-code cases of the census sweep (every combination of AZ AV AN AC x AF, four conditions). **All 107-word dumps agree with the model in every register, including the conditional moves (the AF/ALUSAT rules of `sharc-census-diff.md` hold on SHARC+), except three system registers**: ASTATy, STKYx and STKYy keep different bits of a random write on SHARC+ (the 21489 masks of `sysregs.rs` are `VERIFIED` for the 21489 only; on the 21571 STKYx/STKYy read back almost only the stack-status bits, `0x0540_0000` plus a few). The model keeps the 21489 masks until the 2157x masks are measured.

### Routine 0x122620 on the SHARC+ simulator: bit-exact
6 random cases (random registers, `MODE1 = 0xE10000`, random words in DM `0xC2400..0xC2800`, image tables elsewhere), whole routine including the `DO (PC, 0xFF800055)` outer loop: **every R, S, B, L, I, M register, MODE1, ASTATx, MR0..2 and the 1 024 output words in DM match the model**. The only differences are the three SHARC+ system-register write masks above (ASTATy, STKYx, STKYy; not touched by the routine). So the `DO` bit 7 is indeed a SHARC+ feature the 21489 simulator ignores, and the model's loop semantics are right for it. This is the oscillator-bank inner loop (91 passes x 2): the bank is bit-exact with the SHARC+ simulator on this input class.

### Mix stage 0x12268F on the SHARC+ simulator: bit-exact
40 random cases (seed 7, `--target 21571`, 202 s incl. the 140 s start-up) plus an earlier 12-case run: MODE1 `0xE10000`, `I1 = 0xB473C` (record of set 0), `I0 = 0xB0220`, random floats in [-4, 4) in the weight arrays `wA 0x9566E..`, `a 0x9594F..`, scalars `0x959AF..B6`, `b/g7/g8/L 0x95AE2..0x95B4F`, the bank x array `0xC24E5..0xC264C` and the output window `0xB0220..27`; image tables elsewhere (`0x95000..0x95FFF`, `0xB4700..`, `0xC0000..0xC3FFF`). **All 107 state words, the scratch table and every watched DM word match the model in all 40 cases (0 of 40 differ).** VERIFIED: `routine_diff.py` command below, max err 0 bits. Bank `0x122620` was already bit-exact (above).

```bash
python docs/research/sharc-seq/routine_diff.py IMG/dsp_image_1_PNO.bin 0 12268F OUT/cens21.dxe docs/research/sharc-seq/census21_fixture.txt   --work WORK --target 21571 --dm c0000:c4000 --dm 95000:96000 --dm b4700:b4800 --rand-float 9566e:9594e --rand-float 9594f:959b8   --rand-float 95ae2:95b50 --rand-float c24e5:c264d --rand-float b0220:b0228 --reg i1=b473c --reg i0=b0220 --cases 40 --seed 7
```
(probe: `gen_census_probe.py OUT --plus && cc21k -proc ADSP-21571 -si-revision any -g -O0 -o OUT/cens21.dxe OUT/cens.asm OUT/cens.c`). `routine_diff.py` gained `--reg NAME=HEX` and `--rand-float LO:HI`; `sim_batch.py` runs each 21571 case with `Run(wait)` under a watchdog (`--case-timeout`).

### Post stage 0x12272D and output stage 0x1229B3 on the 21571 simulator: not yet compared (stall)
`routine_diff.py --scenario post|output` feeds the inputs of `tools/organ_post_ref.py` (`synthetic_inputs`, `output_inputs`; contiguous words coalesced into one write, `--watch` ranges `0xE0000..0xE2FFF`, `0xB0000..0xB1FFF` read back) to both machines. The pokes work (4 s per case; `sim_batch.py` prints `case k` / `poked` to stderr), but **`Run` of the post stage never returns on the simulator**: 25+ minutes, and the 60 s `--case-timeout` watchdog (`Halt` from a timer thread) did not recover it either; no case completed (attempts with 6 and 20 cases). The mix stage with the same harness completes in about 5 s per case, so the stall is specific to the routine's inputs or code path (open: circular-buffer registers of the random start state, an MMR read, or a polling loop). Next: run the post stage from the exact `sharc-chain` state (not a random register file), bisect with breakpoints inside `0x12272D..0x1229B2`, and keep the model side (`organ_post_oracle.py verify`, bit-exact against the Rust oracle) as the reference until then.

## Implications
The bank routine is not yet verified. Everything before the loop exit matches, so the multiplier, `FEXT`, dual-memory and SIMD paths it uses are consistent with the instruction-level census (`sharc-census-diff.md`).

## Follow-ups
1. Get `sim_batch.py --target 21571` working (SHARC+ reference), then re-run the routine there.
2. Re-run `routine_diff.py` for 0x122620 and 0x12268F once that is settled; add the passing cases to a regression fixture.
3. Use `step`-based comparison (not breakpoints) after a loop exit.

## Reproduce
```bash
python docs/research/sharc-seq/routine_diff.py target/oracle/dsp/dsp_image_1_PNO.bin 0 122620 OUT/cens.dxe \
  docs/research/sharc-seq/census_fixture.txt --work WORK --dm c0000:c4000 --dm 95000:96000 --rand c2400:c2800 --cases 12
```

# Routine-level diff: PNO Core1 routine 0x122620 on the simulator and the model

Checked: 2026-11-12. Sources: `dsp_image_1_PNO.bin` core 1 code (ADI `elfdump` listing), CCES 2.12.1 ADSP-21489 simulator, `docs/research/sharc-seq/routine_diff.py`.
Question: does the model run the real oscillator-bank routine (pc 0x122620, mix stage 0x12268F) bit for bit like the simulator?

## Method
`routine_diff.py` copies the routine's code (`--code 122600:122900`, free in the probe) and DM ranges of the image into both machines, then calls it from the census probe's slot (`CALL (M8, I8) (DB)` with I8 = entry). Per case a random core state (MODE1 `0xE10000`: SIMD, BDCST, RND32) and random words in `--rand` ranges. `core/census.rs` and `sim_batch.py` gained `code`, `dm` and `watch` header lines and ` | ADDR WORDS` per-case DM chunks. Debug aids: `CENSUS_STOP=hex` (and `CENSUS_HIT=n`) make `sharc_census_run` print R, S, I, M at the first n-th arrival at an address; the simulator side sets a breakpoint and reads the registers.

## Result (routine 0x122620, 12 random cases)
* Registers and index registers match the simulator at every instruction boundary from 0x122623 to 0x122689 (41 addresses, first arrival), including the whole first outer iteration with the SIMD fixed multiplies, `FEXT` by register, swaps, dual-memory accesses and `MODIFY`.
* The final state differs. With the inner count patched to 1 the simulator's state at the outer-loop end (0x12268c) has `I3 = 0xC24E5 + 0xB8` (one outer iteration), the model `I3 = 0xC24E5 + 2 * 0xB8` (two, as `LCNTR = 2` says). The simulator also ignores the inner count for values of 2 or more when a breakpoint sits on the first instruction after the loop (the breakpoint fires on the prefetch at the first pass), so breakpoint comparisons after a loop exit are not reliable.
* Not resolved: whether this is a simulator quirk of nested `LCNTR` loops entered through `CALL (M8, I8) (DB)` or a model gap. The model's own loop tests (`loops_measured.rs`, arithmetic and counter loops, nested) pass; nested counter loops with an inner loop that ends on a multifunction instruction are not among the measured cases.

## Implications
The bank routine is not yet verified. Everything before the loop exit matches, so the multiplier, `FEXT`, dual-memory and SIMD paths it uses are consistent with the instruction-level census (`sharc-census-diff.md`).

## Follow-ups
1. Measure nested counter loops on the simulator with a purpose-built probe (inner loop ending on a three-parcel multifunction instruction, outer count 2) and compare the model.
2. Re-run `routine_diff.py` for 0x122620 and 0x12268F once that is settled; add the passing cases to a regression fixture.
3. Use `step`-based comparison (not breakpoints) after a loop exit.

## Reproduce
```bash
python docs/research/sharc-seq/routine_diff.py target/oracle/dsp/dsp_image_1_PNO.bin 0 122620 OUT/cens.dxe \
  docs/research/sharc-seq/census_fixture.txt --work WORK --dm c0000:c4000 --dm 95000:96000 --rand c2400:c2800 --cases 12
```

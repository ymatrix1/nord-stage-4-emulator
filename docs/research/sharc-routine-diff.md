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

* The firmware's `lcntr = 2, do (pc, 0xff800055)` has bit 7 of the second parcel set (`0c00 0280 0055`); the ADSP-21489 assembler writes the same loop as `0c00 0200 000d`. The images were built for SHARC+ (ADSP-2157x), so this bit may be a SHARC+ extension of `DO` that the 21489 simulator does not honour: the ADSP-21489 functional simulator is then the wrong reference for this routine. `sim_batch.py --target 21571` drives the ADSP-21571 cycle-accurate simulator instead (probe built with `cc21k -proc ADSP-21571`, label `probe.`), but the runs do not finish yet (breakpoint at the probe exit never reached; not diagnosed).

**Decision (coordinator, 2026-11-12):** keep the model's `DO` semantics (they match the ADI listing and the exact count). The divergence on `lcntr = 2, do (pc, 0xff800055)` is tagged *SHARC+ `DO` extension not honoured by the 21489 simulator; the 21489 simulator is not a valid reference for this routine*.

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

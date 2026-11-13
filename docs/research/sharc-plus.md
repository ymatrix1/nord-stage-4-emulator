# SHARC+ (ADSP-2157x) differences measured on the ADSP-21571 cycle-accurate simulator

Checked: 2026-11-13. Sources: CCES 2.12.1 ADSP-21571 cycle-accurate simulator (`sim_batch.py --target 21571`), probe `gen_census_probe.py --plus`.
Question: where does the SHARC+ core differ from the ADSP-21489 model that `crates/nord-sharc` was verified on?

## Method
Same census probe as `sharc-census-diff.md`, built with `cc21k -proc ADSP-21571` (label `probe.`, restart loop). The C-runtime start-up takes about 140 s once, then about 0.3 s per case. 120 random full core states with a no-op in the instruction slot: the loader writes ASTATx/y and STKYx/y from the state table and the dumper reads them back.

## Findings (VERIFIED, `census_regress.rs::sharc_plus_system_register_masks_match_the_21571_simulator`)
| Register | ADSP-21489 | ADSP-21571 |
|---|---|---|
| ASTATx | bits 0..=14, 18, 24..=31 stick | bits 0..=13, 18, 24..=31 (SF, bit 14, does not) |
| ASTATy | all 32 bits stick | same as ASTATx (bits 14..=17 and 19..=23 do not) |
| STKYx | bits 0..=20 and 27..=31 | bits 0-2, 5-9 and 17-20 |
| STKYy | bits 0..=20 and 27..=31 | bits 0-2, 5-9, 17 and 18 |

The stack-status bits (21..=26 read `0x0540_0000` on an empty stack) are the same. Conditions (`IF LT/LE/GT/GE` with AF and ALUSAT) agree with the model on 40 sweep cases, and the whole PNO bank routine `0x122620` is bit-exact on SHARC+ (`sharc-routine-diff.md`).

`Core::sharc_plus` selects the masks; `enable_sc57x` (`sharc-run --sc57x`) sets it, and a `plus` line in a census cases file does too.

## Implications
The 21489 masks apply to SY1SY2 and SMP (ADSP-2148x); FX, PNO and ORG are SHARC+ and use the second column.

## Follow-ups
Re-run the census (`census_diff.py --target 21571`), the P1 to P8 peripheral probes and the timing probes on this target.

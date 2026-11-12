# SHARC fixed-point ALU, multiplier/MR, shifter and CACC (ADSP-21489 simulator)

Checked: 2026-11-12.
Sources: CCES 2.12.1 ADSP-21489 simulator (full licence), probe `docs/research/sharc-fixed/` (generator `gen_fixed_probe.py`, six builds, result `fixed_measured.csv`, 14 712 rows); ADSP-2136x/2137x/214xx Programming Reference rev 2.4 (3-14..3-20, 11-49 ff.).
Question: how do the fixed-point ALU, the fixed-point multiplier with the MR registers, the shifter and the compare accumulator (CACC) behave on the real simulator, compared with the model in `crates/nord-sharc` (flags, saturation, rounding, STKY)?

## Method

* `gen_fixed_probe.py <build> <dir>` writes one assembly probe per build (`alu0`/`alu1` with MODE1 ALUSAT clear/set, `mul0..mul2` (four formats each), `shift`). Each record clears ASTAT and STKY, presets the operands and (for MR forms) the 80-bit accumulator, runs one instruction, then stores R0, MR0..MR2, ASTAT and STKY. The generator assembles every instruction with `easm21k` and reads its **compute word back with `elfdump -n seg_pmco`**, so the Rust test executes exactly the word the simulator ran (16-bit short forms, e.g. `COMP`, `PASS`, `Rn = Rx + 1`, run through `short_compute`).
* `cc21k -proc ADSP-21489 -si-revision any -g -O0 -o <build>.dxe <build>.asm <build>.c`, run in the simulator, Memory Browser > Dump Memory of `_out` (address from `elfsyms`), `build_csv.py` joins the dumps with the layout into `fixed_measured.csv` (columns `build,mode1,name,short,word,x,y,p0,p1,p2,r0,mr0,mr1,mr2,astat,stky`).
* Coverage: 144 operand pairs (12 values) x 26 ALU operations x 2 ALUSAT settings; 49 operand pairs (7 values) x all 12 multiplier formats x {multiply, accumulate, subtract with two extreme accumulator presets, `Rn =` forms}; shifter operations x 12 values x 12 shift amounts or 12 field specifiers; 19 MR operations (SAT, RND, transfers, clear) x 12 accumulator presets.

## Findings (model before -> measured)

Fixed-point ALU (all rows matched after the change; ALUSAT clear and set):
* AC is the carry out; a subtraction is `x + !y + cin`, so `x - x` carries; negate is `0 - x`. AV is a signed overflow and latches STKY AOS (bit 2). The model set only AV/AC of add. Compare, logic, `PASS`, `MIN`, `MAX`, `CLIP` clear AV, AC and AS; `ABS` sets AS for a negative operand and AV for 0x80000000.
* `(x+y)/2` rounds half to even (0.5 -> 0, 1.5 -> 2, -0.5 -> 0); its AC is the carry of `x + y + 1` when the tie rounded up, else of `x + y`. The model truncated.
* With ALUSAT set add, subtract, `ABS`, negate, increment and decrement saturate on overflow.
* `CLIP x BY y` with y = 0x80000000 does not clip (the magnitude is 2^31); the model wrapped.

Multiplier and MR (all rows matched):
* The product is shifted left by one **only for the signed x signed fractional format** (SSF); SUF/USF/UUF are not shifted. The 80-bit MR wraps; the rounding formats round half to even at bit 32.
* `Rn = ...` stores MR0 (integer) or MR1 (fractional) **without saturation** and leaves MRF/MRB unchanged; the multiplier does not touch AZ/AN. The model saturated.
* Flags: MN is the sign of the 80-bit result; MV a result outside 32 bits (integer) or 64 bits (fractional), signed or unsigned per format (signed window for the mixed formats); MU a nonzero fractional result whose MR1 is all zeros or ones; MI is never set. MV latches STKY MOS (bit 6); MU latches nothing on a multiply.
* `SAT MRx (SI|UI|SF|UF)`: the opcode bit 0 is **1 for signed** (the model had it inverted). An out-of-window value clamps to the window's minimum (negative, signed) or maximum (otherwise, so a negative value in an unsigned format gives the maximum). MN follows the result; MU is computed as for the multiplier and latches STKY MUS (bit 8).
* `RND MRx (SF|UF)`: round half to even at bit 32; MN is the sign of the result, MV an out-of-window result or input, or a carry out of the 80-bit rounding addition of a negative value (-1 rounds to 0 with MV); MV latches STKY MVS (bit 7).

Shifter flags (SZ bit 12, SV bit 11, SS bit 13; everything else already matched):
* `BSET`, `BCLR`, `BTGL`, `BTST` set SV for a bit position outside 0..31 (negative too; the model only checked above 31).
* `ASHIFT` left sets SV only when a significant bit is lost; `LSHIFT` left sets it for any left shift (the model used the LSHIFT rule for both). The OR form of `ASHIFT` by 32 or more sets SV even for a zero operand.

FIX/TRUNC and compares:
* A NaN input of `FIX`/`TRUNC` gives all ones with AI only (no AN). A nonzero input that converts to zero latches STKY AUS (bit 0).
* **CACC** (ASTAT bits 24..31) is a shift register: `COMP`/`COMPU` (fixed or float, any operation form) shift `Rx > Ry` in at bit 31 and move the older results down; a NaN operand counts as greater, denormals compare as zero; no other operation changes it (9 212 transitions in the float probe and 288 compare rows). The model did not have it.

## Implications

* `fixed_alu.rs` (new, the fixed ALU moved out of `alu.rs`), `multiplier.rs` (rewritten), `shifter.rs`, `float_conv.rs`, `float_misc.rs` follow the measurement; `fixed_measured.rs` runs every CSV row (R0, MR0..MR2, ASTAT without AF, STKY) through `Core::compute` / `short_compute`.
* Firmware that relied on the multiplier saturating `Rn` results (assumed before) behaves differently: only the explicit `SAT` forms saturate.
* Limits of the data: the `OR` shifter forms were measured with R0 = 0 (a harness slip: the preset was overwritten), so their OR-with-R0 behaviour is untested; MRB/SRCU, SIMD (PEy) flags, the integer-rounding multiplier forms and the `Rn = Rx + CI` / `+ CI - 1` single-operand forms were not measured; the AUS rule for FIX is derived from one operand (a tiny float) and may be broader (any result that rounds to zero) or narrower; `MI` is never set by fixed-point operations in this data.

## Follow-ups

* Re-run the OR forms with a nonzero R0, measure MRB (`MODE1` SRCU), SIMD flags and STKY after FIX overflow.
* Differential run of real firmware routines on the simulator and on the oracle.

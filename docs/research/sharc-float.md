# SHARC float fidelity (ALU and multiplier)

Checked: 2026-11-12.
Sources: CCES 2.12.1 ADSP-21489 simulator (seeds and rounding, see below); ADSP-2136x/2137x/214xx SHARC Processor Programming Reference rev 2.4 (`.reference/adsp-2136x_2137x_214xx_pgr_rev2.4.pdf`, pages 3-37..3-39, 11-22..11-44, MODE1 table A-5); CCES 2.12.1 runtime sources `SHARC/lib/src/libcc_src/lib_fdiv.asm`; ADI `elfdump` listings of the six DSP SRAM images (see `dsp-image-hunt.md` 5.34 to 5.36).
Question: how exactly do the SHARC float ALU and multiplier round, flush and flag, and what does the oracle (`crates/nord-sharc`) have to model to reproduce firmware results?

## Gaps found before this change

| Item | State before | State now |
|---|---|---|
| `Fn = RECIPS Fx` (0xC4), `RSQRTS` (0xC5) | unmodelled (fault) | modelled, seed tables VERIFIED (simulator) |
| `Rn = MANT Fx` (0xAD) | unmodelled | modelled |
| `Rn = LOGB Fx` (0xC1) | zero, infinity unmodelled | modelled incl. ALUSAT forms |
| `Fn = RND Fx` (0xA5) | treated as `PASS` | rounds to a 24-bit significand |
| 40-bit extended precision | f32 only | 8 extra mantissa bits kept per register |
| MODE1 `TRUNC` (bit 15), `RND32` (bit 16), `ALUSAT` (bit 13) | ignored | honoured by add/sub/avg/mul/FLOAT/FIX/RND/SCALB/LOGB |
| Denormal inputs and results | host f32 semantics | inputs flushed to zero, results below 2^-126 give zero with AZ |
| NaN, infinity | host f32 semantics | NaN gives all ones with AI; inf-inf is invalid |
| Flags | AZ, AN (and AV for fixed add) | AZ, AN, AV, AC (cleared), AS, AI per op; sticky bits (STKY) not modelled |
| Multiplier flags MN, MV, MU, MI | not modelled | still not modelled |
| `FPACK`, `FUNPACK` | unmodelled | still unmodelled (shifter lane, not touched) |
| ASTAT register image | no ASTAT register | unchanged: flags live in `Flags` only |

## Rules implemented (ASSUMED unless noted; the rounding rules are VERIFIED, see the next section)

* Register float is 40 bits: sign, 8-bit exponent, 31-bit mantissa. With `RND32 = 0` (reset state) results round to a 31-bit fraction, with `RND32 = 1` input low 8 bits are cleared and results round to 23 bits (3-37; the text at 3-37 says "default RND32 = 1 is 40-bit", the MODE1 table says 1 selects 32-bit rounding; the table is followed, and the firmware never sets the bit statically, see below).
* `TRUNC = 1`: round toward zero for ALU ops and float multiplies; overflow returns the largest normal instead of infinity. `TRUNC = 0`: nearest, ties to even (3-38).
* `FLOAT` always rounds to a 40-bit boundary. `RND` rounds to 32 bits. `FIX` rounds to nearest even, or toward minus infinity when `TRUNC = 1`; `TRUNC` always rounds toward zero.
* `FIX/TRUNC` overflow: `ALUSAT = 1` saturates to 0x7FFFFFFF / 0x80000000; `ALUSAT = 0` returns all ones with AI. AV is set when exponent plus scale is 31 or more.
* Storing a register to memory keeps the upper 32 bits (truncation). The model keeps the low 8 bits in `ExtFile`, tagged with the 32-bit value they belong to, so any other write (load, fixed result, move) drops them. Exact zero sums are `+0`.
* Exact arithmetic: add/sub use a 128-bit aligned sum with a sticky bit; multiply is a 64-bit product; one rounding at the end, so results are platform independent.

## Seeds (`RECIPS`, `RSQRTS`): measured

Checked: 2026-11-12. Source: CCES 2.12.1 ADSP-21489 simulator (full licence), probe in `docs/research/sharc-seeds/`.

The reference gives `RECIPS` an 8-bit accurate seed with exponent `-e-1` and `RSQRTS` a 4-bit accurate seed with exponent `-floor(e/2)-1`; the ROM is unpublished. Measured on the simulator:

* Procedure: `seed_probe.asm` (`RECIPS`/`RSQRTS` over 22 exponents x 128 fraction indices x 2 inputs, low edge `k<<16` and `(k<<16)|0xFFFF` of each bin, plus 12 special values with `ASTAT` read after each op) and `seed_probe.c` (calls it, nothing else) built with `cc21k -proc ADSP-21489 -si-revision any -g -O0 -o probe.dxe seed_probe.asm seed_probe.c`. Loaded in a CCES Debug Configuration (CrossCore Debugger, Target ADSP-21489 Simulator), run to completion, then Memory Browser > Dump Memory (text, hexadecimal, 16 957 words from the `_flags_rec` address 0xB2022, found with `elfsyms probe.dxe`) to a file; the dump is split into `seeds_full.csv` (input, RECIPS, RSQRTS as 32-bit words, 5 644 rows) and `seeds_specials_flags.csv`.
* Both seeds depend only on the index, never on the exponent or the low bit pattern of the bin (every pair of bin-edge inputs and every exponent from 2^-126 to 2^125 agree). `RECIPS`: 7 fraction MSBs index 128 entries; `RSQRTS`: exponent LSB plus 6 fraction MSBs index 2 x 64 entries.
* Both seeds have **8 fraction bits** (mantissa bits 22..15, the rest zero). `RECIPS` entries are even except the first (0xFF), i.e. 7 effective bits; worst relative error 0.00745 (< 2^-7) against the documented 2^-8.
* Exponent rules match the model: `RECIPS` gives `-e-1` for `e` up to 125 and zero with AZ from `e` = 126; `RSQRTS` gives `-floor(e/2)-1` (including 2^126 and 2^127). Negative inputs keep their sign in `RECIPS`; `RSQRTS` of a negative non-zero input is all ones.
* Flags (ASTAT 0x1 AZ, 0x2 AV, 0x4 AN, 0x20 AI): zero gives AV (and AN for -0), infinity gives AZ (AN for -inf in `RECIPS`), a denormal input is treated as zero (AV), NaN gives all ones with AI only, and `RSQRTS` of a negative non-zero input (including -inf) gives all ones with **AI only (no AN)**; `RSQRTS(-0)` gives -infinity with AV and AN.

Measured versus the earlier ASSUMED model: the old `RECIPS` table had 9 fraction bits (so it differed from the ROM in the low bit of most entries) and the old `RSQRTS` table 4 bits (the ROM has 8); the exponent rules and special values were right except that `RSQRTS` of a negative input and NaN no longer set AN. The tables in `crates/nord-sharc/src/core/float_seeds.rs` are the measured ones; `seed_tables_match_every_measured_row` runs `recips`/`rsqrts` over every CSV row and compares the 32-bit result bit for bit, `measured_flags_of_the_special_inputs` checks the 12 flag words.

## Rounding, FIX/TRUNC and the 40-bit extension: measured

Checked: 2026-11-12. Source: same CCES 2.12.1 ADSP-21489 simulator; probe `docs/research/sharc-round/round_probe.asm` + `round_probe.c`, result `round_measured.csv` (912 rows).

* Procedure: `cc21k -proc ADSP-21489 -si-revision any -g -O0 -o round.dxe round_probe.asm round_probe.c`; run in the simulator; Memory Browser > Dump Memory of the `_out` block (address from `elfsyms`, 4 017 words incl. the done flag). For each of the 8 MODE1 combinations (TRUNC 0x8000, RND32 0x10000, ALUSAT 0x2000, written with `MODE1 = Rn`, two NOPs after): 24 floats x 3 scales (0, 4, -4) through `FIX`/`TRUNC` with ASTAT; 26 increments `b` added to 1.0 and read back through `(1.0+b) - 1.0` in registers (the subtraction shows the low 8 bits); 8 multiplies; 8 `FLOAT` of integers.
* `FIX`: nearest, ties to even (0.5 -> 0, 1.5 -> 2, 2.5 -> 2, 3.5 -> 4); with MODE1 TRUNC set it rounds toward minus infinity (-0.5 -> -1, -1.5 -> -2, -2.5 -> -3, 0.5001 -> 0). `TRUNC` always rounds toward zero. Overflow: all ones with AV and AI (no AN) when ALUSAT is clear, 0x7FFFFFFF / 0x80000000 saturated when set; the exact -2^31 is a valid result (AN only, no AV). The model already did all of this except two flag details, now fixed.
* 40-bit extension: with RND32 clear a sum keeps 31 fraction bits (1 + 2^-30 minus 1 gives 2^-30); ties round to even at the 31st bit (1 + 2^-32 gives 1.0, 1 + 1.5 * 2^-31 gives 1 + 2^-30). With RND32 set results round to 23 fraction bits. Multiplies and `FLOAT` behave the same (`FLOAT` of 2^30 + 1 keeps the 1 in every mode: it always rounds at 40 bits). TRUNC chooses truncation at that boundary. `RND` rounds to 32 bits. The model matched every one of these rows unchanged, so the earlier ASSUMED rules held.
* Extension survival: a register-to-register move (`F7 = F0`) and `PASS` keep all 40 bits; a store to memory and reload keep only the upper 32 (the model dropped the extension on moves before; fixed in `Core::move_ureg`). An extension created with RND32 clear and consumed with RND32 set (the probe's variant b) is handled by the model identically.
* Not measured: `Rx <-> Sy` swaps, SCALB, MIN/MAX/CLIP on extended values, multiplier flags.

Test: `float_measured.rs::model_matches_every_measured_round_row` runs the model's `to_int`, `add`, `mul`, `from_int`, `rnd` over every CSV row, bit for bit and ASTAT included; `register_move_keeps_the_extension` covers the move fix.

## What the firmware uses

Census of the ADI `elfdump` listings (these are linear listings of whole images, so data words decode as junk and the counts are upper bounds): `RECIPS` 6 to 58 per image, `RSQRTS` 1 to 5, `COPYSIGN` up to 114, `(Fx+Fy)/2` up to 97, `CLIP Fx BY Fy` up to 338, `TRUNC` up to 23, one `MANT` and one `LOGB` in the FX and PNO core1 images. `MODE1` is changed by `BIT SET/CLR MODE1` with masks 0x2000 (ALUSAT, 48 times), 0x200000 (PEYEN), 0x1000, 0x800, 0x4F8, 0x1C04000 and others; no static write touches TRUNC (0x8000) or RND32 (0x10000), so the firmware runs in the reset mode: round to nearest, 40-bit. This is why the 40-bit extension matters more than `RND32`. Smoke check: `sharc-run` on the ORG Core2 image (`--lenient`, 3 000 000 steps) skips no operation.

## Implications

* Float results are now bit exact for the single-rounding operations under the stated rules, in the firmware's 40-bit mode, and the seed ROMs are measured. Open: the sticky flags (STKY), multiplier flags MN/MV/MU/MI, CACC, and the flag columns marked ASSUMED (the PDF text extraction scrambles several flag tables, so AZ/AN/AV/AS/AI were assigned in the order the reference lists them).
* Register-to-register moves between data registers keep the extension (VERIFIED, see below); swaps `Rx <-> Sy` still drop it (not measured).

## Follow-ups

* Compare a few thousand random operand pairs per op against the simulator to settle ties, zero signs and flags.
* Model MN/MV/MU/MI and the STKY register once the interrupt lane adds ASTAT/STKY.

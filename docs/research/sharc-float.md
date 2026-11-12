# SHARC float fidelity (ALU and multiplier)

Checked: 2026-11-12.
Sources: ADSP-2136x/2137x/214xx SHARC Processor Programming Reference rev 2.4 (`.reference/adsp-2136x_2137x_214xx_pgr_rev2.4.pdf`, pages 3-37..3-39, 11-22..11-44, MODE1 table A-5); CCES 2.12.1 runtime sources `SHARC/lib/src/libcc_src/lib_fdiv.asm`; ADI `elfdump` listings of the six DSP SRAM images (see `dsp-image-hunt.md` 5.34 to 5.36).
Question: how exactly do the SHARC float ALU and multiplier round, flush and flag, and what does the oracle (`crates/nord-sharc`) have to model to reproduce firmware results?

## Gaps found before this change

| Item | State before | State now |
|---|---|---|
| `Fn = RECIPS Fx` (0xC4), `RSQRTS` (0xC5) | unmodelled (fault) | modelled, seed table ASSUMED |
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

## Rules implemented (all ASSUMED unless noted)

* Register float is 40 bits: sign, 8-bit exponent, 31-bit mantissa. With `RND32 = 0` (reset state) results round to a 31-bit fraction, with `RND32 = 1` input low 8 bits are cleared and results round to 23 bits (3-37; the text at 3-37 says "default RND32 = 1 is 40-bit", the MODE1 table says 1 selects 32-bit rounding; the table is followed, and the firmware never sets the bit statically, see below).
* `TRUNC = 1`: round toward zero for ALU ops and float multiplies; overflow returns the largest normal instead of infinity. `TRUNC = 0`: nearest, ties to even (3-38).
* `FLOAT` always rounds to a 40-bit boundary. `RND` rounds to 32 bits. `FIX` rounds to nearest even, or toward minus infinity when `TRUNC = 1`; `TRUNC` always rounds toward zero.
* `FIX/TRUNC` overflow: `ALUSAT = 1` saturates to 0x7FFFFFFF / 0x80000000; `ALUSAT = 0` returns all ones with AI. AV is set when exponent plus scale is 31 or more.
* Storing a register to memory keeps the upper 32 bits (truncation). The model keeps the low 8 bits in `ExtFile`, tagged with the 32-bit value they belong to, so any other write (load, fixed result, move) drops them. Exact zero sums are `+0`.
* Exact arithmetic: add/sub use a 128-bit aligned sum with a sticky bit; multiply is a 64-bit product; one rounding at the end, so results are platform independent.

## Seeds (`RECIPS`, `RSQRTS`)

The reference gives: `RECIPS` returns an 8-bit accurate seed with exponent `-e-1`, mantissa from a ROM indexed by the 7 fraction MSBs; `RSQRTS` returns a 4-bit accurate seed with exponent `-floor(e/2)-1`, ROM index = exponent LSB plus the 6 fraction MSBs. The ROM contents are not published; CCES 2.12.1 contains only users of the instruction (`lib_fdiv.asm`, `sqrtf.asm`), not the table.

Model (ASSUMED): each entry is the function value at the centre of its input interval (harmonic centre for the reciprocal), quantised to 9 fraction bits (`RECIPS`) or 4 (`RSQRTS`). Checked by `recips_seed_is_within_the_documented_accuracy` (error at most 2^-8 + 2^-10 over 28 672 inputs) and `rsqrts_seed_is_within_the_documented_accuracy` (at most 2^-4), and by `fdiv_iteration_of_the_runtime_library_gives_a_correctly_rounded_quotient` (the `lib_fdiv.asm` recurrence reaches 1 ulp from the modelled seed). These prove the accuracy bound and the algorithms converge; they do not prove the ROM bits.

What confirms it: run `F1 = RECIPS F0` / `F2 = RSQRTS F0` for every 7-bit index (and both exponent parities) on the ADI simulator or hardware and compare the seed bits. Exact experiment: assemble `f0 = recips f1; f2 = rsqrts f3;` for ADSP-21489 (`easm21k -proc ADSP-21489`, which runs headless: checked 2026-11-12, produces a 1000-byte `.doj`), link it with the default LDF, load in the CCES simulator, loop `F1` over `1.0 + k/128` (k = 0..127) and `2.0 + k/64`, and read `F0`/`F2`. The install has no command-line simulator executable (only the IDE debugger and `sim10x.dll`), so this needs the CCES GUI with a licence active; not done here.

## What the firmware uses

Census of the ADI `elfdump` listings (these are linear listings of whole images, so data words decode as junk and the counts are upper bounds): `RECIPS` 6 to 58 per image, `RSQRTS` 1 to 5, `COPYSIGN` up to 114, `(Fx+Fy)/2` up to 97, `CLIP Fx BY Fy` up to 338, `TRUNC` up to 23, one `MANT` and one `LOGB` in the FX and PNO core1 images. `MODE1` is changed by `BIT SET/CLR MODE1` with masks 0x2000 (ALUSAT, 48 times), 0x200000 (PEYEN), 0x1000, 0x800, 0x4F8, 0x1C04000 and others; no static write touches TRUNC (0x8000) or RND32 (0x10000), so the firmware runs in the reset mode: round to nearest, 40-bit. This is why the 40-bit extension matters more than `RND32`. Smoke check: `sharc-run` on the ORG Core2 image (`--lenient`, 3 000 000 steps) skips no operation.

## Implications

* Float results are now bit exact for the single-rounding operations under the stated rules, in the firmware's 40-bit mode. Open: seed ROM bits, the sticky flags (STKY), multiplier flags MN/MV/MU/MI, CACC, and the flag columns marked ASSUMED (the PDF text extraction scrambles several flag tables, so AZ/AN/AV/AS/AI were assigned in the order the reference lists them).
* Register-to-register moves and swaps between float registers drop the extension (hardware keeps 40 bits).

## Follow-ups

* Run the simulator experiment above and replace the two seed tables with the measured ones, then upgrade the tags to `VERIFIED`.
* Compare a few thousand random operand pairs per op against the simulator to settle ties, zero signs and flags.
* Model MN/MV/MU/MI and the STKY register once the interrupt lane adds ASTAT/STKY.

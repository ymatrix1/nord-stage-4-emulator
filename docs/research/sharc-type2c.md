# SHARC Type 2c short multiply and an exhaustive compute sweep against ADI

Checked: 2026-11-12.
Sources: ADI `elfdump` / `easm21k` of CCES 2.12.1 (licence active) as disassembler ground truth; the six flat SRAM images (`tools/ns4_dsp_ldr.py --sram`) and the SY1SY2/SMP images (`tools/ns4_dsp_raw.py --sram`) of OS v1.66; `docs/research/sharc-fixed.md` (simulator-measured multiplier/MR behaviour, reused unchanged); `docs/research/org-dsp-map.md`.
Question: which compute forms does ADI know that the oracle still rejects or prints differently, and what stops the oracle on each image after they are fixed?

## Method

* `adi_compute.tsv` (2 048 rows): every value of compute bits 22..12 with `Rn = 3`, `Rx = 1`, `Ry = 2`, wrapped as the 48-bit Type 2a word `0x13E0_0000_0000 | compute` and disassembled by `elfdump -ns seg_pmco` (`tools/adi_dis.py`, `visa=False`). This covers every ALU, multiplier, shifter and multifunction opcode, because the opcode lives in bits 22..12 and the register fields in 11..0.
* `adi_short.tsv` (4 096 rows): every 16-bit Type 2c word `0xC000 | OP:Rn:Rx` through the VISA path of `elfdump`.
* Registers `Rn = 10`, `Rx = 5`, `Ry = 12` were run too (multifunction field mapping, same result, not committed).
* `core/compute_adi.rs` (test only) loads both files and requires, for every row ADI names: the same text from `compute_text` / `short_compute_text` (normalised: case, blanks, parentheses, order of the two halves of a dual add/subtract) and `Core::compute` / `Core::short_compute` returning true. Reserved codes (ADI prints nothing) must stay undecoded and unexecuted.
* Real images: `tools/sharc_census.py` `census_core` over the six flat images, `sharc-run --lenient --skips` start-ups and pass recipes, `sharc-org` with every routine pointer found in the ORG data tables selected in slot 0 (`--poke 1:0xC10B8=PTR --poke 1:0xC10B9=PTR`).

## Findings

| # | Finding | Tag |
|---|---|---|
| 1 | The oracle executed only 15 of the 16 Type 2c operations: OP 7 `Rn = Rn * Rx (SSI)` was missing from `short_op`. Words `c7dc` (`R13 = R13 * R12`, `0x12032E`) and `c701` (`R0 = R0 * R1`, `0x124C9A`) of the ORG image faulted. It is the long `Rn = Rx * Ry (SSI)` (multiplier opcode `0x70`) with `Ry = Rx`, `Rx = Rn`. | VERIFIED (`ssi_short_multiply_is_the_long_multiply`, sweep) |
| 2 | Every other ALU, shifter, float, fixed multiplier, MR-transfer and multifunction compute ADI names was already decoded and executed, with the exceptions in 3 to 5. All 4 096 short computes decode to ADI's text. | VERIFIED (`every_adi_compute_is_decoded_and_executed`, `every_adi_short_compute_is_decoded_and_executed`) |
| 3 | The four integer rounding multiplier formats `SSIR SUIR USIR UUIR` (opcode bit 0 set with the integer flag; 48 opcodes: four formats, plain/`+`/`-` accumulate, four result selectors `Rn` (MRF/MRB accumulator), `MRF`, `MRB`) are accepted by `easm21k` and printed by `elfdump`, but `fixed_mult` returned false. Now executed with the rounding stage of the fractional `R` formats (round half to even at bit 32, so MR0 and an `Rn` result read zero). No image uses them (census: only `SSI SSF SSFR UUI UUF` occur in code). | ASSUMED (not measured on the simulator; the CCES simulator probe of `sharc-fixed.md` would settle it) |
| 4 | Text only, execution was already right: `SAT` and `RND` printed the sign letter inverted (`R3 = SAT MRF (UI)` is opcode `0x00`, `(SI)` is `0x01`, same for `RND ... (SF/UF)`); the Rust and Python decoders had the old inversion that `sharc-fixed.md` fixed in execution only. The fixed-point triple `Rm = Rxm * Rym (SSFR), Ra = Rxa + Rya, Rs = Rxa - Rya` lacked ADI's `(SSFR)`. | VERIFIED (ADI text, sweep) |
| 5 | ADI names six registers behind the multifunction transfer codes `0x08 0x09 0x0C..0x0F` (+`0x10` for the write): `Rn = SHP, SHF, FTA, FTB, FTC, FR` and back. They are not multiplier registers and no image uses them (no hit in the census or in any run). Left undecoded and unexecuted; the sweep asserts that. | VERIFIED (ADI), open (semantics) |
| 6 | Cosmetic differences left on purpose: ADI prints the destination of `Fm = Fxm * Fym, Ra = FIX Fxa BY Rya` as an F register, and `FUNPACK` as `r3 = funpack f1`, which `easm21k` rejects (`Fn = FUNPACK Rx` is the accepted form, `Rn = FPACK Fx` likewise). | VERIFIED (`easm21k` error text) |

## Runs after the change

Strict unless noted; commands as in `sharc-leftovers.md`, `sharc-sequencer.md`, `org-dsp-map.md`, `pno-dsp-map.md`; ORG Core1/Core2, FX, PNO gates as in those documents.

| Image / recipe | Result | First unmodelled instruction |
|---|---|---|
| FX Core1 start-up, 3M steps | idle loop `0x12123B` | none |
| FX Core2 start-up, `--dev 0x28300b00=1`; mailbox poke `0xc02c2=1` at step 1M | runs the handler, stops at step 1,394,509 at pc `0x120000` | none: `CALL` through a null function pointer (`I8 = 0` at `0x121866`, the uninitialised table of `sharc-sequencer.md`) |
| PNO Core1 start-up; `--irq P4I@300000` | pass starts, stops at step 693,638 at pc `0x120000` | none: `CALL (M14, I13)` with `I13 = 0` (return address `0x121467`), the module table is ARM-provided |
| PNO Core2 start-up and mailbox poke | poll loop `0x125DDB..0x125DDF` | none |
| ORG Core2 start-up and 4 passes (`sharc-org`) | 4 264 steps per pass, back in the poll loop | none, skipped ops 0 |
| ORG Core2, slot 0/1 = `0x120300` (the unpack routine of `c7dc`), `0xC1DDC = 0x01020304` | pass 1 now runs 4 991 steps (before: fault at `0x12032E`), back in the poll loop; the routine rewrites `DM(0xC1DDC..)` to `0x01020084 0x80` | none |
| ORG Core2, each of the 19 routine pointers found in the ORG data tables, 3 passes strict | 8 complete; `0x120300 0x1206BE 0x120BF7 0x120600 0x12538C 0x125B79 0x120F0C 0x12063A 0x123900 0x1243F7 0x12082F` return to the poll loop; 9 stop at pointers that are no code entry (`0x12023E 0x120F03 0x120F04 0x121980 0x128300 0x124500 0x129008 0x12063E`, `0x120C68` loops 200,000 steps): decoded data words, not compute gaps | none |
| same, `--lenient` | skipped ops 0 everywhere except the data entry `0x12063E` (2 skipped ops in non-code) | none |
| ORG Core1 boot 3M + 2 passes (`sharc-org`, gates of `sharc-sequencer.md` 3) | back in the poll loop `0x125DF3` | none |
| SY1SY2, SMP ISA boot kernel, `--isa`, 3M steps | `IDLE` at `0x8C090`, "nothing can wake it" | none |
| SY1SY2 `0x1c0b2d`, SMP `0x1c11e0`, `--flag 12=1`, 1M steps | idle loops `0x1C0B33`, `0x1C11FD` | none |
| `--enter 20000:PC` into SY1SY2 `0x1c05b9 0x1c03fd 0x1c058c`, SMP `0x1c0ee0 0x1c03fd 0x1c0a71` | back in the idle loops | none |
| SMP `--enter 20000:0x1c058c`, with and without `--poke 19000:0x5201=0x40 --poke 19000:0x5200=0` | poll `0x1C08ED` / idle `0x1C11FD` | none |

Result: no compute word of any image is unmodelled any more. Every remaining stop is data the ARM provides (function-pointer tables and module configuration: FX Core2 `I8 = 0`, PNO Core1 `I13 = 0`, ORG slot records and `0xC1DDC`), not an instruction gap.

## Implications

* The ORG parameter-unpack routine `0x120300` and any slot that selects it can now be run; its byte-unpack semantics are the next verification (`org-dsp-map.md` follow-up 1).
* The decoders (`visa/compute.rs`, `tools/sharc_visa.py`) and the executor agree with ADI on the whole compute space, enforced by the sweep, so a new opcode in a future image cannot be silently mis-decoded.
* `fixed_mult` accepts the integer rounding formats on an ASSUMED basis; promote it with one more simulator build (`sharc-fixed/gen_fixed_probe.py` formats list plus `ssir suir usir uuir`).

## Follow-ups

1. Measure `SSIR/SUIR/USIR/UUIR` and the `SHP/SHF/FTA/FTB/FTC/FR` transfers on the CCES simulator (`FMTS` of `gen_fixed_probe.py`).
2. Provide ARM-side tables (FX Core2 `0xC02C2` handler pointers, PNO Core1 module table) so the passes run past the null calls, via `sharc-org --msg` as in `dsp-rx-protocol.md`.
3. Verify the `0x120300` unpack output against the Python model of the byte layout.

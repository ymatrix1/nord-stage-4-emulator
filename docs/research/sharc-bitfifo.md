# SHARC shifter bit FIFO (`BITDEP`, `BITEXT`, `BFFWRP`)

**Checked:** 2026-11-12.
**Sources:** ADI `elfdump` of CCES 2.12.1 through `tools/adi_dis.py` (method: `dsp-image-hunt.md` 5.34 to 5.36); *ADSP-2136x/2137x/214xx SHARC Processor Programming Reference* rev 2.4 (`.reference/adsp-2136x_2137x_214xx_pgr_rev2.4.pdf`): section 3 "Bit Stream Manipulation Instructions" (3-26 to 3-29), "Bit FIFO Status" (3-30), Table 3-9, Table 4-37, Table 10-4 and Table A-6 (ASTAT), section 11 pages 11-86 to 11-91; the six DSP images (`tools/ns4_dsp_ldr.py --sram`).
**Question:** how are the bit FIFO operations encoded, what state and flags do they have, and does the Nord Stage 4 firmware use them?

## 1. Encodings (VERIFIED against ADI)

Method: wrap synthetic compute words in a Type 2a instruction (`013e` + compute field) and shift-immediate words (`023e ...`), disassemble with `elfdump`, read the text back. Probed all 256 shifter opcodes and shift-immediate codes 20..31; only the forms below exist (`0x50..0x5F` and `0x70..0x7F` have no other opcode).

Register forms (compute field: cu = 2 in bits 22..20, opcode bits 19..12, `Rn` 11..8, `Rx` 7..4, `Ry` 3..0):

| Opcode | ADI text (n=3, x=1, y=2) | Operands used |
|---|---|---|
| `0x50` | `r3=bitext r2` | length = `Ry[5:0]` (**`Ry`, not `Rx`**) |
| `0x58` | `r3=bitext r2 (nu)` | same, FIFO not updated |
| `0x70` | `r3=bffwrp` | none (`Rn` = pointer) |
| `0x74` | `bitdep r1 by r2` | data `Rx`, length `Ry[5:0]` |
| `0x7C` | `bffwrp=r2` | pointer = `Ry[6:0]` |

Shift-immediate forms (opcode `0x02`, `SHIFTOP` bits 21..16, `DATA12` = bits 30..27 high and 15..8 low, `Rn` 7..4, `Rx` 3..0):

| `SHIFTOP` | ADI text | Used |
|---|---|---|
| 20 | `rn=bitext N` | `N = DATA12[5:0]` |
| 22 | `rn=bitext N (nu)` | `DATA12[5:0]` |
| 29 | `bitdep rx by N` | `DATA12[5:0]` |
| 31 | `bffwrp=N` | `DATA12[6:0]` |

ADI prints `bitext 0x3f` for `DATA12 = 0x07F` and `0xFFF` alike and `0` for `0x80`/`0x800`, so the upper `DATA12` bits are ignored. Codes 21, 23, 24, 26, 28, 30 disassemble to nothing (reserved).

The previous decoders were wrong in three ways, now fixed in `crates/nord-sharc/src/visa/compute.rs` and `tools/sharc_visa.py`: the register operand of `BITEXT` is `Ry` (it printed `Rx`), `0x70` was named `BFFWRP1` and `0x7C` (`BFFWRP = Ry`) was missing, and the immediates printed all 12 bits. Check: `tools/adi_compare.py`-style comparison of 74 synthetic words (10 register, 64 immediate) gives 74 of 74 equal; golden tests in `tools/tests/test_sharc_visa.py` and `visa/compute.rs`.

## 2. State, flags, semantics (manual; VERIFIED = taken from the manual, no hardware)

| Item | Model | Tag |
|---|---|---|
| FIFO | `bff: u64` (oldest bit = bit 63) and `wrp` (0..64 valid bits), one per PE (inside `Flags`, so the PEx/PEy swap exchanges it) | manual (VERIFIED text) |
| `BITDEP` | `bff \|= (x & mask(len)) << (64 - wrp - len)`, `wrp += len` (manual equation on 11-87) | manual |
| `BITEXT` | `Rn = bff[63:32] >> (32 - len)`; unless `(NU)`: `bff <<= len`, `wrp -= len` (11-90/91) | manual |
| `BFFWRP` | `Rn = wrp`; `BFFWRP = value[6:0]` | manual |
| `SF` | `wrp >= 32`; read-only, no storage. `IF SF` / `IF NOT SF` are condition codes 14 and 30 (Table 10-4: `BM/SF`; ADI prints them as `bm`) | manual + ADI cond probe |
| `SZ` | `BITEXT`: result is 0; others cleared | manual |
| `SV` | `BITDEP`: length > 32; `BITEXT`: length > 32 or more bits asked than valid; `BFFWRP =`: value > 64; `Rn = BFFWRP`: cleared | manual |
| `SS` | cleared by all four | manual |

ASSUMED (the manual says "undefined"): a length above 32 sets `SV` and changes nothing (`BITEXT` returns 0); `BITDEP` without room for `len` more bits sets `SV` and changes nothing; `BITEXT` of more bits than valid returns the top 32 bits' field, sets `SV` and leaves `wrp` at 0; `BFFWRP =` above 64 stores 64; the FIFO register is zero at reset and `BFFWRP =` does not clear it (the manual's save and restore sequence, listing 3-3, works either way). What would confirm them: a hardware run of the over-range cases.

The manual's listing 3-3 restores with `BITDEP R2; BITDEP R1`, i.e. in the reverse order of the extraction; the unit test restores in extraction order and then writes the saved pointer, which rebuilds the saved state exactly. Treat the manual's order as a typo (ASSUMED).

Not modelled: the `ASTATx/y` register bit 14 (`SF`, read-only; the core does not expose `ASTAT` yet); the "stack push saves SF" remark; the `SF` interrupt condition (`SF = 1` overflow, Table 4-37 footnote) because interrupts belong to another lane.

## 3. Use by the firmware (negative result)

Counting `BITEXT`/`BITDEP`/`BFFWRP` in the code spans of the six core images (the same spans `tools/sharc_census.py` uses: `sharc_visa.sweep` over bytes `0x240000..0x3A0000`, one-off script, not committed; spans `0x241c00..0x24a800` (FX core 1), `0x241c00..0x249400` (FX core 2), `0x241c00..0x245c00` + `0x248c00..0x24b800` (PNO core 1), `0x240400..0x250c00` + `0x303c00..0x304c00` (PNO core 2, ORG core 1 and 2)):

| Image | Instructions in code spans | Bit FIFO instructions |
|---|---|---|
| FX core 1 / core 2 | 7,477 / 6,487 | 0 / 0 |
| PNO core 1 / core 2 | 5,767 / 15,205 | 0 / 0 |
| ORG core 1 / core 2 | 15,200 / 15,207 | 0 / 0 |

(Total 65,343 instructions.) The ADI listing of the *whole* flat images (`adi_sram_*` listings in the session scratchpad, which also decode the data regions) shows 9 bit FIFO lines in PNO core 2, ORG core 1 and ORG core 2 and none in FX and PNO core 1, all at short-word addresses `0x9EC4..0xBC76` (bytes `0x13D88..0x178EC`), identical in the three images, with undecodable parcels in between (`** fail to disassemble **`) and nonsense neighbours (random conditions such as `if mv`, `rti (db,lr)` next to `jump`). That is the data area below the code, misread as code, not program text. Conclusion: **the shipped firmware does not use the bit FIFO**; the model exists for completeness (the ADSP-21489 has it) and costs the code paths nothing.

## Implications

- No firmware path depends on this model, so its ASSUMED corner cases do not affect any trace; the oracle stays strict-clean on the images either way (strict mode used to fault on these opcodes).
- The `SF` condition (code 14) is now evaluated; before, `IF SF` and `IF NOT SF` read as always false and always true.

## Follow-ups

- Hardware or a vendor simulator run for the undefined cases (over-length, underflow, overflow, `BFFWRP =` above 64) if a future program needs them.
- Expose `ASTATx/y` (with `SF` as bit 14) when system register access is modelled.
- Interrupt-driven `SF` condition (Table 4-37) with the interrupt lane.

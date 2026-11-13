# Census diff: every firmware instruction, model against simulator

Checked: 2026-11-12. Sources: CCES 2.12.1 ADSP-21489 simulator (headless, `sim_run.py`/`sim_batch.py`), the five DSP images (`tools/ns4_dsp_images.py`), ADI `elfdump` (`tools/adi_dis.py`).
Question: does `crates/nord-sharc` execute every distinct instruction the firmware uses exactly like the simulator?

## Method
`tools/sharc_census.py` tiles the code of the five images (FX, PNO, ORG, SY1SY2, SMP; 6 core sections). `census_diff.py` takes every distinct instruction (parcels), labels it with ADI's own disassembly and drops what ADI does not decode to the same parcels (the tiler cuts data into "instructions"), control flow, timer/stack/interrupt registers and MODE1 writes. 8 287 instructions remain; each runs in 6 random core states (all registers, ASTATx/y with AZ and AN never together, random STKYx/y, MODE1 in 6 variants incl. SIMD, ALUSAT, TRUNC, no RND32; special values mixed in).

`gen_census_probe.py` builds one probe: a loader that sets the whole state from a DM table, one patchable instruction slot (`_slot`), a dumper (107 state words) and a 512-word scratch table. `sim_batch.py` pokes each instruction into the slot of the simulator (`TpsdkServer`, 14 to 16 processes in parallel, 20 minutes for 49 722 cases); `sharc_census_run` (`core/census.rs`) runs the same machine code on a `Core`. `census_diff.py` reports every difference by instruction shape.

## Result
First run: 1 875 of 50 052 cases differed. After the fixes below: **18 of 49 722** (the "Open" list), found and fixed model gaps listed here. All are unit-tested; `census_regress.rs` replays 1 104 cases (278 shapes of the affected families and a 288-case sweep of every condition code over AZ AV AN AC x AF) against the committed simulator output.

| # | Behaviour (VERIFIED by the census) | Model before |
|---|---|---|
| 1 | Loading Bx also loads Ix | B write left I alone |
| 2 | STKYx and STKYy keep bits 0..20 and 27..31 of a written value (stack-status bits 21..26 read-only); ASTATy keeps all 32 bits (ASTATx drops 15..17 and 19..23); STKYy reads the same stack-empty bits as STKYx | masks 0x3E7/0x1E0000, STKYy status 0 |
| 3 | Fixed ALU and fixed COMP clear AI | AI left alone |
| 4 | Every shifter operation clears SF (ASTAT bit 14) | SF kept |
| 5 | Long-word access (`(LW)`: bit 39 of Types 14a, 15a, 15b; bit 30 of 3a, 3b) moves a register pair at `addr & !1`; the named register gets the lower word | word access |
| 6 | SIMD: a store of a non-data register also writes `addr + 1`; an S register transfers like an R register with the roles swapped; immediate loads (Type 17) and immediate stores (Type 16) reach the twin register / next word; STKY immediates also set STKYy | PEx only |
| 7 | SIMD register moves copy the twin (`Sn = Rm` also sets `Rn = Sm`); a conditional move/transfer follows PEx's condition for the R side and PEy's for the S side | one condition |
| 8 | A register transfer and a compute in one instruction both read the old registers (`F12 = F8 * F7, R8 = R0`) | transfer first |
| 9 | Conditions LT, LE, GE, GT depend on AF and ALUSAT: after a float ALU op (AF = 1) LT = AN and not AZ, LE = AN or AZ; AF = 0 with ALUSAT: LT = AN, LE = AN or AZ; AF = 0 otherwise LT = AN xor AV, LE = (AN xor AV) or AZ; GE, GT are the negations | LT = AN and not AZ always |
| 10 | Dual-memory form with a multiplier-plus-ALU compute leaves MN MV MU MI and STKY 6..9 alone; multiplier-only updates them; `MRF = 0`/`MRB = 0` clear MN MV MU MI | always updated |
| 11 | Triple operation (multiply, add, subtract): ALU flags are the OR of sum and difference | flags of the difference |
| 12 | A shift count of `i32::MIN` must not overflow | panic in debug builds |

Provenance in code: each fix carries a `VERIFIED` tag naming the census; items marked ASSUMED stay ASSUMED (AF cleared by a fixed ALU op; the SIMD long-word case).

## Open (18 cases of 49 722, 6 shapes, listed by `census_diff.py`)
* `STKYx = DM(addr) (LW)`: the model writes the pair to the wrong registers (1 instruction).
* `IF NOT MS F0 = F0 * F4, F0 = F8 + F12, F15 = F8 - F12, R14 <-> S3` (multiply, dual add/subtract and swap under a condition, 5 cases).
* `IF EQ R0 = LSHIFT R12 BY 0x100` and `IF EQ R2 <-> S0` in SIMD (per-element condition on shifter flags and swaps).
* `MR1F = R3, R1 = DM(...)`: MR2 reads back zero-extended in the simulator.
* `IF EQ R0 = FPACK F0, M12 = DM(...)` in SIMD.
* Long-word loads in SIMD mode (the simulator also changes S0).

## Implications
Several of these hit common firmware code: conditional ops after float arithmetic (9), dual-memory multiply-accumulate (10, 11), shifts before bit-field tests (4), index-register setup with `B` writes (1), the FX start-up (`ASTAT`/`STKY` writes, 2).

## Follow-ups
Close the Open list; run the census on the ADSP-21571 cycle-accurate simulator to compare SHARC+ with the 21489 core; extend the state generator to circular buffers (L > 0) and to hardware loops.

## Reproduce
```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out IMG
cd docs/research/sharc-seq
python gen_census_probe.py OUT && cc21k -proc ADSP-21489 -si-revision any -g -O0 -o OUT/cens.dxe OUT/cens.asm OUT/cens.c
python dxe2fixture.py OUT/cens.dxe census_fixture.txt --code 124300 1245D0 --data B2000 B2001 --entry 124300 --exit 1245CE --out B206C 107 \
  --sym _slot 124462 --sym _st B2001 --sym _res B206C --sym _scr B20D7 --sym _done B2000   # addresses from elfsyms
python census_diff.py IMG OUT/cens.dxe census_fixture.txt --work WORK --jobs 16
python census_regress.py IMG OUT/cens.dxe census_fixture.txt --work WORK     # rebuilds census_cases/measured
```

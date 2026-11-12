# Research: SHARC sequencer and addressing leftovers (DO UNTIL, stack instructions, jump modifiers, Type 25, broadcast, CB7I/CB15I)

- **Checked date:** 2026-11-12
- **Sources (local, never committed):** SHARC Processor Programming Reference rev 2.4 (`.reference/adsp-2136x_2137x_214xx_pgr_rev2.4.pdf`, pages cited as `PRM 4-53`); ADI `elfdump` of CCES 2.12.1 as disassembler ground truth, with probe instructions assembled by `easm21k -proc ADSP-21489` (licence active) and read back through `elfdump -ns seg_pmco` (`tools/adi_dis.py` for raw words); the SY1SY2 and SMP images of OS v1.66 (`docs/research/sharc-sy1sy2-smp.md`); `sharc-run`.
- **Question:** Which of the sequencer and addressing forms that `docs/research/sharc-sequencer.md` and `docs/research/sharc-sy1sy2-smp.md` list as unmodelled can the oracle now run, with what evidence, and what stops the SY1SY2/SMP boot kernel and the SMP record dispatcher?

## Verdict

1. Type 13a `DO ... UNTIL cond`, `PUSH/POP LOOP|STS|PCSTK`, the `(LA)`/`(CI)` jump modifiers, `RTS (LR)`/`RTI (LR)`, `CJUMP`/`RFRAME`, the `BDCST1`/`BDCST9` broadcast loads and the `CB7I`/`CB15I` circular buffer overflow interrupts are modelled. `BITREV` (Type 19a) was already modelled by the sequencer work; it is re-verified here against ADI (section 1).
2. The SY1SY2/SMP boot kernel now runs to its final `IDLE` at ISA `0x8C090` (171,826 steps, strict): it arms a DMA receive of the block stream and sleeps. Nothing in the oracle delivers that DMA, so the run ends "halted in IDLE (nothing can wake it)"; that is a peripheral gap, not an instruction gap.
3. The SMP record dispatcher at `0x1C058C` needs no initialisation by the program: its function-pointer table is the first data block of the image, `DM(0x92180..)` = `{0x40, 0, 0x1C0300, 0x1C031F, 0x1C0355, ...}` (16 handler entries at `0x92182 + n`). The earlier note "uninitialised" was wrong: the table is loaded with the image and `I1 = 0x92182` is set by the dispatcher itself. The strict run stops where the handler polls an IOP status register, `ustat3 = DM(0x5201)`, bit 6 (loop at `0x1C08ED`); with that bit set (`--poke 19000:0x5201=0x40`) the dispatcher, its handler and the return to the idle loop run strict.
4. While comparing against ADI one existing decoder/executor bug surfaced and was fixed: the 48-bit `RTS`/`RTI` keep the delay flag in bit 26 (the 16-bit form in bit 38). `0a3e 0400 0000` is `rts (db)` in ADI; the core used bit 38 for both and returned at once, skipping the delay slots. The 48-bit form also carries a compute (`rts (db), r2=r3+r4`, `, ELSE compute` with bit 25), which was not executed.

## 1. ADI ground truth (probes assembled with `easm21k`)

Instruction at the left, `elfdump` parcels, bit position derived. All of it is VERIFIED by golden tests (`visa/flow_text.rs`, `core/*.rs` tests, `tools/tests/test_sharc_visa.py`); `adi_compare.py` reports 0 differences for types 8a, 9a/9b, 11a/11c, 12a, 19a on the SY1SY2 code blocks and the FX, PNO and ORG code ranges.

| Source | Parcels | Reading |
|---|---|---|
| `do (pc,7) until eq` / `ne` / `lce` / `forever` | `0e00 0000 0007` / `0e20` / `0e1e` / `0e3e` | opcode `0x0E`, TERM = bits 37..33 as the `IF` condition field; 31 prints `forever`; end address `pc + reladdr23` (bit 23 ignored, as in Type 12a) |
| `do (pc,-7) until av` | `0e08 00ff fff9` | |
| `push loop`, `pop loop` | `1780`, `1740` (+ `0000 0000`) | Type 20a: bit 39 push loop, 38 pop loop, 37 push sts, 36 pop sts, 35 push pcstk, 34 pop pcstk, 33 flush cache |
| `push sts`, `pop sts`, `push pcstk`, `pop pcstk` | `1720`, `1710`, `1708`, `1704` | |
| `push loop, push sts, push pcstk` | `17a8` | listing order: loop, sts, pcstk, flush cache |
| `pop pcstk, flush cache` | `1706` | |
| `17c0` | prints `push loop` | a push wins over a pop of the same stack; `1700` prints an empty line |
| `jump (m8,i8)(ci)` | `083e 013f` | Type 9: CI bit 24, DB bit 26, LA bit 38 (jumps only); `(db,la)` = `087e 043f` |
| `jump (pc,4)(ci)`, `(la)`, `(db,la,ci)` | `073e 0100 0004`, `077e 0000 0004`, `077e 0500 0004` | Type 8 modifiers in the same bits; `jump (pc,4)` with bit 25 set prints no modifier |
| `rts (lr)`, `rts (db,lr)`, `rti (lr)` | `0abf`, `0aff`, `0bbf` | 16-bit Type 11c: DB bit 38, LR bit 32 |
| 48-bit `rts (db)`, `rts (lr)` | `0a3e 0400 0000`, `0a3e 0100 0000` | Type 11a: DB bit 26, LR bit 24, ELSE bit 25; bits 38 and 32 mean nothing |
| `cjump (pc,0x10)(db)`, `cjump 0x1234(db)` | `1844 0000 0010`, `1804 0000 1234` | Type 25: relative = bit 38, DB = bit 34 (always set by the compiler); ADI prints no space before `(db)` |
| `rframe` | `1900 0000 0000` and 16-bit `1901` | the classifier gave `1901` three parcels; it is one |
| `i1=bitrev (i2,0x10)` | `169a 0000 0010` | bit 39, destination `(37..35) xor (34..32)`, as modelled since the sequencer work |
| `bit set mode1 0xe00000` | `1402 00e0 0000` | the firmware's `PEYEN|BDCST9|BDCST1` (MODE1 bits 21, 22, 23) |

`easm21k` aliases `jump (m8,i8)(lr)` to `(ci)` and `rts (db,ci)` to `(db,lr)`; the two modifiers are the same bit in the assembler's eyes only for these forms.

## 2. Semantics added

| Feature | File | Behaviour | Status |
|---|---|---|---|
| `DO ... UNTIL cond` | `core/arith_loop.rs` | pushes a loop frame with a termination code. The condition is evaluated on the flags after the body instruction of index `(N - 4) mod N`; when it holds, three more instructions run and the loop is left after the last body instruction. The PRM example (`R5=6, R6=10, R7=14`, six instructions, result `0 / 4 / 8`) reproduces; one instruction loops run three more iterations, two and three instruction loops one more pass (PRM 4-58). `FOREVER` never terminates. SIMD: the condition must hold in PEx and PEy | EMPIRICAL against the PRM example and rules (4-53, 4-58); first pass, branches inside the body and cycle-exact flushes are ASSUMED |
| Conditions `FLAG0..3_IN` | `core/mod.rs` | `IF` and `DO UNTIL` conditions 9..12 read the flag pin levels | ASSUMED (PRM Table 10-4) |
| `PUSH/POP LOOP|STS|PCSTK` | `core/pcstack.rs` | return addresses and loop entries share one PC stack (`cs_depth`, `detached`); `PCSTK`, `LADDR`, `CURLCNTR` can be read and written, so the save and restore sequence of PRM Listing 4-6 round-trips two nested loops. `PUSH STS` applies `MMASK` to `MODE1` (PRM A-44, interrupt mode section) | encodings VERIFIED; stack ordering ASSUMED (PRM 4-75, 4-76) |
| `JUMP (LA)` | `core/branch_ext.rs` | pops the innermost loop when taken | encoding VERIFIED, behaviour PRM 4-71 |
| `JUMP (CI)` | `core/branch_ext.rs` | ends the interrupt service (status stack popped, in-service state cleared), keeps the return address for `RTS (LR)` | encoding VERIFIED, behaviour PRM 4-36 |
| `RTS (LR)` | | same as `RTS` (the flag only freezes the hardware loop counter) | VERIFIED text |
| 48-bit `RTS/RTI (DB)`, compute, ELSE | `core/branch.rs` | DB bit 26, compute executed, bit 25 selects ELSE | VERIFIED text, ELSE ASSUMED |
| `CJUMP`, `RFRAME` | `core/branch_ext.rs` | `CJUMP`: `R2 = I6` (and `S2 = I6` in SIMD), `I6 = I7`, delayed jump; `RFRAME`: `I7 = I6`, `I6 = DM(I6)` | encodings VERIFIED; Table 9-2 of the PDF is garbled, `RFRAME` ASSUMED |
| `BDCST1/BDCST9` | `core/dag_modes.rs` | a data register load through `I1` (BDCST1) or `I9` (BDCST9) writes `Rn` and `Sn` with the same word, whatever PEYEN says; stores and non-data registers unaffected | PRM 6-24, A-6; bit numbers fit the firmware's `0xe00000`; extended and long word cases not modelled |
| `CB7I`, `CB15I` | `core/dag_modes.rs` | a circular buffer operation on `I7`/`I15` that wraps sets `STKYx.CB7S`/`CB15S` and latches vector 30/31 (`IRPTL` bits 20/21) when the instruction ends | PRM 6-30, 6-31; the `L = 0` special case is not modelled |
| ISA `IDLE` | `core/exec.rs` | the 48-bit word `0080 0000 0000` | VERIFIED text (ADI prints `idle`) |

Not changed: `Type 19a` `BITREV` (modelled before, golden test in `core/dag.rs`), `FLUSH CACHE` (no cache).

## 3. Runs

All strict (no `--lenient`), `sharc-run` built from this branch; images from `tools/ns4_dsp_raw.py --sram`.

| Run | Result |
|---|---|
| `sharc-run DIR/dsp_image_3_SY1SY2.bin 0x8c005 3000000 --isa` | 171,826 steps, pc `0x8C090`, "halted in IDLE (nothing can wake it)"; before: fault at `0x8C083` (`DO ... UNTIL EQ`) |
| SY1SY2 start-up `0x1c0b2d` `--flag 12=1`, 100,000 steps | idle loop `0x1C0B33` |
| SMP start-up `0x1c11e0` `--flag 12=1`, 100,000 steps | idle loop `0x1C11FD` |
| `--enter 20000:PC` into SY1SY2 `0x1c05b9`, `0x1c03fd`, `0x1c058c` (400,000 steps) | back to the idle loop, no fault |
| `--enter 20000:PC` into SMP `0x1c0ee0`, `0x1c03fd`, `0x1c0a71` | back to the idle loop, no fault |
| SMP `--enter 20000:0x1c058c` | leaves the code of the dispatcher, calls a handler and polls `DM(0x5201)` bit 6 at `0x1C08ED` |
| same with `--poke 19000:0x5201=0x40 --poke 19000:0x5200=0` | returns to the idle loop `0x1C11FD`, 21 device reads, 8 device writes |
| FX, PNO, ORG Core1 and Core2 start-up (entry from the reset vector, gates of `sharc-sequencer.md` section 3, 3,000,000 steps) | register file and pc identical to the build before this work (regression check) |

The dispatcher walks records through `I0` (`r0 = DM(I0, M1)`, `FEXT` of the opcode nibble at bit 28), loads `I8 = DM(M5, I1)` with `I1 = 0x92182` (a pre-modify access: address `0x92182 + nibble`) and calls `IF NE CALL (M8, I8)`. A real run needs the record buffer in `DM(0x92418..)` and the count words `DM(0x92416)`, `DM(0x92419)` filled by the ARM side; the stand-in `--enter` leaves `I0`, `R3` and `R2` as the idle loop left them, so the handler that ran is not a meaningful one.

## 4. Provenance summary

| Item | Status | Basis |
|---|---|---|
| Type 13a, 20a, 25, jump/return modifier bit positions and text | VERIFIED | ADI `elfdump` of `easm21k` output (table in section 1), golden tests |
| 48-bit Type 11a modifier bits (DB 26, LR 24) | VERIFIED | ADI; `0a3e 0400 0000` occurs in FX and PNO (byte-range offsets `0x33a8` and `0x370f` from `0x120200`, formerly the one `different` row of `adi_compare.py`) |
| Arithmetic loop evaluation point | EMPIRICAL | PRM 4-53 example reproduced; first pass, SIMD, branches in the body ASSUMED |
| PC stack ordering, `LADDR` termination code `0x0F` for counter loops | ASSUMED | PRM 4-75, no data (code `0x0F` carried over from the sequencer work) |
| `JUMP (CI)` clears exactly the in-service state and pops the status stack | ASSUMED | PRM 4-36 text |
| `RFRAME` register transfers | ASSUMED | PRM Table 9-2 is garbled in the PDF |
| Broadcast and `CB7I/CB15I` semantics | ASSUMED | PRM text only; no firmware use of `CB7I/CB15I` found |

## Implications

- The two last DSP images run their start-up and every interrupt routine probed so far strictly. What remains before real work runs is peripheral state: the boot kernel's DMA receive of the block stream and the IOP status registers the handlers poll (`DM(0x5201)`), not instruction gaps.
- The SMP dispatcher and its 16-entry handler table (`0x92182`) are the entry point for the ARM-to-DSP message path; the table needs no model, only the record buffer.

## Follow-ups

- Model the DMA receive of the boot kernel (`DM(0x1080)` and the `0x30024` control register) to run the block stream through the oracle instead of loading the flat image.
- Find the writer of the SMP record buffer and the count words `DM(0x92416)`, `DM(0x92419)`, and drive the dispatcher with real records.
- Measure the arithmetic loop evaluation point on the CCES simulator (CCES licence is active) for first-pass, SIMD and in-body branch cases, and promote ASSUMED to VERIFIED.
- `CB7I/CB15I` with `L = 0`, extended-precision and long-word broadcast loads, `EMUIDLE`, `IICDI`, the extra interrupt cycle of PRM 4-43 (still listed in `sharc-sequencer.md`).
- `T10a` (jump with dual move) is still raw text in the decoder (2 rows in FX Core1).

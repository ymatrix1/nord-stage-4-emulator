# SHARC ADSP-214xx core model (Track B oracle)

Specification for a test-oracle core that executes single routines of the real DSP images. It is **not** a full-system emulator and it does not replace the high-level engine ([D2](../DECISIONS.md)). Scope and counts come from `tools/sharc_census.py` (`docs/research/dsp-image-hunt.md` 5.19).

## 1. Goal

Run a routine of the FX, PNO, ORG, SY1SY2 or SMP image on test vectors and compare its output with the Rust engine, so that engine constants and algorithms can be checked against the firmware. Typical use: load a core's SRAM image, set registers and memory, call a routine address, run until it returns, read the result.

## 2. Inputs

| Input | Source |
|---|---|
| Per-core SRAM image | `tools/ns4_dsp_ldr.py --sram` (flat image indexed by `address & 0x7FFFFF`, byte `2 * short_word_address`); mapping VERIFIED, `dsp-image-hunt.md` 5.18 |
| Entry value | section header of the loader stream (short-word address) |
| Instruction encoding | SHARC Processor Programming Reference rev 2.4, chapters 9 to 12 (`.reference/`, not committed); decoder in `tools/sharc_visa.py` |
| Memory map | ADSP-21489 datasheet Table 5 (internal blocks, short, normal and 48-bit aliasing) |

## 3. State to model

- Register file: `R0..R15` / `F0..F15` (PEx) and `S0..S15` (PEy, SIMD), 40-bit data with the float extension, `MRF/MRB/MSF/MSB` multiplier accumulators.
- Data address generators: `I0..I15`, `M0..M15`, `L0..L15`, `B0..B15` (circular buffers, bit-reverse), pre-modify and post-modify addressing.
- Sequencer: `PC`, PC stack, loop stack (`LCNTR`, `CURLCNTR`, `DO ... UNTIL LCE`), delayed branches, status stack, `MODE1`, `MODE2`, `ASTAT`, `STKY`, `USTAT1..4`, `IMASK`, `MMASK`, `IRPTL`.
- Memory: four internal SRAM blocks as one byte array with short-word, normal-word (32-bit) and instruction-word (48-bit) views; the external SDRAM window (`0x20xxxxxx` loader addresses) as a plain array until its mapping is known.

## 4. Instruction scope (from the census)

Implement first, in this order:

1. Moves and data memory: `Ureg = Ureg`, `Ureg = <imm>` (types 17a/17b), `DM/PM(imm, Ii)`, `DM/PM(Ii, Mm)` with pre and post modify, direct `DM(addr)`. About 21% of all census instructions are memory moves.
2. Float compute: multiply (13,515 plain, 383 with modifiers), add (9,314), subtract (4,372), `PASS`, `ABS`, `CLIP`, `COPYSIGN`, `MAX/MIN`, `(a+b)/2`, `RECIPS`, `RSQRTS`, `FLOAT`, `FIX`, `COMP`; multifunction forms (multiplier plus ALU, dual ALU).
3. Control: `DO UNTIL LCE`, `CALL`, `JUMP`, `RTS`, conditional execution (`IF cond`), `NOP`, `IDLE`.
4. Bit and integer: `AND/OR/XOR`, `FDEP/FEXT`, `LSHIFT/ASHIFT`, `MODIFY`, `BIT SET/CLR/TST` on system registers.
5. SIMD: PEy registers, `Dreg <-> CDreg` swap, the `MODE1` PEY enable bit.

Instruction decoding for all used forms is a prerequisite. `tools/sharc_visa.py` currently decodes the type for 98.7% and the operands for 95.4% of the instructions in the large blocks. The remaining forms (1a, 7a/7b, 9a/9b, 10a, 16a/16b, parts of 15b) must be completed first.

## 5. Milestones and acceptance

| Milestone | Acceptance |
|---|---|
| M1 decoder | Full operand decoding into a Rust enum; round trip against the `tools/sharc_visa.py` listings on all code spans. **Status:** `crates/nord-sharc/src/visa/` (submodules `types`, `classify`, `regs`, `compute`, `decode`) ports the Python decoder (type, length and text for every form the Python tool decodes) and its `sharc-disasm` output is identical to the Python listing on about 970,000 instruction lines of four core SRAM images; still open: structured operands (an enum) for the execution core and the forms the Python tool leaves raw |
| M2 core | Registers, DAGs, loops and calls; one unit test per instruction from the semantics in the manual  **Status:** `crates/nord-sharc/src/core/` (submodules `memory`, `dag`, `alu`, `multiplier`, `moves`, `branch`, `exec`) implements moves, DM/PM addressing with circular buffers, `MODIFY`, `Dreg <-> CDreg` swaps, system-register bit operations, jumps and calls with delay slots, `DO UNTIL LCE` loops, the single-function ALU and short compute (float and fixed) and the fixed-point multiplier with MRF/MRB; SIMD (`PEy` registers `S0..S15`, per-element flags and conditions, data transfers at `addr + 1`, immediates broadcast, MODE1 bit 21), the barrel shifter (`shifter.rs`: shifts, rotates, single-bit ops, `FDEP`/`FEXT`, `EXP`, `LEFTZ`/`LEFTO`), multifunction compute (`multifunction.rs`: multiplier plus ALU, triple op, dual add/subtract) and `SAT`/`RND` of MRF/MRB are modelled; interrupts, DMA, shift immediate (Type 6a), `FPACK`/`FUNPACK`, shift immediate (`0x02` form) and the bit FIFO (`bitfifo.rs`: `BITDEP`, `BITEXT`, `BFFWRP`, `IF SF`; per-PE state; [findings](../research/sharc-bitfifo.md)) are modelled; DMA faults explicitly; **Sequencer** (`interrupts.rs`, `vectors.rs`, `timer.rs`, `sysregs.rs`, `stacks.rs`, `banks.rs`; [research](../research/sharc-sequencer.md)): interrupt controller with IVT dispatch, nesting, `RTI`, `IDLE`, status/PC/loop stacks with depth limits, core timer, `EMUCLK`, ASTAT/STKY/FLAGS, secondary register sets, `CBUFEN`, bit-reversed addressing, Type 19a; `sharc-run --irq N@STEP --flag N=LEVEL`; float ALU/multiplier fidelity (40-bit extended precision, TRUNC/RND32, denormal/NaN/INF rules, `RECIPS`/`RSQRTS`/`MANT`/`LOGB`, AI/AS flags) is modelled in `float*.rs`, seed tables ASSUMED, see `docs/research/sharc-float.md` (`sharc-run --lenient` skips and counts compute faults, `--skips` lists them) |
| M3 start-up | Run the entry code of each core section until it reaches its idle loop with no unknown instruction. **Status: done for FX Core1/Core2 and ORG Core1/Core2** (`dsp-image-hunt.md` 5.21): each reaches its main loop (peripheral poll or command mailbox); PNO is not run yet **SY1SY2 and SMP** (`research/sharc-sy1sy2-smp.md`): the ISA-mode boot kernel (`sharc-run --isa`) and both programs run their start-up to a `JUMP (PC, 0)` idle loop once FLAG0 is high (`--flag 0=1`); their work is interrupt-driven (`--enter STEP:PC` enters a handler body). |
| M4 routine test | Call one pure routine (for example a biquad or the sinc interpolation) with test vectors and match the expected result |
| M5 host protocol | Reverse enough of the ARM-to-DSP messages (`DSP Comm`, `bank0_CODE@0x6CA48`, more than 1,000 call sites) to set one effect's parameters |
| M6 engine comparison | Render an effect through the oracle and compare it with the Rust engine |

## 6. Risks

- Float exactness: 40-bit extended precision and the rounding modes (`RND32`, `TRUNC`) are modelled (`docs/research/sharc-float.md`); the seed ROM bits are not verified. Tolerance-based comparison is expected; bit-exactness is a stretch goal.
- Unknown peripherals: DMA and SPORT behaviour are not modelled (interrupts are raised from outside with `Core::raise_interrupt` / `--irq`); routines that depend on them cannot run in isolation.
- External memory: delay and reverb lines live in the SDRAM window whose address mapping is still open (`dsp-image-hunt.md` 5.18).
- Provenance: the oracle is only as good as the decoding; every newly decoded field needs a test that fails without it.

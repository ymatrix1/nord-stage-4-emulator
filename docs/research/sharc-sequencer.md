# Research: SHARC sequencer and system registers (interrupts, timer, stacks, IDLE) in the oracle

- **Checked date:** 2026-11-12
- **Sources (local, never committed):** `.reference/adsp-2136x_2137x_214xx_pgr_rev2.4.pdf` (PRM, pages cited as `PRM 4-27`), the six flat SRAM images written by `tools/ns4_dsp_ldr.py --sram` (`sram_{0_FX,1_PNO,2_ORG}/core{1,2}_sram.bin`), ADI `elfdump` listings of them (`tools/adi_dis.py`, CCES 2.12.1, method of [dsp-image-hunt](dsp-image-hunt.md) 5.21-5.36), `sharc-run` runs.
- **Question:** Which sequencer and system-register features does `crates/nord-sharc/src/core` lack, which of them does the real firmware rely on, and what do the idle loops of the Core1/Core2 sections wait on?

## Verdict

1. The firmware uses a small, regular part of the sequencer: the interrupt vector table with **two** maskable interrupts per section (`P0I`, and `P4I` on the Core1 sections), `RTI`, `MODE1.IRPTEN`, `IMASK`, the core timer used as a stopwatch, `EMUCLK`, `FLAGS` pins as a board handshake and `BIT TST ASTATx`. It never uses `IDLE`, `PUSH/POP STS`, `CJUMP`/`RFRAME`, `BITREV`, `JUMP (CI)`, `RTS (LR)`, `PCSTK`, `LADDR`, `PX` or software interrupts in code (the hits in the ORG/PNO listings are data decoded as code).
2. The Core1 main loops are **interrupt driven**: the `P4I` service routine stores the status register `DM(0x3108945C)` in the mailbox the loop polls (`DM(0xC02C1)` in FX, `DM(0xB00E1)` in ORG). Raising `P4I` with a non-zero status wakes the loop (VERIFIED by `sharc-run`, below).
3. The Core2 main loops are **not** woken by an interrupt: no code in the Core2 sections writes the polled word (`DM(0xC02C2)` in FX, `DM(0xB00E2)` in ORG) except the clear after use, and the only service routine (`P0I`) stores a status in a different word. The mailbox is written from outside (the ARM or a DMA through the external port). The earlier observation that a poke does not wake FX Core2 came from the start-up gate `DM(0x28300B00)` (see below), not from the idle loop.

## 1. Audit of `core/` before this work

| Feature | State before | Now |
|---|---|---|
| IRPTL / LIRPTL / IMASK / IMASKP, priorities, IVT dispatch | `imask` stored, nothing else; no dispatch | `interrupts.rs`, `vectors.rs`: 42 vectors, latch/mask/pointer bits per Table A-12/A-13, dispatch to `ivt_base + 12 * vector` |
| Nesting (`NESTM`), `IRPTEN`, delay-slot deferral | none | modelled |
| `RTI` | popped the call stack like `RTS` | also ends the service, pops the status stack, clears the mask pointer |
| Status stack (ASTATx/y, MODE1, `MMASK`) | none | `stacks.rs`: 15 deep, pushed for `IRQ2-0` and timer interrupts, `MMASK` clears MODE1 bits (reset value `PEYEN`) |
| PC / loop stack depth, overflow, STKY bits, `SOVFI` | unbounded `Vec`s | 30 / 6 deep, `PCFL/PCEM/SSEM/LSEM/SSOV/LSOV`, `SOVFI` at 29 of 30 |
| Timer `TCOUNT/TPERIOD/TIMEN`, `TMZHI/TMZLI` | registers read 0 | `timer.rs`, lazy evaluation (no per-instruction cost) |
| `EMUCLK`, `EMUCLK2` | read 0 | core clock (instructions + stall cycles) |
| `IDLE` | faulted | halts until an interrupt is taken, fast-forwards to the next event |
| ASTATx/y, STKYx/y, FLAGS, PX/PX1/PX2, PCSTK, PCSTKP, LADDR, CURLCNTR | read 0 / ignored | `sysregs.rs`, `stacks.rs` |
| Secondary register sets (MODE1 `SRRFL/H`, `SRD1L/H`, `SRD2L/H`) and `SRCU` | ignored: one bank | `banks.rs`: the visible arrays hold the active set, toggling a select bit swaps with the hidden set |
| Circular buffers: `CBUFEN` | `L`/`B` always applied | applied to data accesses only while `CBUFEN` is set; `MODIFY` always |
| Type 19a `Id = MODIFY(Is, imm)` / `BITREV` | fault (stopped FX Core2 at `0x1213AB`) | `dag.rs` |
| Bit-reversed addressing `BR0`/`BR8` | none | `dag_out`, applied to every data access through `I0`/`I8` |
| Arithmetic exception interrupts `FIXI/FLTOI/FLTUI/FLTII` | none | `Core::note_arith_exception` sets STKY and latches; the ALU/multiplier lanes still have to call it |

**Still missing** (none is used by the firmware code): `PUSH/POP STS` and `JUMP (CI)` / `RTS (LR)` instruction forms (encodings unverified, not in the images); circular-buffer overflow interrupts `CB7I/CB15I` and `STKY.CB7S/CB15S`; `IICDI` (illegal IOP access, `MODE2.IIRAE`) and the unaligned 64-bit access check; `EMUIDLE` and the emulator interrupts; `CJUMP`/`RFRAME`; the extra cycle of a higher-priority interrupt right after a lower one (PRM 4-43); cycle-exact interrupt latency (instruction cache, pipeline stalls); `MODE1` `BDCST1/BDCST9` broadcast loads (the firmware sets both together with `PEYEN`: `bit set mode1 0xe00000`) and `ALUSAT/SSE/TRUNC/RND32` (compute lanes); `FADDR`/`DADDR` (read as the pc); a write to `LADDR`/`LCNTR`; the PEy view of the system registers of Table 10-3.

## 2. What the firmware does (VERIFIED from the ADI listings)

The six images share one start-up and one set of service routines (listing offset + `0x120200` = short-word address):

| Item | Evidence |
|---|---|
| Vector table at short-word `0x120000`, 32 vectors x 12 parcels, ISA parcel order | all six images: vector 1 (RSTI) word 1 is `JUMP entry`; unused vectors are `RTI` (Core2) or a jump to self |
| `P0I` (vector 11, `0x12002C`): `BIT CLR MODE2 0x20` (stops the timer), `JUMP isr` | all six; ISR reads `DM(0x31022204)` into a scratch word (FX Core2: `DM(0xC03D3)`, compared with `0x28300830` in the main flow) |
| `P4I` (vector 15, `0x12003C`), Core1 only: `DM(mbox+2) = R0; R0 = DM(0x3108945C); DM(0x3108945C) = R0; JUMP isr` | FX Core1 ISR `0x122FC1`: `DM(0xC02C1) = R0` (the status) ... `RTI` |
| Enable: `BIT SET IMASK 0x800` (+ `0x8000` on Core1), `BIT CLR IMASK 0x8` (SOVFI off), `BIT CLR MODE1 0x800` (no nesting), `BIT SET MODE1 0x1000` (IRPTEN) | one routine per section (FX Core1 `0x122F91`, called before the idle loop) |
| Timer: `TPERIOD = 0xFFFFFFFF`, `TCOUNT = 0x2400`, `BIT SET MODE2 0x20` (TIMEN is bit 5 in this PRM) | start-up; the `P0I` vector stops it again, so it measures one interval |
| `R0 = EMUCLK` (`76BE 803F`) | 8-12 per section, differences stored as min/max in `DM(0xB6294..0xB629E)` |
| `BIT SET FLAGS 0x1; BIT TST FLAGS 0x4; IF NOT TF JUMP` | Core1 handshake: raise `FLG0` (bit 0), wait for the input `FLG1` (bit 2) high; FLAGS interleaves value and direction bits (`FLGx` = bit `2x`, `FLGxO` = `2x+1`) |
| `BIT TST ASTATx 0x20` (AI) after float blocks | FX 48 times |
| `BIT SET MODE1 0x1C004F8` / `BIT CLR MODE1 0x4F8` around register initialisation | both register sets are initialised (secondary banks) |

## 3. Idle loops: experiments (`sharc-run`, strict mode)

`--irq N@STEP` (N = vector number or mnemonic) raises an interrupt at core cycle STEP; `--flag N=LEVEL` drives a FLAG pin. Gates that the run needs: Core1 FX `--dev 0x3108d008=5 --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 --dev 0x28b00b00=1 --flag 1=1`; Core2 FX `--dev 0x3108d008=5 --dev 0x28300b00=1`.

| Run | Result |
|---|---|
| FX Core2 without `--dev 0x28300b00=1` | spins at `0x12121B..0x12121F` (`R3 = DM(0x28300B00); PASS; IF EQ JUMP`), never reaches the mailbox loop: this is why a poke "did not wake" it |
| FX Core2 with the gate, `--poke 1000000:0xc02c2=1` | leaves the mailbox loop `0x12125B`, runs about 6,800 instructions of the handler, then calls through `I8 = 0` at `0x121866` and slides through zeroed memory (function-pointer table not initialised, the 5.26 situation) |
| FX Core2, `--irq P0I@1000000 --dev 0x31022204=7` | 1 interrupt taken, `DM(0xC03D3) = 7`, **`DM(0xC02C2)` unchanged, loop keeps polling**; `P4I` is not enabled in Core2, `SFT0I` is masked |
| FX Core1, `--irq P4I@300000 --dev 0x3108945c=1` | `DM(0xC02C1) = 1` written by the ISR at pc `0x122FC1` (step 300004), the loop `0x12128B` ends and the pass runs; the loop later clears the word again (`0x1212CD`); without the interrupt it polls forever |
| ORG Core1, `--irq P4I@400000 --dev 0x3108945c=1` (gates `... --flag 1=1 --dev 0x28ac0380=1`) | the ISR at `0x128BBC` writes `DM(0xB00E1) = 1`, the loop at `0x125DF3` runs a pass and clears it at `0x125E36` |
| ORG Core2 (5.22/5.35) | unchanged: mailbox `DM(0xB00E2)` poke, pass returns to `0x125DDF`, `DM(0xC13CE)` still `0x12538C 0x1267` (regression check after this work) |

No section executes `IDLE`: the hypothesis "the FX Core2 idle loop waits for IDLE" is false.

## 4. Provenance summary

| Item | Status | Basis |
|---|---|---|
| IVT base `0x120000`, 12 parcels per vector, ISA parcel order | VERIFIED | six images, reset jump in vector 1; ISA order from the vector words (`0B3E 0000 0000` is `RTI`) |
| Register codes (Table 10-2) incl. `EMUCLK`, `TCOUNT`, `IMASK` | VERIFIED | ADI listing of the firmware words |
| IRPTL/LIRPTL bit to vector mapping, nesting rules, status-stack set, stack depths | VERIFIED (text) | PRM Tables A-12, A-13, B-2, pages 4-12, 4-14, 4-41 |
| Type 19a field layout | VERIFIED | `1600 0000 0028` = `i0 = modify(i0, 0x28)` (ADI) and the existing decoder; `BITREV` bit 39 per the decoder text |
| `IDLE` = parcel `0x0081`/`0x0091` | VERIFIED | ADI prints both as `idle` |
| Timer enable/disable latency (2/1 cycles), period = TPERIOD + 1 | ASSUMED | PRM Figure 5-2 and text, no hardware measurement |
| Interrupt entry stall 4 cycles (+1 from IDLE), 1 cycle per instruction | ASSUMED | PRM 4-30; no cache or stall model |
| `EMUCLK2` = upper half of the clock, `FADDR`/`DADDR` = pc | ASSUMED | name only |
| LADDR termination code `0x0F` for `LCE`, PC stack drop on overflow | ASSUMED | no data |
| `PX` low word for `PX`/`PX1` | ASSUMED | PRM calls PX 48-bit/64-bit |
| An unmasked interrupt with `IRPTEN` clear does not end `IDLE` | ASSUMED | PRM only says "the execution of the ISR releases the processor" |

## Implications

- Host-protocol experiments for the Core1 sections must deliver the command by raising `P4I` with a non-zero `DM(0x3108945C)` (the status the vector reads); for the Core2 sections the mailbox word is poked (or written by the external-port DMA model, milestone M5).
- The start-up of every section needs the board gates listed in section 3 (external-bank words `DM(0x28x00B00)`/`DM(0x28ac0380)`, `0x3108C06C`, `0x3108C00C`, FLAG1 high): these are the inter-chip handshakes, not sequencer features.

## Follow-ups

- Call `note_arith_exception` from the ALU and multiplier lanes (STKY bits and `FIXI/FLTOI/FLTUI/FLTII`).
- Broadcast loads (`BDCST1/9`) belong to the SIMD transfer path; the firmware enables them in every section.
- Resolve what writes the Core2 mailboxes (external-port DMA from the ARM) in the protocol lane.
- Open: verify the `PUSH STS` / `POP STS` / `JUMP (CI)` encodings with `adi_dis.py` before modelling them (no firmware use).

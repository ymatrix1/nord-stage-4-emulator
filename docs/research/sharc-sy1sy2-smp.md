# SHARC images SY1SY2 and SMP (synth and sampler DSPs, OS v1.66)

Checked: 2026-11-12.

Sources: the two images of `bank0_CODE@0x4FD980` (SY1SY2, 121,844 B) and `bank0_CODE@0x51B574` (SMP, 44,596 B) of OS v1.66, exported with `tools/ns4_dsp_images.py` (`docs/research/dsp-image-hunt.md` 5.12, 5.13); ADI `elfdump` of CCES 2.12.1 as disassembler ground truth (`tools/adi_dis.py`, 5.34); ADSP-21489 datasheet (`.reference/sharc/`, memory ranges) and ADSP-214xx hardware reference (IVT location, `.reference/sharc/ADSP-214xx_hwr_rev1.1.pdf`); the execution core `crates/nord-sharc` (`sharc-run`).

Question: what is in the last two DSP images, how are they laid out (they do not start with the loader headers of FX, PNO and ORG), where do their programs start, and what does the SHARC core model still lack to run them?

## 1. Result in one table

| | SY1SY2 | SMP |
|---|---|---|
| Size | 121,844 B | 44,596 B |
| Boot kernel (ISA mode, `0x000..0x5FF`) | identical to SMP up to `0x6B2` | identical to SY1SY2 up to `0x6B2` |
| Blocks after the kernel | 569 (284 zero, 16 code, 269 data) | 1,172 (586 zero, 17 code, 569 data) |
| Code (SRAM block 3, short words) | `0x1C0300..0x1C23F2`, 8,142 short words | `0x1C0300..0x1C1582`, 4,439 short words |
| Data (DM normal words) | `0x92180..0xE7C2A`, 23,908 words loaded, 74,789 zero-filled | `0x92180..0xE3A58`, 4,639 words loaded, 54,975 zero-filled |
| Program entry (reset vector target) | `0x1C0B2D` | `0x1C11E0` |
| Start-up to idle loop | 8,780 steps | 8,620 steps |
| Idle loop | `JUMP (PC, 0)` at `0x1C0B33` | `JUMP (PC, 0)` at `0x1C11FD` |
| Program size | 3,216 instructions | 1,940 instructions |

Per-core table (strict runs, no `--lenient`, `sharc-run`; the numbers are reproducible with the commands in section 6):

| Image | Entry | First unmodelled instruction | Reached by |
|---|---|---|---|
| Boot kernel (both) | ISA `0x8C005` | Type 13a `DO 0xffff81 UNTIL EQ` at `0x8C083` (a `DO ... UNTIL cond`, not `UNTIL LCE`), after 171,814 steps | the strict run itself |
| SY1SY2 program | `0x1C0B2D` | none in start-up; idle loop reached (needs FLAG12 input high, section 4) | the strict run itself |
| SY1SY2 ISR at `0x1C05B9` (IVT vector 17) | `--enter 20000:0x1c05b9` | on the pre-merge core: `F0 = RECIPS F12` (Type 4a) at `0x1C0692`, 114 skipped operations (58 `RECIPS`, 56 `FUNPACK`). With the float lane merged (main) the strict run is clean and returns to the idle loop | interrupt stand-in |
| SY1SY2 ISRs at `0x1C03FD`, `0x1C058C` | | none (return to the idle loop with empty buffers) | interrupt stand-in |
| SMP program | `0x1C11E0` | none in start-up; idle loop reached (needs FLAG12 input high) | the strict run itself |
| SMP ISRs at `0x1C0EE0`, `0x1C03FD`, `0x1C0A71` | | none (return to the idle loop) | interrupt stand-in |
| SMP ISR at `0x1C058C` | | none unmodelled, but the run leaves the code: `IF NE CALL (M8, I8)` takes `I8` from a function-pointer table `DM(I1, M5)` that nothing has initialised yet (state missing, not an instruction gap) | interrupt stand-in |

Two gaps surfaced and were fixed with this research (unit tests): Type 19a (main now has its own `exec_modify_imm`, which this branch uses) and `FLUSH CACHE` (no-op). Remaining for the other lanes after the merge of main: `DO ... UNTIL cond` (Type 13a, boot kernel only) and the SMP function-pointer state. Type 6a (8 instructions in SY1SY2, 7 in SMP) is executed by main and not reached by the probes. A static census of the code blocks (`tools/sharc_visa.py` text) finds `RECIPS` 2, `FUNPACK` 1, `BITEXT` 8, `BITDEP` 9 and `BFFWRP` 7 in SY1SY2, and `BITEXT` 2, `BITDEP` 3, `BFFWRP` 1 in SMP (EMPIRICAL: counts of decoder text, not of executed operations).

## 2. Container format (VERIFIED)

The two images are not loader streams of the kind in 5.17 (no tag `0x5003`, no header checksums; `tools/ns4_dsp_ldr.py` rejects them). Every byte of both is accounted for by the layout below (`tools/ns4_dsp_raw.py`, tests `tools/tests/test_ns4_dsp_raw.py`; the walk reaches the tail of both images exactly).

| Bytes | Content |
|---|---|
| `0x000..0x5FF` | boot kernel: 256 ISA-mode words (fixed 48-bit, six bytes each, least significant parcel first) for the 48-bit address range `0x8C000..0x8C0FF`, five zero words then `CALL 0x8C091 (DB)` in word 5 |
| `0x600..` | block stream, 12-byte headers `{tag, count, address}`, every block padded to 4 bytes: tag 1 zero fill (`count` units, no payload), tag 3 VISA code (`count` 16-bit short words at short-word `address`), tag 4 data (`count` 32-bit words at normal-word DM `address`) |
| last `0x60C` bytes | 12-byte header (not decoded) and a 256-word interrupt vector table (`0x600` bytes, ISA words, least significant parcel first), reset slot = word 5 |

Evidence for the tags: contiguity. A zero block at DM `0x92192` of `0x287` words is followed by a data block at `0x92419 = 0x92192 + 0x287` (SMP); the code blocks tile the short-word range `0x1C0528 + 0xF = 0x1C0537` followed by a zero block of 3 words at `0x1C0537` and the next code block at `0x1C053A`; counts of tag 3 blocks are exactly the short words up to the next header (`0x134` words at `0x600` end at `0x874`, where the first tag 4 header starts). This is the 12-byte (three word) block header of the standard ADI loader for ADSP-214xx (tag 3 = 48-bit/VISA program memory, tag 4 = 32-bit data, tag 1 = zero fill); FX, PNO and ORG use the 16-byte checksummed header of the newer loader (ASSUMED reading of the two formats; the walk above is the proof of the layout).

Flat SRAM image (`python tools/ns4_dsp_raw.py IMAGE --sram OUT`): code at byte `2 * short_word`, data at byte `4 * dm_word` (the same memory, 5.18), the vector table at short-word `0x120000` (byte `0x240000`), the same place as in the other three images (5.20). The kernel is a separate ISA program (section 3).

The six interpolation kernels of 5.15 are one data block of exactly 18,432 words at DM `0xE3400` in SY1SY2 (image offset `0x76A8`). SMP has a 1,536-word block at the same address `0xE3400` (image offset `0x6D68`) holding small values that do not read as floats (denormals as IEEE single; ramp-like integers, not decoded).

Both images share the boot kernel and the six small code blocks `0x1C0528..0x1C0588` (byte-identical payloads, 15 or 12 short words each) and IVT words 52 to 54; the first code block `0x1C0300` (`0x134` short words) is equal only in its first 166 bytes, and `0x1C058B` (408 against 491 short words) differs from its first bytes on. Later code blocks are different programs.

## 3. The boot kernel (ISA mode)

`tools/adi_dis.py --isa IMAGE 0 0x600` disassembles it with ADI's tool (a `visa=False` ELF without the VISA align record). **256 of 256 words decode identically** in `tools/sharc_visa.py` and in ADI's listing after the fixes in section 7 (before: 3 differences, `jump (pc, 0)`, `flush cache`, `idle`).

What it does (from the ADI listing; function names are mine):

| Words (`0x8C000 +`) | Code | Reading |
|---|---|---|
| `0x00..0x04` | five NOPs | vector slots 0 to 4 |
| `0x05` | `CALL 0x8C091 (DB)` | reset |
| `0x06..0x14` | `IMASK = 0`, `IRPTL = 0`, `BIT SET MODE1 0x1000`, `L0..L15 = 0`, `M5 = 0`, `M6 = 1`, `M13 = 0`, `M14 = 1`, `R11 = DM(0x30024)`, `I4 = 0x30024`, `CALL 0x8C031` | register set-up after the PLL returns |
| `0x15..0x7B` | command switch on `DM(0x92003)` (`R8`) with arguments `DM(0x92004)` (count) and `DM(0x92005)` (address): zero fill, `PX` fill, store/load helpers | a host-command loop (ASSUMED) |
| `0x7C..0x90` | `I8 = 0x8C030`, `R9 = 0xB16B0000`, `R11 = BSET R11 BY 9`, `DM(0x30024) = R11`, `I12 = 0x8C004`, `DO (PC, -127) UNTIL EQ`, `PCSTK = 0x8C003`, `BIT CLR IRPTL 0x1000`, `BIT SET IMASK 0x1000`, `BIT SET MODE1 0x1000`, `FLUSH CACHE`, `DM(0x1080) = 0x92000`, `JUMP 0x8C003 (DB)`, `IDLE` | arms the DMA/interrupt-driven receive of the block stream and idles (ASSUMED: `0x30024` is a DMA control register) |
| `0x91..0xB1` | `IMASK = 0`, `USTAT3 = DM(0x2000)`, `BIT CLR/SET` of `PMCTL` (`0x2000`) with `LCNTR = 0x10 / 0x1000` delay loops | PLL multiplier and divider programming (ASSUMED from `DM(0x2000)` and the delay loops; the code itself is VERIFIED) |
| `0xB2..0xDB` | zero loops `DM(I0, 1) = R0` over `0x92180 + 0xBE80`, `0xB2000 + 0xBFFF`, `0xC0000 + 0x7FFF`, `0xE0000 + 0x7FFF`, read-modify-write of `DM(0x1C22)`, `DM(0x1C12)`, `DM(0x1C03)` (IOP registers), `RTS` | clears all internal data RAM above the kernel, configures peripheral registers |
| `0xDC..0xFF` | NOPs | padding |

`sharc-run IMAGE 0x8C005 N --isa` (new) runs it: 171,814 steps (the four zero loops are 163,453 words), then stops at the `DO ... UNTIL EQ` of word `0x83` (inside `0x7C..0x90`) (Type 13a). The kernel executes from the 48-bit address space at `0x8C000` (the ADSP-214xx hardware reference places the IVT there in ISA normal-word space when IIVT is set), so it overwrites the IVT region; the stream then replaces it with the tail table of section 2 (ASSUMED order of events).

## 4. The programs

Start-up (`sharc-run` on the flat image from the reset target, strict): the entry code runs `CALL 0x1C0528`, `CALL 0x1C0CA8` (SY1SY2; `0x1C0528..0x1C0588` are the helper blocks shared by both images) and ends in `JUMP (PC, 0)`. Both programs finish their set-up in under 9,000 steps and then **idle in a jump-to-self**: the work is done in interrupt service routines. Before that, each polls `BIT TST FLAGS 0x1000000` (FLAGS bit 24 = FLAG12 input;  SY1SY2 loop at `0x1C10DD`, SMP at `0x1C1572`) until it is high: an external start signal, the counterpart of the `DM(0xB00E2)` mailbox in the ORG image (5.22, 5.26) but a pin, not a data word. `sharc-run --flag 12=1` drives it (FLGx is FLAGS bit 2x, so bit 24 is FLAG12).

Vector table (VERIFIED by ADI's ISA listing of the tail; word numbers; the unused slots are `jump (pc, 0)` or `rti`):

| Word | SY1SY2 | SMP |
|---|---|---|
| 5 (reset) | `JUMP 0x1C0B2D` | `JUMP 0x1C11E0` |
| 49 | inline code, `RTI` | `JUMP 0x1C0EE0 (DB)` |
| 52 | `JUMP 0x1C03FD (DB)`, then `DM(0x92194) = MODE1; MODE1 = 0x14FC; RTI` | the same |
| 64 | `DM(0x93DF2) = M0; RTI` | `DM(0x9241A) = M0; BIT SET FLAGS 0x100; RTI` |
| 68 | `JUMP 0x1C05B9` | `JUMP 0x1C058C` |
| 80 | `JUMP 0x1C058C (DB)`, `R0 = 1`, `DM(0x1400) = R0` | `JUMP 0x1C0A71 (DB)`, `R0 = 1`, `DM(0x1400) = R0` |

What the two programs are (EMPIRICAL from the instruction mix and the data, ASSUMED for the product role):

- **SY1SY2 (synth engine).** The only image with the 18,432-float polyphase kernel set (six 12-tap, 256-phase band-limited interpolators, 5.15) in its data; its interrupt routine at `0x1C05B9` is a loop of `RECIPS` (reciprocal, division) and `FUNPACK` (float unpack) per record, the shape of an oscillator or sample-rate step; 593 of its 3,216 instructions are shift-immediates with bit extract/deposit (`FEXT`, `FDEP`, `BITEXT`, `BITDEP`), the packing of parameter words. The name `SY1SY2` ties it to the Synth section's two sound chips (5.12, ASSUMED).
- **SMP (sample engine).** No float tables; 4,639 data words and a 220 KB zero-filled work area; 62 shift-immediates. Its dispatcher at `0x1C058C` (also used by SY1SY2 vector 20) walks records with a counted `DO` loop, extracts fields with `FEXT` and calls handlers through a pointer table (`IF NE CALL (M8, I8)` with `I8 = DM(I1, M5)`), the same delayed-call convention as the ORG processing pass (5.25). The sample data itself (`CNSP` `.nsmp4` payload) is not in the image; the engine reads it from external memory (ASSUMED from the missing sample tables and the `0x20xxxxxx` windows of 5.17).

## 5. Where the code blocks differ

`adi_compare.py` on the code blocks (SMP `0x8660..0xAC20`, SY1SY2 `0x19748..0x1D9F0`) agrees with ADI on all code types after the fixes of section 7. The remaining differences are in non-code bytes between blocks (the 12-byte headers), the `FPACK`/`FUNPACK` operand naming (float lane), shifter operand names inside multifunction words (`BFFWRP`, `BITEXT` with a move; shifter lane), and the SF/BM naming of condition code 14 (ADI prints `sf` for some Type 6a/3a words and `bm` for jumps; the decoder prints `BM`; left open, no evidence on which instruction families use which name).

## 6. Commands

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_raw.py DIR/dsp_image_3_SY1SY2.bin --sram sy.bin --other DIR/dsp_image_4_SMP.bin
python tools/ns4_dsp_raw.py DIR/dsp_image_4_SMP.bin --sram smp.bin --kernel kernel.bin
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/adi_dis.py DIR/dsp_image_3_SY1SY2.bin 0 0x600 --isa              # the boot kernel
python tools/adi_compare.py DIR/dsp_image_4_SMP.bin 0x8660 0xac20             # code blocks
sharc-run DIR/dsp_image_3_SY1SY2.bin 0x8c005 300000 --isa                     # kernel, strict
sharc-run sy.bin 0x1c0b2d 100000 --flag 12=1 --trace 6                   # SY1SY2 start-up
sharc-run smp.bin 0x1c11e0 100000 --flag 12=1 --trace 6                  # SMP start-up
sharc-run sy.bin 0x1c0b2d 400000 --flag 12=1 --enter 20000:0x1c05b9 --lenient --skips
```

## 7. Decoder and core changes made with this research (each with a test)

| Item | ADI evidence | Where |
|---|---|---|
| Type 6a text `Rn = <shift> Rx BY imm, R = DM/PM(I, M)` | write bit 31, PM bit 32, data register bits 26..23, post-modify only; 10 samples incl. `827e 0280 0188` | `visa/decode.rs`, `tools/sharc_visa.py` |
| `JUMP`/`CALL` flags `(DB)` bit 26, `(LA)` bit 38 (jumps), joined as `(DB,LA)`; relative form printed `(PC, 0)` / `(PC, 0xffffffe0)` | `jump 0x1c0b8b (la)`, `jump 0x1c11e3 (db,la)`, `jump (pc, 0)` | same |
| `RTS`/`RTI` carry a parallel compute (Type 11a) | `if gt rts, r2=MRF-r3*r14(uui)` | same |
| `IDLE` (`0080 0000 0000`, 16-bit `00ff`), `FLUSH CACHE` (`1702 0000 0000`) | ADI text | same |
| System register name `LIRPTL` (was `LRPTL`) | `bit set lirptl 0x2400` | `visa/regs.rs`, `tools/sharc_visa.py` |
| ISA classification (`classify_isa`: no 16/32-bit forms) | kernel listing, 256/256 identical | `visa/classify.rs` |
| ISA-mode execution (`Core::load_isa`, `sharc-run --isa`) | kernel runs 171,814 steps | `core/isa.rs` |
| `FLUSH CACHE` as a no-op; Type 19a test cases (execution is main's) | first faults of SY1SY2/SMP ISRs | `core/exec.rs`, `core/moves.rs` tests |
| `Core::enter_handler`, `sharc-run --enter STEP:PC` (direct handler entry; FLAG pins and `--irq` come from main's sequencer) | idle-loop polls `BIT TST FLAGS 0x1000000` | `core/branch.rs`, `bin/sharc_run.rs` |
| `adi_dis.py` ISA mode (`--isa`, `build_elf(visa=False)`) | | `tools/adi_dis.py` |
| Raw image parser and SRAM writer | | `tools/ns4_dsp_raw.py` |

## Implications

- The synth and sample DSPs run small programs (16 KB and 9 KB of code) that idle in a jump-to-self and work in interrupt routines. The oracle therefore needs interrupt dispatch (IRPTL/IMASK/vector priorities) to run any real work; `sharc-run --enter STEP:PC` is a stand-in that enters a handler body directly.
- The start trigger of both is a FLAG12 input, not a mailbox word, so the ARM-to-DSP message path (the three `DM(0x92003..5)` words of the kernel, then the record dispatcher at `0x1C058C`) is where parameters will enter, as in the ORG image.
- For milestone M3 the two images now run to their idle loops (`docs/specs/sharc-core.md`); the remaining gap seen is `DO UNTIL cond` in the boot kernel.

## Follow-ups

- Run SY1SY2 vector 17 with real inputs: find what buffer it reads (its `I7` frame, offsets `-32` and `-26`) and compare an oscillator step against the 18,432-float kernels.
- Initialise the SMP function-pointer table `DM(I1, M5)` the way the program does (find the writer; it is not in start-up) to run the dispatcher.
- Identify the 12-byte tail header and the first block `{3, 0x134, 0x1C0300}` (the code at `0x1C0300..0x1C0434` is the shared start-up helper code).
- Decide the SF/BM condition-code naming against ADI output on a real conditional compute.
- Confirm on hardware or in the driver which ADI loader format (12-byte or 16-byte headers) the ARM uses per image (`bank0_CODE` DSP download routine, 5.12 follow-up).

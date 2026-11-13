# SY1SY2 and SMP: real voice start and audio through the oracle

- **Checked:** 2026-11-12
- **Sources (local, never committed):** the SY1SY2 and SMP images of OS v1.66 (`tools/ns4_dsp_images.py`, flat SRAM from `tools/ns4_dsp_raw.py --sram`); ADI `elfdump` listings (`tools/adi_dis.py`, CCES 2.12.1); `crates/nord-sharc` (`sharc-ref`, extended here); `target/fw_unpacked/bank0_CODE.bin` (ARM literals, `tools/arm_xref.py`, capstone); the local factory backup `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (`ns4 backup --extract`, `nord-core` sample decoder); [sy-dsp-map](sy-dsp-map.md), [sharc-sy1sy2-smp](sharc-sy1sy2-smp.md), [oracle-vs-engine](oracle-vs-engine.md), [arm-param-messages](arm-param-messages.md), [pno-dsp-map](pno-dsp-map.md) section 7.
- **Question:** Can the SY1SY2 and SMP images be driven the way the ARM drives them (command FIFO, audio interrupt, Stage A state machine) so that the oracle produces real audio, and does that audio equal the Rust engine's resampled signal?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = measured or read from code shape, **ASSUMED** = reading of the purpose.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| SY voice start through the real path | FIFO reset interrupt (vector 13), message `rec+0x14 <- 0x1C07CD` ("prepare"), audio interrupt (vector 17), message `rec+0x14 <- 0x1C084C` ("go"), then one audio interrupt per output frame. Stage A installs the resampler `0x1C10EE` into Stage B itself; gain and step are computed by the firmware (ISR steps 3 and 4) | VERIFIED (`sy_voice_*.scn`) |
| Audio against the engine, sine, 240 frames, kernel 0, ratio 0.625 | `SincKernelSet` + the DSP's 40-bit rules: max abs error **2.98e-8**, SNR 150.5 dB, 186 of 240 frames bit-exact. Plain f32 accumulation: 5.96e-8, 143.2 dB. With the oracle's own kernel table and the 40-bit rules: **0 error, 240 of 240 bit-exact** (both channels) | VERIFIED |
| Audio against the engine, real factory sample (128 frames of "Weltmeister 4 Accordion_PS 4.1"), ratio 0.8749 | engine + 40-bit: max 5.96e-8, 144.9 dB, 136 of 240 bit-exact; engine + f32: 1.19e-7, 139.0 dB; oracle table + 40-bit: **0 error, 240 of 240 bit-exact** | VERIFIED |
| Oracle bug found and fixed | long-word (`(LW)`) accesses were executed as 32-bit: the frame clear left the right channel uncleared (every ISR added to it), and the 64-bit voice masks lost their high word. Now modelled for Types 14a, 15a, 15b, 3a, 3b | VERIFIED (ADI listing, 6 unit tests, 58 `(LW)` sites in the two images) |
| SMP | Audio interrupt, FIFO and status poll run under the same harness (SMP FIFO base `0x92196`); the voice loop `0x1C07D8` plays real CNSP piano pages bit-exactly against the Rust model, see [smp-voice](smp-voice.md) | VERIFIED |

What the harness does not model: the external-port DMA that refills the 128-frame source ring from SDRAM and the unpacker family that decodes it (section 5), and the ARM-side initialisation script (section 5). The ring is pre-filled and the refill bookkeeping word is held above its threshold.

## 2. Harness changes (`sharc-ref`, `sinc_ref`, new example)

| Piece | Change |
|---|---|
| `irq VECTOR [STEPS]` | raises the interrupt through the core's interrupt controller (real IVT slot, `MODE1` register-bank swap, `RTI`) and runs until it returns. Vector 13 (IVT word 52, handler `0x1C03FD`) is unmasked for the call and also resets the harness FIFO write pointer: the firmware's own reset sets `I0 = 0x93046` (SY) or `0x92196` (SMP) |
| `msg [op=N] ADDR VALUE` | appends `[opcode:4 \| address:28], value` to the FIFO ring and moves the DMA write index `DM(0x2400)` (the ISR reads it at `0x1C0638`); `fifo BASE` selects the ring of the image |
| `dev ADDR VALUE` | `Core::preset_device` (device space; IOP registers such as `0x5201` are plain memory and use `poke`) |
| `fill ... raw path=FILE chan= chans= skip=` | float32 test input from a local file (`cargo run -p nord-core --example sample_dump` writes it from an `.nsmp4`); never committed |
| `sinc_ref` | reads optional per-frame series `gainL`, `gainR`, `step` of the dump (the firmware slews them) instead of one fixed gain and ratio |
| `core/longword.rs` | `(LW)` long-word moves (section 6) |

An unfinished `irq` reports the last 24 distinct program counters; that is how the refill hang of section 5 was located.

## 3. SY command and voice-start sequence (VERIFIED unless marked)

**Command FIFO.** The audio ISR (`0x1C05B9`, IVT word 68 = vector 17) reads `n = min((DM(0x2400) + R3 - I0) >> 1, DM(0x92180))` pairs. A pair is `word0 = [opcode:4 | address:28]`, `word1 = value`; opcode 0 stores `value` at `DM(address)`, opcodes 1 to 15 call the handler `DM(0x92182 + opcode)`. The read pointer must be re-armed by the vector-13 handler before a batch (EMPIRICAL: without it a second batch is not consumed; whether the hardware raises vector 13 per DMA completion is ASSUMED).

**Opcode handlers** (the table was read back from the image: `1C0300 1C031F 1C0355 1C0359 1C035D 1C030F 1C0304 1C0381 1C039E 1C03AA 1C03DD 1C03F8 .. 1C03FC`; the descriptions are EMPIRICAL code shape, the voice start needs opcode 0 only):

| Opcode | Entry (`DM(0x92182 + opcode)`) | Effect |
|---|---|---|
| (0) | `0x1C0300` | slot 0 is never called (opcode 0 stores); `RTS` with `DM(I7) = R7` in the delay slots |
| 1 | `0x1C031F` | writes `DM(0x2400)`, `DM(0x2420)` and the IOP bits `0x24B0`, `0x24B2`, `0x2481`; sets `IMASK` bit 13 (ASSUMED: arms the external-port DMA that fills the FIFO) |
| 2, 3 | `0x1C0355`, `0x1C0359` | `R14 = R7`, `R15 = R7` |
| 4 | `0x1C035D` | programs `DM(0x1820..0x1825)` and `DM(0x180B)` (a DMA channel) |
| 5 | `0x1C030F` | `FLAGS` pins `= value AND 0x15` |
| 6 | `0x1C0304` | `I6 = address + 0x4000000; DM(I6) = value` (store to a high address) |
| 7..10 | `0x1C0381`, `0x1C039E`, `0x1C03AA`, `0x1C03DD` | DMA and interrupt-mask register sequences (`DM(0x1805)`, `DM(0x1000 .. 0x1084)`) |
| 11..15 | `0x1C03F8 .. 0x1C03FC` | `RTS` |

**Stage A state machine** (record word `+0x14`, `I3 = record + 0x26`; word offset `q` from `I3` is record word `0x26 + q`). The four states below are the only values the ARM image holds as literals in this range (`0x1C07B0` 56 times in the record initialiser at `0xC04FECFC`, `0x1C0794`, `0x1C07CD`, `0x1C084C`; static scan of `bank0_CODE.bin`):

| Entry | Role | What it does |
|---|---|---|
| `0x1C0794` | voice reset (ARM: `0xC01D66C4`, `0xC01D6730`) | clears the slewed fields of the record and its pair block, writes `rec+0x39 = 0.046` (`0x3D3CBACC`), then falls into the idle state |
| `0x1C07B0` | idle (initial state of all 56 voices) | clears `+0x17`, `+0x1A`, sets `+0x19 = 1`, Stage B pointer `= 0x1C10ED` (`RTS`), ORs the voice bit (`+0x06`) into the 64-bit mask at `DM(0x92196)` |
| `0x1C07CD` | **prepare** ("note on", ARM `0xC01E1198`, `0xC01E193C`) | `+0x17 = 4`; `+0x1C = +0x2F + +0x13` (stream read pointer); `+0x1A = -(+0x13)`; `+0x26 = +0x31` (ring base); `+0x10 = 0`; clears the last 10 words (5 frames) of the ring (write the ring after this state); **`+0x2B = 0xFB000000` (frame index 125, phase 128)**; `+0x22 = +0x23 = 0`; Stage A `= 0x1C0811`, Stage B `= 0x1C10ED` |
| `0x1C0811` | waiting | Stage B `= RTS`; if `+0x10 + +0x1A > 0x52` the next state is the release state `0x1C083C` |
| `0x1C083C` | release | Stage B `= RTS`, sets the voice bit in the mask `DM(0x9219A)` |
| `0x1C084C` | **go** (ARM `0xC01E2C2C`, `0xC01E2F54`) | Stage A `= 0x1C0851`, then falls into `0x1C0851` |
| `0x1C0851` | playing | `RTS (DB)` whose delay slots install **Stage B `= 0x1C10EE`** on every call; the state never changes by itself |

The ARM sends each state as one path-1 message `(base + 0xE, pointer)` with `base = 0x92406 + 0x38 k` (sender `0xC01E00AC`; word `0xE` of the base-`0x92406` table is record word `0x14`). The ARM writes more fields before the prepare message (`+0x0A, +0x0C .. +0x0F, +0x13`, ...); their roles are open, the scenario relies on the reset values and the fields the prepare state reads (`+0x2F`, `+0x13`, `+0x31`).

**One ISR is one output frame** (VERIFIED, `sy_voice_sine.scn`): ISR order is counter and re-entrancy guards, three bookkeeping calls (`0x1C0B36`, `0x1C0B5A`, `0x1C11AC`), FIFO, DMA re-arm, 28 pair iterations of rate and gain slewing (step 3), 56 smoothing iterations (step 4, writes the `step` word `rec+0x2D` and the gain words), Stage A of all 56 voices, the two bookkeeping helpers, the frame clear, Stage B of all 56 voices, `RTI`. Stage B writes one stereo frame per call; there is no 120-frame loop in the interrupt. The block of 120 frames in earlier notes is the size of the DMA buffer (`0x78` pair stores in the clear loop), not of one interrupt.

## 4. Mixing, accumulation order and gain staging (SY, VERIFIED on the oracle)

| Stage | Behaviour |
|---|---|
| Frame buffer | `DM(0x93590)` points at the frame (`0x932CA`; the DMA-done handler `0x1C058C` flips it to the second half at `0x934AA + 0xF0`). Word `+0` is left, `+1` right; words `+0x28`, `+0x29` receive the second FIR pass as shifted bit patterns (not audio, EMPIRICAL) |
| Clear | `R0 = R1 = 0; DM(I5, M2) = R0 (LW)` 120 times with `M2 = 2`: with the long-word store both channels are cleared. Without LW support the right channel accumulated (the oracle bug of section 6) |
| Accumulation | voice 0 first, then ascending to 55 (the Stage B loop order). Each voice does a read-modify-write: `frame[M4] = frame[M4] + gain * sum(12 taps)`, `M4 = rec+0x33` (0 here). So the bus is a float32 sum in voice order; rounding is the 40-bit rule of [oracle-vs-engine](oracle-vs-engine.md) |
| Voice gain | record words `+0x08` (left) and `+0x09` (right), slewed by the firmware from the pair block (`0x9219E + 0x16 k`); the kernel DC gain is `1/fc` and is not normalised, so the output level is `gain / fc` relative to the source. In the scenarios the gain settles to exactly 1.0 after the first frame (`0x3F7FFFFF`, then `0x3F800000`, EMPIRICAL) |
| Position | `pos` word `+0x2B` (frame index bits 31..25, phase bits 24..17), advanced by `2 * step` per call; `step = +0x2D` is rewritten every ISR from the pair block |
| Pitch | the step comes from a 16-bit packed float in the table at `DM(0x9354A + index)` (index = low half of pair-block word 0, `0x36` for voice 0, so `DM(0x93580)`): `ratio = 0.625 * 2^((e >> 11) - 1) * (1 + (e & 0x7FF) / 2048)`, six points measured: `e = 0x0800, 0x0A00, 0x0B33, 0x0C00, 0x1000, 0x1800` give `step = 0xA00000, 0xC80000, 0xDFFC00, 0xF00000, 0x1400000, 0x2800000` (ratio 0.625, 0.78125, 0.8749, 0.9375, 1.25, 2.5). The table is empty at start-up (rate 0); the ARM fills it. The upper clip is ratio 8 (`0x08000000`) |

## 5. What the harness replaces, and why

| Hardware path | Harness | Evidence |
|---|---|---|
| Ring refill. Every 4th interrupt the scheduler `0x1C11AC` scans 14 voice records (`DM(0x95C69)` is the scan pointer, `B3 = 0x92406`, `L3 = 0xC40`) and queues those with `+0x10 - R1 <= 0x4B` and `+0x1A >= 0x2B`; the queue is unpacked by `0x1C1228` (header word: bit 23, 4-bit count at bit 19, bit 18, **4-bit width at bits 17..14**, 14-bit length at bits 13..0) through a width-indexed pointer table at `DM(0x95C7C)` that the ARM writes | The ring is filled before the first frame and `rec+0x10` is poked to `0x200` each frame so the scheduler never queues the voice. Untreated, the fifth interrupt jumps through an empty pointer-table entry into the trap `JUMP 0x1C12DD` (a jump to itself) | EMPIRICAL (the hang is reproduced; the scheduler conditions are read from the listing) |
| ARM initialisation script | The ARM image holds it as `(type, ...)` records at `0xC0503600 ..` (type 4 `[4][n][address][n values]`, type 1 `[1][value][address]`; for example `[4][3][0x93E1E][-1 -1 1]`, `[4][3][0x95C69][0x92426 0 0x92426]`, `[4][0x23][0x95C7B][0x95C6D, 32 unpacker entries ...]`). Not replayed; a full replay would also supply the unpacker table and the pitch table | ASSUMED structure (read from the data, no ARM function traced) |
| External-port DMA | `msg` writes the FIFO and its index directly | |

The ring is 128 stereo frames (left at even words, right at odd words from `0xC0000`), so a 240-frame run wraps the ring more than once. The reference renderer wraps the same way, so the comparison is exact; for the real-sample scenario the discontinuity at the wrap is part of both signals.

## 6. Oracle fix: long-word accesses

ADI marks 34 instructions of the SY1SY2 image and 24 of the SMP image `(LW)`: `5abe c000 0000` is `dm(i5,m2)=r0 (LW)`, `1180 0009 2196` is `dm(0x92196)=r0 (LW)`, `9688 0160` is `r2=dm(0xffffffe0,i3) (LW)`, `1092 0009 4036` is `i2=dm(0x94036) (LW)`. A long-word access moves register `Rn` with the word at `addr` and register `Rn+1` with `addr+1` (the firmware zeroes `R0` and `R1` before the store, and uses `R8`, `R9` / `R10`, `R11` as the low and high half of the 64-bit masks). Bit positions (from pairs with and without the flag in the listing): bit 39 in Types 14a, 15a, 15b; bit 30 in Types 3a, 3b. Types 4a and 4b are not modelled (no sample in these images). In SIMD mode the existing PEy transfer already covers the second word, so the flag adds nothing there (ASSUMED; the SMP voice loop has five such accesses). Before the fix the core treated them as 32-bit, which is why the right channel of the frame accumulated.

Re-checked after the change: the FX, ORG and PNO start-ups and the SMP start-up still run to their loops (no fault), `cargo test -p nord-sharc` passes.

## 7. SMP: what was established

| Item | Result | Tag |
|---|---|---|
| Interrupts | IVT word 52 (vector 13) = FIFO reset, same code as SY; word 68 (vector 17) = audio ISR `0x1C058C`; word 80 (vector 20) = DMA done `0x1C0A71` | VERIFIED |
| FIFO | ring base `0x92196` (SY: `0x93046`); same pair format and handler table `DM(0x92182..)` (same 16 entries); maximum pairs per interrupt `DM(0x92419) = 16`; `msg 0xA0000 0x1234` stores through it (`tools/oracle_scenarios/smp_dispatch.scn`) | VERIFIED |
| Status poll | the ISR waits for bit `0x40` of `DM(0x5201)` (an IOP register); the harness sets it with `poke 0x5201 0x40` | VERIFIED |
| Voice lists | voice counts `DM(0x9268B)` (compressed voices, loop at `0x1C07D8`) and `DM(0x9268A)` (second loop, listing `0x3810C8`), list pointer `DM(0x9268C)` (initially `0x92534`); history ring `B5` with `L5 = 0x10`, packed stream ring `B1` with `L1 = 0x100`, kernel base `R14 = 0xE3400`, `M11 = 0x100` | VERIFIED (listing) |
| Record fields read by the loop (`I3` signed offsets) | `-30` and `-28` and `-27` (position arithmetic: `R0 = rec[-28] * rec[-30] (UUI) - rec[-27]`, low 4 bits = bit offset, next 8 bits = word index), `-26` stream ring base (`B1`), `-25` history ring base (`B5`), `-24` and `-29`/`-31` (64-bit phase accumulator, kernel phase = top 8 bits of `-29`), `-19` residual width (`<< 6` is the `FEXT` length), `-17` predictor coefficient table (`I9`, four taps), `-32`, `+0`, `+1` (left and right gain), `+4` (last output), pointer at `-18` (mix bus `I7`; first read as `+0x36`, corrected in [smp-voice](smp-voice.md)) | VERIFIED in [smp-voice](smp-voice.md) |
| Loop executes | one voice with an all-zero record runs the whole loop without a fault and adds 0 to the bus | VERIFIED (`smp_dispatch.scn`: `poke 0x9268B 1`, list entry `0x9B000`) |

Done in [smp-voice](smp-voice.md): the record fields, a real packed CNSP page driven through the loop, and a bit-exact comparison with the Rust model (the bus pointer is record word `-18`, not `+0x36`; that note above was a misreading of an earlier `M6`).

## 8. Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_raw.py DIR/dsp_image_3_SY1SY2.bin --sram sy.bin
python tools/ns4_dsp_raw.py DIR/dsp_image_4_SMP.bin --sram smp.bin
cargo build --release -p nord-sharc -p nord-core --example sinc_ref --example sample_dump
mkdir -p target/oracle
# kernel table of the firmware (for the bit-exact comparison)
target/release/sharc-ref sy.bin tools/oracle_scenarios/sy_stage_b_kernel.scn --out target/oracle/kernel
# sine voice
target/release/sharc-ref sy.bin tools/oracle_scenarios/sy_voice_sine.scn --out target/oracle/sv --wav
target/release/examples/sinc_ref target/oracle/sv.csv --accum ext40 > target/oracle/sv.ref.csv
python tools/ns4_oracle_compare.py target/oracle/sv.csv target/oracle/sv.ref.csv
target/release/examples/sinc_ref target/oracle/sv.csv --accum ext40 --kernel-from target/oracle/kernel.csv > target/oracle/sv.ref.csv
python tools/ns4_oracle_compare.py target/oracle/sv.csv target/oracle/sv.ref.csv --tol-abs 0
# real factory sample (local files only)
target/release/ns4 backup "Nord Stage 4 Factory Restore RevK v1.66.ns4b" --extract "Samp Lib/Accordion_Harm/Weltmeister 4 Accordion_PS 4.1.nsmp4" target/oracle/accordion.nsmp4
target/release/examples/sample_dump target/oracle/accordion.nsmp4 target/oracle/accordion.f32 --start 1000 --frames 128
target/release/sharc-ref sy.bin tools/oracle_scenarios/sy_voice_sample.scn --out target/oracle/vs --wav
# SMP dispatch check
target/release/sharc-ref smp.bin tools/oracle_scenarios/smp_dispatch.scn --out target/oracle/smp
# regression test (no firmware needed): the 48-frame hash case embedded in the test
cargo test -p nord-core --test oracle_sy_voice
```

## Implications

- The synth sample path of the engine is now checked against the firmware's own interrupt, not only its Stage B routine: with the oracle's table and the 40-bit rules the engine rendering is reproduced bit for bit (240 of 240 frames, two signals); with the engine's table the difference is one to two float32 ulp.
- `ratio` from the pitch table is `0.625 * 2^(e>>11 - 1) * (1 + m/2048)` for the 16-bit packed float `e`; the engine's pitch-to-step map should use the same grid (open: which `e` the ARM writes for a MIDI note; the table is ARM-initialised).
- Any other module that stores through `(LW)` was wrong in the oracle before this change; the FX, ORG and PNO images still reach their loops, but their numerical results were not re-run.

## Follow-ups

1. Decode the ARM initialisation script (`0xC0503600 ..`) and replay it: this supplies the unpacker pointer table `DM(0x95C7C)` and the pitch table, so the ring can be refilled by the firmware's own unpackers from a packed stream in DM and the `rec+0x10` poke can go.
2. ~~SMP: drive one compressed voice~~ Done: [smp-voice](smp-voice.md).
3. Find which `e` the ARM writes for each MIDI note and the kernel selector (`rec+0x3B`).
4. Check LW in SIMD mode on the ADI simulator (method of [sharc-float](sharc-float.md)).

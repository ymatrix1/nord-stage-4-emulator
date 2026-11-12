# PNO DSP image map (piano core 1, entry `0x121007`)

- **Checked:** 2026-11-12
- **Sources (local, never committed):** the PNO image of OS v1.66 (`bank0_CODE@0x46EA8C`, 0x46824 B) written by `tools/ns4_dsp_ldr.py --sram` (`core1_sram.bin`); ADI `elfdump` listing of its code (`tools/adi_dis.py 0x240600 0x24C700`, CCES 2.12.1, method of [dsp-image-hunt](dsp-image-hunt.md) 5.34); `sharc-run` (strict, `--irq`, `--dev`, `--trace`); [sharc-sequencer](sharc-sequencer.md), [sharc-sy1sy2-smp](sharc-sy1sy2-smp.md), [arm-dsp-protocol](arm-dsp-protocol.md), [nsmp4-and-npno-cnsp-codec](nsmp4-and-npno-cnsp-codec.md); `tools/sharc_routines.py` (new) for the routine census.
- **Question:** What does the PNO core 1 program compute (per-voice sample interpolation, resonators, hammer or string models, sympathetic resonance, EQ), where are its entry and exit points, mailboxes, per-module record layout and buffers, and what does this say about `crates/nord-core/src/piano.rs`?

Tags: **VERIFIED** = read in the ADI listing or reproduced by a command below; **EMPIRICAL** = textual shape of the code (counts, patterns), not a behavioural test; **ASSUMED** = reading of what the code is for.

## 1. Verdict

| Question | Answer | Tag |
|---|---|---|
| Does PNO core 1 play back piano samples (decode, interpolate per voice)? | **No.** It has no bit-field unpacker family and no 256-stride kernel loop. Those are in the **SMP** image (section 7): fixed-LPC residual decode, 6-tap Q31 polyphase interpolation, stereo gain mix. | VERIFIED (absence in this code, presence in SMP) |
| What is it then? | A stereo block processor on 120-frame blocks: ping-pong buffers, three random sample-and-hold generators, a chain of 18 to 22 insert modules with ramped, clipped gains (every slot empty in the image: all routine pointers are the pass-through stub `0x12637B`), banks of transposed direct form II biquads (Butterworth fs/4 coefficients in the image), delay-line readers with fractional-position interpolation, 65-point table interpolators. | VERIFIED structure, ASSUMED role |
| Hammer or string physical model? | Not identifiable from the image alone. The routines that look like a recirculating delay line with a loop gain (`0.9908`, `0x3F7DA3C2`) and a damping filter exist (section 5), but no routine is reached by the start-up or by one triggered pass; the module routines are installed at run time. | ASSUMED |
| Sympathetic resonance, EQ | No evidence that separates them from the biquad and delay routines. | open |
| Entry, exit | Reset vector `JUMP 0x121007`; one pass per `P4I` interrupt (mailbox `DM(0xB00E1)`); no exit (the program never returns). | VERIFIED |

Consequence for `piano.rs`: the sample voice (decode, interpolate, pan) is the SMP engine and is reproduced by the CNSP codec plus a 6-tap interpolator, not by this image. What this image adds on top of the sample voices is block-level stereo processing; its exact DSP needs the run-time module installation (section 6) before any 1:1 claim.

## 2. Image, memory and vectors

The code is `0x120300..0x12637C` (24,701 short words; 10,602 instructions decoded by ADI, the rest is data). Core 2 of the same image is the organ program (`0x125C88`, [dsp-image-hunt](dsp-image-hunt.md) 5.17), not covered here.

| Region (normal-word DM address) | Content |
|---|---|
| `0x90180..0x931BE` | 12,351 words: coefficient and table area (PM reads use `0x95000`, `0x95100`, ...) |
| `0x95000..0x95009` | two biquad coefficient sets `b0 b1 b2 a1 a2` = `0.292893 0.585786 0.292893 0 0.171573` (section 4) |
| `0x95100`, `0x95102`, ... | pointer pairs for the two 2-iteration crossfade loops (`0xB2EEE, 0xB2EFD, 0xB2F0C, ...`, 15-word stride) |
| `0xB00E0..0xB015E` | mailboxes and ping-pong bookkeeping (section 3) |
| `0xB0000/0xB0078`, `0xC0000/0xC0078` | the two buffer sets of one pass, `0x78` = 120 words each (the start-up clears `0xB0078` and `0xC0078` for `0x64` words and `0xB0000`, `0xC0000` for `0x78`) |
| `0xB2873..0xB33DD`, `0xB4452..0xB4CCF` | module state and the 65-point tables of section 5 (`0xB4E97`, stride `0x41`) |
| `0xC01D6..0xC01F5` | call table of 8 slots (section 6) |
| `0xC2DE0..0xC2E00` | timing statistics (`EMUCLK` differences, min and max per phase) |
| `0xE0000..0xE122B` | 4,652 words of float tables, among them the 512-entry pair `-1 .. -0.0031`, `0 .. 0.3125` already listed in [dsp-image-hunt](dsp-image-hunt.md) 5.14 (role not identified) |
| `0x20004000..` | external window: 655,360 B zero-fill (work RAM, ASSUMED) and 8 descriptor addresses (`0x20004000`, `+0x28`, `+0x50`, `+0xA0`, `+0xC8`, `0x20004100`, `0x20004128`, `0x20004130`) loaded into `USTAT1` before DMA set-up (ASSUMED: DMA control blocks) |

Vectors (`0x120000`, ISA parcel order, VERIFIED by `adi_dis.py --isa`):

| Slot | Content | Meaning |
|---|---|---|
| RSTI (`0x12000F`) | `JUMP 0x121007` | entry |
| P0I (`0x120084`) | `BIT CLR MODE2 0x20; JUMP 0x1233B7` | stops the timer; ISR `0x1233B7`: `DM(0xB0158) = DM(0x31022084)`, `RTI` |
| P4I (`0x1200B4`) | `DM(0xB00E3) = R0; R0 = DM(0x3108945C); DM(0x3108945C) = R0; JUMP 0x1233A4` | status read and acknowledged; ISR `0x1233A4`: `DM(0xB00E1) = R0; DM(0x31022330) = 1; DM(0x3108900C) = DM(0xB00E1); R0 = DM(0xB00E3); RTI` |

## 3. Start-up, mailbox, pass (VERIFIED by listing and run)

Start-up (`0x121007`): `BIT CLR MODE1 0x1000`, calls `0x12034C` and `0x1203C4` (circular-buffer register set-up), clears the four buffer areas above, calls `0x122EF6` (470 instructions, state initialisation, `DM` only), `0x1233D2`, `0x1234B0`, `0x123505` (waits for `DM(0x3108C06C)` bit `0x400`, then toggles bit 2 of `0x3108C00C`/`0x3108C008`), polls the gate `DM(0x28AC0380)` until non-zero, raises FLAG0 and waits for FLAG1, sets `DM(0xB00E0) = 1`, then enables `P0I` and `P4I`.

Idle loop (`0x121091`): `R0 = DM(0xB00E1); PASS; IF EQ JUMP`. The pass is released by `P4I` with a non-zero status, which the vector copies to `DM(0xB00E1)`.

Experiment (VERIFIED, v1.66, strict):

```bash
python tools/ns4_dsp_ldr.py DIR/dsp_image_1_PNO.bin --sram SRAM
sharc-run SRAM/core1_sram.bin 0x121007 3000000 --dev 0x28ac0380=1 --dev 0x3108d008=5 \
  --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 --flag 1=1 --irq P4I@2000000 --dev 0x3108945c=1 --dump 0xb00e0:8
# DM(0xb00e0..): 1 0 0 0 b0000 b0078 121e90 121e90   (pass ran; mailbox cleared)
```

The pass runs `0x121099..0x121464`, enters the stub `0x12637B` through the empty module table and then falls into zeroed memory (`0x121467` on the call stack, fault at `0x120000`). So the pass reaches the first indirect call with the image's initial state and stops there, like the ORG pass of [dsp-image-hunt](dsp-image-hunt.md) 5.26 before its record pointers are right.

Pass structure (listing `0x121099..0x121469`, VERIFIED reading):

| Step | Code | What |
|---|---|---|
| 1 | `0x121099` | bookkeeping: `EMUCLK` differences into `DM(0xC2DE4..0xC2DEE)`; clear `DM(0xB00E1)` |
| 2 | `0x1210D9` | **ping-pong select**: `R0 = DM(0xB0158)` compared with `0x28AC01E0`; `IF NE` use `0xC0000/0xC0078` instead of `0xB0000/0xB0078`; the choice goes to `DM(0xB00E4)` (input base) and `DM(0xB00E5)`; six derived pointers `DM(0xB0159..0xB015E)` = base `+ 0xA200000 + {0x50, 0x12, 0x18, 0xC, 0x40, 0xC}`. `DM(0xB0158)` is written by the `P0I` ISR from the device register `0x31022084` (a DMA or serial-port buffer indicator, ASSUMED) |
| 3 | `0x121120` | `TCOUNT = 0x2400`, timer on, `BIT CLR MODE1 0x202000` |
| 4 | `0x12112C` | first-pass-only init (`DM(0xC00DC)` counts down from 8): sets `DM(0x310C3000)`, builds two 5-pointer records at `0xE31A6` and `0xE359E` |
| 5 | `0x12116C` | per-pass register load of those records (`M3`, `B7`, `B5`, `L7`, `L5`, `I7`, `I5`), `PEYEN`/broadcast on (`BIT SET MODE1 0xE00000`) |
| 6 | `0x1211B4` | one MAC-based stereo smoothing (`MRF = MRF + R1*R2`) on `DM(0xB0162)` |
| 7 | `0x1211ED` | three-term accumulator of the block at `0xB0106` |
| 8 | `0x121211` | **three random sample-and-hold generators** (section 5) |
| 9 | `0x1212E7` | 8 plain calls through the **slot table** `DM(0xC01D6..)` (section 6) |
| 10 | `0x121345` | two 2-iteration loops that crossfade into `0xC010A` and `0xC0112` using tables at `0x95100` and `0x95102` |
| 11 | `0x1213E9` | the **module chain** (section 6) |

No instruction of the pass decodes an ARM command (the SY1SY2 FIFO decoder pattern, section 7 of [sy-dsp-map](sy-dsp-map.md), does not occur: no `FEXT ... 0x1C:0x4`, no `0x700` mask). Parameters therefore arrive by writes to the module records and tables themselves (ASSUMED: external-port DMA into DM), which fits the two-word `(id, value)` messages of [arm-dsp-protocol](arm-dsp-protocol.md) being addresses plus values on the DSP side.

## 4. Biquads: Butterworth at fs/4 (VERIFIED numbers, EMPIRICAL structure)

Three near-identical routines (`0x120DFF..0x120FE4`, `0x124E59..0x125036`, `0x1254A9..0x1256AC`; 201, 196 and 216 instructions; 26 to 28 `fA=fB*fY, fA+1=fB+1*fY` pairs each; `tools/sharc_routines.py`) are unrolled SIMD cascades of transposed direct form II sections:

```text
f8 = x*b0; f9 = x*b1; f10 = x*b2              ; coefficients r4 r5 r6 from PM (i9, stride 1)
y  = f8 + z1                                   ; f7 = f8 + f12
z1 = f9 - a1*y + z2 ; z2 = f10 - a2*y          ; f12 = f2*f7, f13 = f3*f7 (a1*y, a2*y)
```

The coefficient set at PM `0x95000` (and an identical one at `0x95005`) is `b = (0.2928932, 0.5857864, 0.2928932)`, `a1 = 0`, `a2 = 0.1715729`. These equal the second-order Butterworth low pass at fs/4: `b0 = 1/(2+sqrt 2) = 0.29289322`, `b1 = 2 b0`, `a2 = (2-sqrt 2)/(2+sqrt 2) = 0.17157288` (error below 2e-8, one float32 ulp). Reading (ASSUMED): a fs/4 low pass is the anti-alias and anti-image filter of a **2x oversampled** stage (the unrolled code reuses one coefficient load for three consecutive samples, and `0x120BAE` is called between the two coefficient sets). Not proven: the routine has not been run against a Python biquad; the sample-by-sample behaviour is read from the instruction pattern only.

## 5. Other routine families (EMPIRICAL census)

`python tools/sharc_routines.py listing.txt 0x120300 --min 40` (93 routines). Families that matter for `piano.rs`:

| Family | Routines | Evidence | Reading (ASSUMED) |
|---|---|---|---|
| Table interpolators, 65 points | `0x1254A9..` region of `0x1256AD` (1,440 instructions, 437 multiplies) | `R3 = FEXT R1 BY 0x1A:0x6` (6-bit index), `FDEP` fraction, `F4 = F4*F5 + F8*F6` between `DM(M3,I6)` and `DM(M4,I6)`, `I6 += 0x41` per parameter: rows of 65 entries (64 intervals) from `0xB4E97` | parameter mapping (key position or velocity to filter and gain values) with linear interpolation |
| Fractional delay line + all-pass | `0x1204D4`, `0x1206D3`, `0x1208D9` (235 to 287 instructions each, three copies) | `RECIPS` with two Newton steps on a value clipped at `0.01` (`0x3C23D70A`), `FEXT r by 0:0x10` fraction, `FLOAT ... by -15`, lerp `x*(1-f) + x'*f`, feedback gain `0.9908` (`0x3F7DA3C2`), `MAX`/`MIN` clipping | recirculating delay with loop gain (string or resonator). Same `0.9908` as ORG module records (5.27) |
| Random sample-and-hold | `0x121211..0x1212E7` | `R1 = R0*0x0BB38435 + 0x3619636B` (LCG constants), phase `R10 = R10*R1`, `COMP` with the previous phase, new value only `IF LT` (wrap), three channels at `DM(0xB0106)` offsets `-0x1A..` | slow random modulation |
| Module insert chain | `0x121112..0x1216FA` | section 6 | |
| Stereo pair mixer | `0x120454..0x1204D3` etc. | 15 multiplies, `ABS`, `FLOAT` | pan or level |
| `RSQRTS` | `0x125261` | one `RSQRTS` | gain from power (RMS or normalisation) |

There are only 8 distinct code pointers stored in data in this section (`0x120600`, `0x120620`, `0x12063E`, `0x1206BE`, `0x120B36`, `0x120F19`, `0x121E90`, `0x12637B`) and 4 immediate routine addresses in code (`0x124C3C`, `0x1252D6`, `0x125312`); the other routines are entered through pointers written at run time (section 6).

## 6. Slot table and module chain (VERIFIED layout, empty in the image)

**Slot table** `DM(0xC01D6 + 4 k)`, `k = 0..7`, read at `0x1212E7..0x12133D` as `I8 = DM(+0); I1 = DM(+2); I0 = DM(+1); CALL (M8, I8)`:

| Word | Image value | Meaning |
|---|---|---|
| 0 | `0x12637B` (all eight) | routine; the stub is `NOP; RTS` at the end of the code |
| 1 | `0xC00EA`, `0xC00F2`, `0xC00FA`, `0xC0102` (twice around) | state record, 8 words apart |
| 2 | `0xB52C7` | buffer |
| 3 | 0 | |

**Module chain**: head pointer `DM(0xC2F48)` (`0xC2E4A` in the image), further heads `DM(0xC2F82)`, ...; 18 delayed call sites at `0x121112..0x1216FA`. Record fields (offsets from the record pointer `I1`, read at `0x1210E9..0x121112`):

| Offset | Use | Image (record at `0xC2E4A`) |
|---|---|---|
| `-0x20` | routine (`I8`) | `0x12637B` |
| `-0x1F` | signal pair pointer (`I0`; `L, R` are `DM(0, I0)` and `DM(2, I0)`) | `0xC2E06` |
| `-0x1E` | pointer loaded into `I1` for the call (callee state) | 0 |
| `-0x1D` | link read as `I1 = DM(-0x1D, I1)` after the call (next record) | `0xC2E4A` (itself: end of chain) |
| `-0x1C` | gain step (float) | `-0.0099` (`0xBC23D70A`) |
| `-0x1B` | gain `g` (float, written back clipped) | `-0.5` |
| `-0x19` | limit `c` | `0.5` |

Per module: `g' = CLIP(g + step, c)`, `wet = g' + c`, signal pair `*= wet`, the module routine is called on the scaled pair, and `c - g'` is the complementary dry weight. With the image values (`g = -0.5`, `c = 0.5`) `wet = 0`: every module starts fully dry. (EMPIRICAL reading of the multiply and clip sequence; the link handling after the call is not fully understood: the stub does not touch `I1`, so the real callee state record must carry the next-record link at `-0x1D` too.)

So the chain is a stereo insert path with a ramped wet/dry crossfade per module; the image ships every slot as pass-through. The routines that populate it (sections 4 and 5) are not named by any static pointer, so which of them run for a given piano sound is decided by the ARM at run time. That is the missing piece for a 1:1 piano replication.

## 7. Cross-reference: the piano sample voice is the SMP image (VERIFIED code, ASSUMED role)

SMP (`bank0_CODE@0x51B574`) contains the loop that PNO lacks. Listing `0x1C07D8..0x1C08E3`, voice count from `DM(0x9268B)`:

| Step | Instruction evidence | Computation |
|---|---|---|
| Voice | `I3 = DM(M0, I4)` | voice record pointer from a list |
| Phase | `R0 = R6*R7 (UUI); R0 = R0 - R1`, 64-bit accumulate `R3 = R7 + R0 + CI` | integer position and fraction |
| Residual | `R5 = DM(I1, M4)`, `R5 = R5 OR LSHIFT R1 BY 16`, `R5 = FEXT R5 BY R0` with `R0 = width << 6`, `FDEP ... (SE)` | variable-bit-width signed residual from a packed stream |
| Predictor | `MRF = 0; MRF = MRF + R0*R1 (SSI)` x4 with `R0 = DM(I5, M2)` (history, `L5 = 0x10`) and `R1 = PM(I9, M9)` (per-voice coefficients) | 4-tap integer predictor, `y = pred + residual` |
| Interpolation | `I9 = R14 + (R1 >> 24)` with `R14 = 0xE3400`, `M11 = 0x100`, six `MRF = MRF + R0*R1 (SSF)` with `R1 = PM(I9, M11)` | **6-tap, 256-phase polyphase kernel** read with stride 256 |
| Output | `R3 = R2*R3 (SSF)`, `R5 = R2*R5`, added to `DM(M0, I7)` | left and right gain, mixed into the bus |

This matches the fixed-LPC residual codec of [nsmp4-and-npno-cnsp-codec](nsmp4-and-npno-cnsp-codec.md) (order 0..4, 1..16 bit residuals). The 1,536-word block at SMP `DM(0xE3400)` is that kernel in **Q31 (24-bit significant, low byte zero)**: six rows of 256 phases, row `t` and phase `p` at `0xE3400 + 256 t + p`; value `h[t][p] = sinc(d) * window(d)` with `d = (2 - t) + p/256` (peak 0x7FFFFF00 = 1.0 at `t = 2, p = 0`), exactly mirror symmetric (`h[3][p] = h[2][256-p]`, `h[4][p] = h[1][256-p]`, `h[5][p] = h[0][256-p]`, 0 differences). **Identified and bit-exact** ([smp-kernel-fit](smp-kernel-fit.md)): `h[t][p] = u + (1 - sum_t u) / 6` with `u = sinc(d) (0.54 + 0.46 cos(pi d / 3))` (Hamming window, half width 3) and word `min(round(h 2^23), 2^23 - 1) << 8`; 0 mismatches over 1,536 words (VERIFIED, `tools/ns4_smp_kernel_fit.py`). The earlier fits (3-term cosine rms 1.9e-4, Kaiser rms 4.6e-4) missed the additive per-phase DC correction. It is a different kernel from the 12-tap float set of SY1SY2.

For `piano.rs` and `sample_osc`: the sampler path of the hardware is **6-tap Q31**, not 12-tap float; see TD-08. `crates/nord-core/src/piano/q31_kernel.rs` implements the kernel (default `SampleInterp::SmpQ31`).

## Implications

- The PNO image is block-level stereo post-processing around the sample voices, not the voices themselves. Replicating it 1:1 needs the module installation order from the ARM side (which routines are put in the 8-slot table and the chain for each piano sound), not more static reading.
- `piano.rs` sample interpolation follows SMP (6 taps, 256 phases, Q31; window identified, implemented as `SampleInterp::SmpQ31`); the SY1SY2 12-tap kernel is the synth path only.
- The `0.292893 / 0.585786 / 0.171573` coefficients are a VERIFIED constant of the firmware (Butterworth fs/4); where they are applied (oversampling filter) is ASSUMED.

## Follow-ups

1. Find the ARM code that writes the slot table and chain heads (`0xC01D6`, `0xC2F48`, `0xC2F82`) as `(id, value)` messages: join with the 904 `0xC006CA48` call sites of [arm-dsp-protocol](arm-dsp-protocol.md); the ARM holds DSP addresses as constants (for example `0xE3400` is loaded at `0xC0001BAC` and stored into a table).
2. Run one biquad routine under `sharc-run` against a Python transposed direct form II model (needs `MODE1` SIMD bits and `M`/`I` registers preset at an `--enter` point) to turn section 4 from EMPIRICAL into VERIFIED.
3. Run SMP's voice loop (`0x1C07D8`) on a CNSP page from a backup to compare decoded and interpolated samples with `crates/nord-core` (the fixed-LPC decoder exists).
4. ~~Identify the SMP kernel window~~ Done: Hamming 0.54/0.46 with additive DC normalisation ([smp-kernel-fit](smp-kernel-fit.md)).
5. Trace which of the 93 routines the P4I pass calls once the slot table is populated (for example by poking `0xC01D6..` with the routine addresses of section 5).

## Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_ldr.py DIR/dsp_image_1_PNO.bin --sram SRAM
python tools/adi_dis.py SRAM/core1_sram.bin 0x240600 0x24c700 > pno1.txt
python tools/sharc_routines.py pno1.txt 0x120300 --min 40
python tools/adi_dis.py SRAM/core1_sram.bin 0x240000 0x240600 --isa     # vectors
```

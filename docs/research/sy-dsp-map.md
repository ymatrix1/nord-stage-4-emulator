# SY1SY2 DSP image map (synth engine) and the interpolation kernel set

- **Checked:** 2026-11-12
- **Sources (local, never committed):** the SY1SY2 image of OS v1.66 (`bank0_CODE@0x4FD980`, 121,844 B) exported with `tools/ns4_dsp_images.py`, flat SRAM image from `tools/ns4_dsp_raw.py --sram`; ADI `elfdump` listing of its code (`tools/adi_dis.py 0x380600 0x3847E6`, CCES 2.12.1); `tools/ns4_sinc_check.py` and `tools/ns4_synth_kernels.py` for the kernel numbers; `tools/sharc_routines.py` for the routine census; [sharc-sy1sy2-smp](sharc-sy1sy2-smp.md) (container, vectors), [dsp-image-hunt](dsp-image-hunt.md) 5.14 and 5.15, [arm-dsp-protocol](arm-dsp-protocol.md), [nsmp4-and-npno-cnsp-codec](nsmp4-and-npno-cnsp-codec.md); `.reference/` is not needed beyond the ADI tools.
- **Question:** What is the synth voice structure of the SY1SY2 image (oscillators, filters, envelopes), what do the 18,432 floats at DM `0xE3400` mean, and what does that say for TD-08 (the sinc kernel that is not wired into `synth.rs`)?

Tags: **VERIFIED** = read in the ADI listing or reproduced by a command below; **EMPIRICAL** = textual shape of the code or a fit; **ASSUMED** = reading of the purpose.

## 1. Verdict

| Question | Answer | Tag |
|---|---|---|
| The 18,432 floats at `0xE3400` | **Six Kaiser-windowed sinc kernels**, 12 taps x 256 phases each, `h[t][p] = sinc(fc d) * kaiser(d)`, `d = t - 5 - p/256`, Kaiser `beta = 8.5`, half width 6, `fc = 1.100, 1.050, 1.000, 0.960, 0.925, 0.895`. Maximum error against the model 5.9e-8 (one float32 ulp), rms 1.1e-8 over all 18,432 values. The earlier 3-term cosine window (0.4231, 0.4991, 0.0778; rms 3.6e-5) was a fit of this window. | VERIFIED |
| Who reads it | One routine, `0x1C10EE`: a SIMD stereo 12-tap FIR with the kernel read from PM at stride 256 (`R4 = PM(I9, M11)`, `M11 = 0x100`). Each of the 56 voice records holds its kernel base (initially `0xE3400`) at record word `+0x3B`. | VERIFIED |
| Kernel selection | No instruction of the DSP program writes the kernel pointer; the six kernels are 3,072 words (`0xC00`) apart. The selection by playback ratio is therefore done by the ARM (a write to the voice record) or never; the ARM holds `0xE3400` as a constant (`bank0_CODE@0xC0001BAC`). | VERIFIED (negative on the DSP), HYPOTHESIS (ARM) |
| Voices | 56 voice records, 28 SIMD pairs, 120-frame blocks. | VERIFIED |
| Oscillators, filters, envelopes | The image is a **sample-based voice engine**: packed-residual unpackers (1 to 15 bits), the 12-tap sinc resampler, per-voice parameter smoothing and gain. No waveform generator, no filter cascade and no envelope generator was found by the routine census (no `biquad` pattern at all; the only two `RECIPS` instructions are in the audio ISR). Subtractive-synth filters and envelopes therefore run elsewhere (ARM or other images) or are in the sample material. | VERIFIED (absence by census), ASSUMED (consequence) |

## 2. TD-08: the kernel is a Kaiser-windowed sinc

Check (new tool, no numpy):

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_sinc_check.py DIR/dsp_image_3_SY1SY2.bin
# 6 kernels at image offset 0x76a8
# fitted Kaiser beta = 8.5000 (nominal 8.5)
# kernel 0: fc 1.100  rms 1.03e-08  max 5.88e-08  mirror 0e+00  tap sum 0.90899..0.90907 (1/fc 0.90909)
# ... kernel 5: fc 0.895  rms 1.21e-08  max 5.95e-08
```

| Property | Result | Evidence |
|---|---|---|
| Shape | `h = sinc(fc d) * kaiser(d)` | max error 5.9e-8 for all six blocks (test: `tools/tests/test_ns4_sinc_check.py`) |
| Free fit | `fc = 1.1000000, 1.0500000, 1.0000000, 0.9600000, 0.9250000, 0.8950000`, `beta = 8.50002`, half width `6.00001` | scipy least squares over all 18,432 values (analysis only, rms 1.0e-8); `fit_beta` in the tool recovers `8.5000` with golden-section search |
| Edge | `d = +6` (tap 11, phase 0) is **not** zero: it is `sinc(6 fc) / I0(8.5)` (`6.7e-5` for kernel 0). The model must keep `|d| <= 6`, not `< 6` | element `[11][0]` of each kernel |
| Symmetry | `h[t][p] = h[11-t][256-p]`, bit-exact, 0 differences | `mirror_error` |
| DC gain | per phase `1/fc` to 1e-4 (tap sum 0.90899..0.90907 for kernel 0): the kernels are not normalised | `dc_gain` |
| Phase-0 column | `h[5][0] = 1.0`; for `fc = 1` it is an exact delta (other taps 0) | element check |
| Layout | `[tap][phase]`, 256 floats per tap row; a tap row stride of 256 words is exactly `M11 = 0x100` | listing `0x1C10F7` ff. |
| Blocks | six blocks of 3,072 floats at `0xE3400 + 0xC00 k`, `k = 0..5` | image offset `0x76A8` |

Cut-off meaning: `sinc(fc d)` is a low pass with cut-off `fc/2` cycles per input sample; `fc = 1` is Nyquist, `fc = 1.1` passes slightly above it, `fc = 0.895` is `0.4475 fs`. The DC gain `1/fc` shows that the table is **not** amplitude-normalised; the voice multiplies by a gain afterwards (Stage B, section 5).

Evidence that the hardware window is the real Kaiser and not the 3-term fit: with Kaiser the residual is float32 rounding (a 1-ulp wobble at values 0.5..1: `5.96e-8`), the 3-term fit leaves 3.6e-5. Alternatives tested and rejected by residual: exact Blackman (rms 2.6e-4), classic Blackman (3.0e-4), Hann (8.6e-3), Hamming (1.1e-2).

TD-08 consequence: `crates/nord-core/src/sinc.rs` should replace `WINDOW` by `kaiser(d) = I0(8.5 sqrt(1 - (d/6)^2)) / I0(8.5)` and use `|d| <= 6`; this changes values by at most 8.8e-5 (the old rms was 3.6e-5), so the golden tests stay valid at their tolerance but the 'EMPIRICAL' tag on the window becomes VERIFIED. The remaining open point of TD-08 is the kernel selection rule; it is not in the DSP code (this section) and must be taken from the ARM side. Also: the hardware sampler path (SMP image) uses a different 6-tap Q31 kernel ([pno-dsp-map](pno-dsp-map.md) section 7), so `sinc.rs` is the synth oscillator path only.

## 3. Memory map (VERIFIED from image data and listing)

| DM range | Content |
|---|---|
| `0x92180` | `0x40` = maximum commands handled per audio ISR |
| `0x92182..0x92191` | 16-entry command handler table (section 4) |
| `0x92194..0x9219C` | saved `MODE1`, 64-bit voice event masks (`0x92196`, `0x9219A`, `LW`) |
| `0x921BE + 0x16 k` | 22-word sub-records of voice pair `k` (`k = 0..27`), two lanes per field for SIMD (`1.0 1.0 1.0 1.0`, `0.979` = `0x3F7A9FBE`, `0x04000036` x2, `0.0046` x4, `0.05` x2 at initial state) |
| `0x92400 + 0x38 k` | voice records, 56 words stride (`I3 = base + 0x26` in the loops) |
| `0x932CA`, `0x934AA` | the two ping-pong output frames (`0x78` iterations of a stride-2 store = 120 frames of left and right); `DM(0x93590)` is the frame being written; the DMA-done ISR `0x1C058C` flips it |
| `0x93E20..0x93E34` | pass counter (`+2` per ISR) and phase timing words |
| `0x93046` | start of the command FIFO read pointer `I0` |
| `0xB6000..0xB6618` | queues of the 2nd ISR path (`0x1C0852`: flags `0xB6001`, `0xB6510`, list `0xB6511`; role ASSUMED) |
| `0xC0000 + 0x100 k` | per-voice 256-word circular sample buffer (record word `+0x26` = base, `+0x31` = base) |
| `0xE3400..0xE7BFF` | the six kernels (section 2) |

Vectors and container: see [sharc-sy1sy2-smp](sharc-sy1sy2-smp.md). The audio ISR is the vector-68 target `0x1C05B9`; the vector-80 target `0x1C058C` is the DMA-done ping-pong switch; vector 52 `0x1C03FD` resets the FIFO read pointer (`I0 = 0x93046`) and re-enables the interrupt.

## 4. Mailboxes: the command FIFO (VERIFIED by listing)

The audio ISR (listing `0x1C0638..0x1C064B`, also in a second ISR at `0x1C0852..`) drains a command FIFO before it processes voices:

```text
n   = min( (DM(0x2400) + R3 - I0) >> 1 , DM(0x92180) )       ; pairs between the DMA write index and I0
loop n:  R0 = DM(I0, M1); R5 = FEXT R0 BY 0x1C:0x4           ; opcode = bits 31..28
         R6 = FEXT R0 BY R10 (0x700)                          ; bits 27..0 (pos 0, len 28)
         R7 = DM(I0, M1)                                      ; value
         IF EQ I6 = R6 ; IF NE I8 = DM(M5, I1)                ; I1 = 0x92182, M5 = opcode
         IF EQ DM(I6, 0) = R7 ; IF NE CALL (M8, I8)           ; opcode 0: store, else call handler
```

So the DSP side of the ARM's two-word message ([arm-dsp-protocol](arm-dsp-protocol.md)) is `[opcode:4 | DM address:28]`, `value`: opcode 0 writes `value` to `DM(address)` (the ARM sends DSP addresses as 28-bit ids; they are not stored as literals in the ARM code but are added to a channel base), opcodes 1 to 15 call the handlers at `DM(0x92182 + opcode)`: `0x1C0300, 0x1C031F, 0x1C0355, 0x1C0359, 0x1C035D, 0x1C030F, 0x1C0304, 0x1C0381, 0x1C039E, 0x1C03AA, 0x1C03DD, 0x1C03F8..0x1C03FC`, small routines in the shared helper block (for example `0x1C030F` ANDs the value with `0x15` and writes the `FLAGS` pins; the others write IOP registers). The producer is the external-port DMA behind `DM(0x2400)` (ASSUMED). The 28-bit field makes every voice parameter, including the kernel base at `0x9243B + 0x38 k`, writable by the ARM. Dynamic check (feeding a command through `sharc-run`) is not done: the FIFO needs `I0`/`L0`/`B0` state that start-up code sets and a device model for `DM(0x2400)`.

## 5. Voice structure (VERIFIED code, EMPIRICAL roles)

Per audio block (120 frames) the ISR at `0x1C05B9` does, in listing order:

| Step | Listing | What |
|---|---|---|
| 0 | `0x2B9..0x2F8` | reentrancy guards `DM(0x93DF6/7)`, counter `DM(0x93E20) += 2`, clear timing words, three bookkeeping calls (`0x1C0B36`, `0x1C0B5A`, `0x1C11AC`) |
| 1 | `0x305..0x34B` | `MODE1 = 0x1001CFC`, FLAG handshake, **command FIFO** (section 4) |
| 2 | `0x353..0x376` | re-arm the DMA (`DM(0x1400..0x1404)`: `0x15`, `0x1B58`, `0x6BD`) |
| 3 | `0x379..0x3E6` | **28 SIMD iterations** over the sub-records at `0x921BE` (stride `0x16`): `F0 = RECIPS F12` with two Newton steps, `R4 = FEXT R13 BY R8`, table fetches `DM(M5, I5)`, `R12 = FUNPACK F12` (16-bit float table), `CLIP`, `MIN`, `MAX`, `COPYSIGN`: per voice pair rate and gain computation (EMPIRICAL) |
| 4 | `0x3E8..0x40B` | 56-voice loop: `F0 = (F0 - F1) * F4 + F1` style smoothing of per-voice parameters, `R5 = FIX F5 BY 31` |
| 5 | `0x41F..0x430` | **Stage A**, 56 times: `I8 = DM(rec - 0x12)`, `CALL (M8, I8)`, `M5 = 0x38` stride |
| 6 | `0x43D..0x45B` | helpers `0x1C0BAE`, `0x1C0BF9`; clear the output frame (`0x78` stride-2 stores at `DM(0x93590)`) |
| 7 | `0x475..0x493` | **Stage B**, 56 times: `I8 = DM(rec - 0x11)`, `CALL (M8, I8) (DB)`, `MODE1 |= 0xE00000` (`PEYEN`, `BDCST1`, `BDCST9`), `M11 = 0x100`, `L0 = 0x100`, `R9 = 1.0`, `R11 = 0.046` (`0x3D3CBACC`), then `RTI` |

Each voice record has two function pointers: **Stage A at record word `+0x14`** (initially `0x1C07B0`) and **Stage B at `+0x15`** (initially `0x1C10ED`). Stage A is a per-voice state machine: it replaces its own pointer (`0x1C0811`, `0x1C083C`, `0x1C0851`) and sets or clears the voice's bit in the 64-bit masks at `DM(0x92196)` and `DM(0x9219A)` (key on and off events, ASSUMED). The initial Stage-B pointer `0x1C10ED` is an `RTS`; Stage A installs the resampler `0x1C10EE` into word `+0x15` when the voice starts (listing `0x552`: `R1 = 0x1C10EE; DM(-0x11, I3) = R1`).

**Stage B resampler `0x1C10EE`** (listing `0xDEE..0xE99`, 60 instructions):

```text
phase  : R2 = R0 + R1 ... FEXT by R14 (0x211), R13 (0x1D9)       ; integer step and 8-bit phase index
i0     = modify(i2, m3)         ; sample pointer = circular buffer base + integer position
i9     = modify(i10, m12)       ; I10 = kernel base from record +0x3B (dm(0x15, i1)), M12 = phase index
12 x   : R0 = DM(I0, M2) , R4 = PM(I9, M11)                       ; M2 = 2 (stereo pair), M11 = 0x100 (tap stride)
         F12 = F0*F4 ; F8 = F8 + F12                              ; accumulate
second 12 taps with M3/M12 taken from the PEy side (S6, S2)       ; a second read position
mix    : F0 = F0*F5 ; F0 = F0 + F1 ; ... output to DM(M4, I5)     ; two positions combined and written
```

The two PEs run on adjacent data words (left and right) with the same broadcast coefficient (`BDCST9`), so one pass of 12 MACs yields a stereo sample; the code then repeats the 12 taps for a second read position (EMPIRICAL: the combination of the two positions is a crossfade or a loop splice, not decoded). The kernel base of the voice selects among the six kernels; phase resolution is 1/256 sample, no interpolation between phases is performed (the phase index goes straight into `M12`; the arithmetic after the two FIRs combines the two positions, not two phases, EMPIRICAL).

**Packed-sample unpackers** (`0x1C1228..0x1C2303`, 14 routines, `tools/sharc_routines.py`): one routine per residual width `n = 1 .. 15` bits (`FEXT R0 BY pos:n`, 32 fields per call, `FDEP (SE)` sign extension, `FLOAT ... BY -19`, store with stride 2), each 105 instructions (the first, `0x1C1228`, holds the 1-bit variant and one extra). This is the CNSP residual stream of [nsmp4-and-npno-cnsp-codec](nsmp4-and-npno-cnsp-codec.md) (1..16-bit signed residuals) unpacked to floats scaled by `2^-19`; the prediction and the gain are applied elsewhere (ASSUMED). A routine for 16 bits was not found.

Not in the image (VERIFIED by census): biquad filters (0 `fA=fB*fY, fA+1=fB+1*fY` pairs), table interpolators, delay lines with feedback, LFO or envelope generators. The synth sound design layers (filters, ADSR, FM, ...) are not computed here; see [synth-engine](../specs/synth-engine.md) for the ARM-side status.

## 6. Open points

| Item | Status |
|---|---|
| Which of the two Stage-B read positions is which (loop splice, crossfade, or 2x oversample) | open; needs a run with a voice set up |
| Selection of the kernel by the ARM (value `0xE3400 + 0xC00 k` sent to record word `+0x3B`) | **decoded** in [synth-sample-kernel-rule](synth-sample-kernel-rule.md): by voice key (`<= 47` kernel 0, then every 12 keys), not by ratio; `0xC0001BAC` is a static channel record |
| Handlers of opcodes 1 to 15 | table read back and handlers described in [sy-smp-audio](sy-smp-audio.md) section 3; the voice start needs opcode 0 only |
| The parameter-slewing math of step 3 | shape only (`RECIPS`, `FUNPACK`, `CLIP`) |
| 16-bit unpacker | not found |

**Update 2026-11-13 ([synth-voice-control](synth-voice-control.md)):** the synth edit ids do not talk to this image (except the tune ids, which write `b[20], b[21]` of the 28 pair blocks); they write the Core2 channel strips on ORG and PNO. The voice gain smoother of step 4 is measured: `s' = t + 0.979 (s - t)` per frame.

## Implications

- TD-08 reduces to a window swap in `sinc.rs` (Kaiser 8.5) plus a kernel selector. The selector cannot be decoded from the DSP; it is an ARM-side decision.
- The synth image does not contain the synth's filters, envelopes or oscillators. A 1:1 replication of `synth.rs` from this image covers sample playback only.
- The DSP side of the ARM message protocol is now decoded: `[opcode:4 | address:28]`, `value`, a store into `DM(address)` for opcode 0.

## Follow-ups

1. ~~Run Stage B under the oracle with a planted voice record, sample buffer and phase and compare against a 12-tap model.~~ Done in [oracle-vs-engine.md](oracle-vs-engine.md): record layout, position word and bit-exact agreement with `SincKernelSet` arithmetic rules. The role of the second read position (mask words at frame `+0x28`) is still open.
2. Decode the ARM-side voice parameter writes: the 28-bit address field against `0x92400 + 0x38 k + field` and the kernel pointer write.
3. Wire the Kaiser window into `crates/nord-core/src/sinc.rs` (separate change; this lane touches docs and tools only).
4. Identify the 16-bit residual unpacker (or confirm the encoder never emits width 16 for synth material).

## Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_raw.py DIR/dsp_image_3_SY1SY2.bin --sram sy.bin
python tools/adi_dis.py sy.bin 0x380600 0x3847e6 > sy_adi.txt
python tools/sharc_routines.py sy_adi.txt 0x1c0300 --min 25
python tools/ns4_sinc_check.py DIR/dsp_image_3_SY1SY2.bin
python tools/arm_xref.py literal target/fw_unpacked/bank0_CODE.bin 0xE3400
```

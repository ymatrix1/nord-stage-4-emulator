# SMP compressed voice: record layout, real piano pages on the oracle

- **Checked:** 2026-11-12
- **Sources (local, never committed):** SMP image of OS v1.66 (`tools/ns4_dsp_images.py`, flat SRAM from `tools/ns4_dsp_raw.py --sram`); ADI `elfdump` listing of `0x1C0680..0x1C0858` and `0x1C07D8..0x1C08E3` (`python tools/adi_dis.py smp.bin 0x380D00 0x380FB0` and `0x380FB0 0x3811D0`, CCES 2.12.1); factory backup `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (`ns4 backup --extract`): `Piano/Grand/Bright Grand Sml 6.1.npno`, `Piano/Clavinet/Clavinet D6  6.1.npno`, `Piano/Electric/Wurlitzer 1  6.3.npno`; `crates/nord-sharc` (`sharc-ref`); [sy-smp-audio](sy-smp-audio.md), [smp-kernel-fit](smp-kernel-fit.md), [pno-dsp-map](pno-dsp-map.md) section 7, [nsmp4-and-npno-cnsp-codec](nsmp4-and-npno-cnsp-codec.md).
- **Question:** What are the fields of the SMP voice record that the loop at `0x1C07D8` reads, can a real compressed `CNSP` piano page be played through that loop on the oracle, and does the output equal the Rust sample path?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = measured or read from code shape, **ASSUMED** = reading of the purpose.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| Record layout | 15 words between `I3 - 32` and `I3 + 4` (section 2). Every field was set by the scenario generator and the oracle result equals the Rust model in all of them; the fields that the first runs left at zero (`-24` history phase, `-29` initial phase, `-27` base, gains) were varied in separate runs | VERIFIED |
| Stream format | A `.npno` page needs **no re-layout**: its payload words (16-bit big-endian, bits taken LSB first) are the 16-bit units the loop reads, one per DM word (low half, high half zero). `.nsmp4` strokes (big-endian 32-bit words, MSB first) are a different layout; how the ARM feeds them is not known | VERIFIED (npno), open (nsmp4) |
| Oracle against the Rust model (`piano/smp_voice.rs`, `smp_page.rs`) | **Bit-exact** in every run: counter, phase, the 16-word history rings, `R2` after the shift (record `+4`), and the bus words (section 5): 17 runs, about 14,000 output frames, 0 mismatches | VERIFIED |
| Oracle against the engine path | The DSP rounds the interpolated sample to a whole `y` (units of 1/8192) before scaling, the f32 engine path does not: max difference **0.5 LSB of the 1/8192 scale**, SNR 66 to 79 dB (76 dB typical) at fractional steps, 139 dB at step 1. With the new `SmpKernel::interpolate_hw` (integer MAC on the decoded stroke of `decode_npno_stroke`) the engine equals the DSP in 100 % of the frames | VERIFIED |
| Oracle bug found and fixed | A write to a `B` register did not load the `I` register. The loop relies on it (`B1 = DM(..)` then reads through `I1`); before the fix the voice read the ring through `I1 = 0`, decoded nothing, wrote nothing | VERIFIED (`memory.rs` test; the run above) |

## 2. Voice record (VERIFIED unless marked)

`I3` points into the middle of the record; the list of voices is `DM(0x9268C)` (initially `0x92534`), `DM(0x9268B)` entries are processed by this loop (`DM(0x9268A)` by the second loop, section 7). Offsets are signed word offsets from `I3` (listing form `0xFFFFFFxx`):

| Offset | Name | Role | Evidence |
|---|---|---|---|
| `-32` | `step_int` | `counter += step_int + carry` | `r0 = dm(-0x20)`, `R3 = R7 + R0 + CI` |
| `-31` | `step_frac` | `phase += step_frac` (32-bit, carry out) | `r1 = dm(-0x1F)`, `R4 = R4 + R1` |
| `-30` | `counter` | residual number `n` | `r7 = dm(-0x1E)`, written back at the end |
| `-29` | `phase` | kernel phase index = `phase >> 24` (value before this call's add) | `r1 = dm(-0x1D)`, `R1 = LSHIFT R1 BY -24`, `I9 = R14 + R1` |
| `-28` | `stride` | `bit = stride * counter - base` (`UUI` multiply, low 32 bits) | `r6 = dm(-0x1C)`, `r0 = r6*r7 (uui)` |
| `-27` | `base` | bit offset subtracted; `-width` selects the right channel of an interleaved stereo page | `r1 = dm(-0x1B)`, `r0 = r0 - r1` |
| `-26` | stream ring | `B1` (and `I1`), `L1 = 0x100` words | `b1 = dm(-0x1A)` |
| `-25` | history ring | `B5` (and `I5`), `L5 = 0x10` words | `b5 = dm(-0x19)` |
| `-24` | `hist_off` | history slot `s = 2 * ((hist_off + counter) & 7)` | `r8 = dm(-0x18)`, `r8 + r7`, `AND 0xF`, `LSHIFT 1`, `M4` |
| `-19` | `width` | residual bits, `FEXT` length (`width << 6`) | `r0 = dm(-0x13)`, `LSHIFT R0 BY R11` (`R11 = 6`) |
| `-18` | bus pointer | `I7`; left word at `+0`, right at `+1` (`(LW)` accumulate) | `i7 = dm(m6,i3)`, `M6 = -18` set at `0x1C07B1`, before the loop |
| `-17` | predictor pointer | `I9`, four coefficients, oldest history first (PM read, same address space) | `r6 = dm(-0x11)`, `i9 = r6` |
| `0`, `+1` | gain L, R | Q31 `SSF` multipliers | `r3 = dm(0,i3)`, `r5 = dm(0x1,i3)` |
| `+4` | last output | `R2` after `LSHIFT 17`, stored | `dm(0x4,i3) = r2` |

Two width fields exist because of stereo pages: `stride = channels * width` positions the residual, `width` extracts it. For an interleaved page (`L0 R0 L1 R1 ...`) the left voice has `base = 0`, the right voice `base = -width`. The scenario uses two voices on one ring; the right voice is bit-exact, which proves the field roles (VERIFIED). Registers fixed by the ISR before the loop (listing `0x1C0680..0x1C07D8`): `M0 = 0, M1 = 1, M2 = 2, M3 = -12, M5 = -32, M6 = -18, M9 = 1, M11 = 0x100`, `R9 = 0xF`, `R10 = 0xFF`, `R11 = 6`, `R14 = 0xE3400` (kernel base), `R15 = 0x11`, `L1 = 0x100`, `L5 = 0x10`, `I4 = DM(0x9268C)`.

## 3. What one call does (VERIFIED, `smp_voice::SmpVoice::call`)

1. `bit = stride * counter - base`; `off = bit & 15`; `idx = (bit >> 4) & 0xFF`; `w = ring[idx] | ring[idx + 1] << 16` (ring wrap at 256), `w >> off`, `FEXT width`, `FDEP (SE)`: residual `r`.
2. History slot `s = 2 * ((hist_off + counter) & 7)` (stride-2 ring of 8 samples); `pred = sum_i c_i * h[s + 2 i]`, `i = 0..3` (`SSI`, 32-bit); `y = pred + r`; stored at `s + 8`.
3. The six interpolation taps are `h[s + 12], h[s + 14], h[s], h[s + 2], h[s + 4], h[s + 6]` = `y_(n-6) .. y_(n-1)`. The sample just decoded (`y_n`) is **not** used: the loop decodes one sample ahead. Kernel row `t` is `DM(0xE3400 + 256 t + (phase >> 24))`; `R2 = MRF (SSFR)` rounds to a whole `y`. The output position is therefore `n - 4 + phase / 256` in decoded samples.
4. `R2 <<= 17`; `L += (R2 * gain_L) >> 31`, `R += (R2 * gain_R) >> 31` (`SSF`, floor), then the 32-bit add into the bus. Unity gain `0x7FFFFFFF` gives `bus = y << 17`, so a full-scale 14-bit sample (8191) is `0.5` of Q31: the engine amplitude is `bus / 2^30`.
5. `phase += step_frac`; `counter += step_int + carry`.

Step above 1: `counter` then skips residuals, so the predictor loses samples and the voice decodes garbage (the model equals the oracle at step 1.3, the engine does not, ASSUMED consequence: the ARM keeps the step at or below 1 per voice and chooses the multisample so that `35 kHz / 48 kHz * 2^(k/12) <= 1`; the key map has a sample per key, this fits up to about 5 semitones above the root).

## 4. Stream layout and the ARM-side question

| Source | Layout | DSP reads it as |
|---|---|---|
| `.npno` page (1022 B mono, 2044 B stereo) | 2-byte header `[atten, order<<5 \| nbits]`, then 16-bit big-endian words, residual fields taken LSB first (stereo interleaved `L R L R`) | the word-for-word copy, one 16-bit word per 32-bit DM word | 
| `.nsmp4` frame | control word, then big-endian 32-bit words, residual fields taken MSB first | not the same bit order; reversing a whole frame would reverse the sample order, so a per-field repack is needed. The ARM step is not observed here |

The page fits the DSP without conversion (VERIFIED: 510 or 1021 payload words go into the ring unchanged, bit-exact decode for widths 5, 8, 9, 11, 12 and orders 1, 2, 3). The 256-word ring holds a quarter to half of a page (1021 words stereo); the ARM refills it. The scenario refills it from a `poke` list ahead of the read position and the ring wraps four times in the long runs, so the circular `(bit >> 4) & 0xFF` addressing and the two-word fetch at the wrap are covered.

## 5. Comparison runs (VERIFIED, `smp_voice_ref`, oracle = `sharc-ref` on the SMP image)

Common: gain `0x7FFFFFFF` unless stated, page 0 of the stroke, history from the stroke descriptor (`+0x14`, `+0x1C`), kernel from the image. "Exact" = oracle value equals the Rust model, bit for bit; the five lines are counter, phase, history rings, `R2 << 17`, bus.

| Run | Frames | Exact (all five series) | Engine f32 path | Engine `interpolate_hw` |
|---|---|---|---|---|
| Bright Grand Sml, stroke 2 (stereo, 9 bit, order 2), step 0.5 | 1800 | 1800 / 1800 / 57,600 / 3,600 / 3,600 | max 0.500 LSB, SNR 79.1 dB L, 77.7 dB R | 1788 / 1788 both sides |
| same, step 0.8749 | 1000 | all | max 0.500 LSB, 76.0 / 74.6 dB | 993 / 993 |
| same, step 1.0 | 900 | all | max 0.001 LSB, 138.7 / 139.1 dB | 894 / 894 |
| Wurlitzer 1, stroke 1 (mono, 8 bit, order 3), step 0.6 | 1100 | all | max 0.500 LSB, 76.4 dB | 1090 / 1090 |
| Wurlitzer 1, stroke 150 (mono, 11 bit, order 3), step 1.0 | 600 | all | 138.6 dB | 594 / 594 |
| Clavinet D6, stroke 1 (stereo, 9 bit, order 2), step 0.7 | 1000 | all | | 991 / 991 |
| Bright Grand strokes 0 (12 bit, order 1), 20 (5 bit, order 3), 100 (9 bit, order 3), 390 (11 bit, order 3), step 0.6 | 800 each | all | | 790 / 790 each |
| Field probes on stroke 2: `hist_off = 5`; initial `phase = 0xC0000000`, step 0.43; gain `0x2AAAAAAA`, step 0.75; `base = -48` (3 words in) | 700 each | all | | 687 to 692 each (base probe: engine skipped) |
| Step 1.3 (skips residuals) | 650 | all | engine differs (expected) | |
| Synthetic pages (no firmware data in the test): stereo order 1, 6 bit, step 0.6; mono order 3, 5 bit, step 0.8749 | 400, 500 | all | | |

The engine columns compare from `counter >= 6`: before that the DSP history is zero (the descriptor's four initial samples sit in the LPC state, not in the output history) while `SmpKernel::interpolate` repeats the edge sample (ASSUMED engine rule; the real ARM preload is unknown). The two synthetic cases are committed as golden fingerprints (`smp_page` test `test_synthetic_pages_reproduce_the_oracle_bus_capture`, FNV-1a of the bus capture `f073fc2a5525ca67` and `28f2016949294371`), so CI checks the Rust model against the firmware's arithmetic without the firmware.

## 6. Rust model changes

| Change | Why | Tag |
|---|---|---|
| `piano/smp_voice.rs`: `SmpVoice::call` | integer model of the loop above, 8 unit tests | VERIFIED against the oracle |
| `piano/smp_page.rs`: `NpnoPage`, `page_voices`, `play_page`, `synthetic_page`, `fnv64` | page parse (mono and stereo), the record set-up an ARM would perform, DMA-style ring refill, goldens | VERIFIED |
| `SmpKernel::interpolate_hw` | the DSP rounds the six-tap sum to a whole `y` before the `<< 17`; the f32 path keeps the fraction. The engine default stays f32 (the f32 path is not worse for audio and the synthetic test PCM is not integer-valued); `interpolate_hw` is the bit-exact variant for integer PCM | VERIFIED |
| oracle: `B` write loads `I` | PRM circular buffering; needed by this loop | VERIFIED |
| oracle: `capture ... part=lo\|hi` | integer words in the f32 dump without NaN loss | tested |

## 7. Not done, open

| Item | State |
|---|---|
| Second loop (`0x1C0865..0x1C08E3`, `DM(0x9268A)` entries) | listing only: same fields, `R13 = 0x20`, three-unit window and SIMD mode (`MODE1 0xE00000`), residual added through `MRF = R6 * R2 (SUI)` with `R2 = 1`, one bus word per voice with gain `+0`. Not run; the CNSP widths (1..16) are served by the first loop. EMPIRICAL |
| Page boundaries | page `p > 0` restarts with the history 64 frames before the end of page `p - 1` and a new width/order/ring: the ARM must rewrite the record (and for a changed width recompute `counter`/`base` so that `stride * counter - base` is the page position). Not driven; the decoder side is the Rust `decode_npno_stroke` |
| ARM-side refill and record writes | the ARM code that writes these records (`0xC0503600..` init script, SMP FIFO messages) is not decoded; the pitch (`step`) to MIDI note map is open |
| `.nsmp4` strokes | layout differs (section 4) |
| Stage A/B state machine | the SY voice start (prepare/go) has no counterpart in this loop: the voice runs whenever its record is on the list |
| Edge handling at stream start and end | zero history here, edge repeat in the engine |
| Accuracy of `(LW)` in SIMD loop B | not exercised (listing only) |

## 8. Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_raw.py DIR/dsp_image_4_SMP.bin --sram smp.bin
cargo build --release -p nord-sharc -p nord-tools --bin smp_voice_ref
target/release/ns4 backup "Nord Stage 4 Factory Restore RevK v1.66.ns4b" --extract "Piano/Grand/Bright Grand Sml 6.1.npno" target/oracle/grand.npno
# scenario -> oracle -> compare (file name `synthetic:ORDER:NBITS:CHANNELS` needs no firmware data)
target/release/smp_voice_ref target/oracle/grand.npno target/oracle/g --step 0.5 --frames 1800
target/release/sharc-ref smp.bin target/oracle/g.scn --out target/oracle/g
target/release/smp_voice_ref target/oracle/grand.npno target/oracle/g --step 0.5 --frames 1800 --compare
# options: --stroke N --gain G --hist-off N --phase0 W --base-words K
cargo test -p nord-core piano::smp_ && cargo test -p nord-sharc
```

## Implications

- The piano sample path of the hardware is now reproduced end to end: stream words, fixed-LPC decode, 6-tap kernel, rounding, gain and bus add, bit for bit. The engine differs only by the final rounding to 1/8192 (`interpolate_hw` closes it) and by the start-of-stream history.
- The re-layout question for `.npno` is closed: none is needed. A faithful piano loader can hand page words to the DSP unchanged.
- The step limit (one decoded sample per call) constrains the pitch range per multisample; the engine does not model it.

## Follow-ups

1. Decode the ARM record writer (find the stores to `rec - 32 .. rec + 1` for the SMP list entries) to confirm `stride`, `base`, `hist_off` conventions and the page-switch rule.
2. Drive a page boundary (new width and order, history reload) and the second loop.
3. Decide whether `PianoLayer` should use `interpolate_hw` for integer-valued strokes (quantisation noise about -78 dB, hardware parity).
4. Check the `B`-write rule in SIMD mode on the ADI simulator (method of [sharc-float](sharc-float.md)).

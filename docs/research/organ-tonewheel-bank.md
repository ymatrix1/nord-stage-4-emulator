# Research: the PNO Core1 wavetable oscillator bank (tonewheel candidate), oracle run and ARM tables

> **Correction (2026-11-12):** the bank algorithm below is VERIFIED, but its ROLE as the B3 tonewheel generator is ASSUMED and now judged unlikely (about 35 %). Newer evidence ([pno-module-chain](pno-module-chain.md), branch `research/pno-module-chain`): the bank is installed by the PNO Core1 piano state machine (state 1 generators: `0x122620` bank, `0x12272D` post stage, `0x122A71` LFO delay) and by no ORG/Core2 module id; the detuned pair per pitch fits piano string resonance (ARM `Edt_Sys_kStringResonanceLevel`) at about 65 %. The organ tone source is still unlocated; it would be settled by the ARM writer of the bank weights, the piano-versus-organ program selector, or a running boot capture. The Implications and To-do sections below were written under the tonewheel reading; `ToneWheelSource::Firmware` is now opt-in and `Legacy` is the default.

- **Checked:** 2026-11-12
- **Sources (local, never committed):** the PNO image of OS v1.66 (`python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR`, then `tools/ns4_dsp_ldr.py DIR/dsp_image_1_PNO.bin --sram SRAM`, file `SRAM/core1_sram.bin`; data word at DM address `A` is the u32 at byte `4 * A`, pc `P` is byte `2 * P`), `target/fw_unpacked/bank0_CODE.bin` (ARM, loads at `0xC0000000`), `sharc-bank` (new, `crates/nord-sharc/src/bin/sharc_bank.rs`), `sharc-disasm`, `tools/arm_xref.py` plus `capstone` for the ARM listings. Builds on [organ-tonegen](https://github.com/ymatrix1/nord-stage-4-emulator/pull/64) (`docs/research/organ-tonegen.md` on `origin/research/organ-tonegen-hunt`), [pno-dsp-map](pno-dsp-map.md), [arm-dsp-protocol](arm-dsp-protocol.md).
- **Question:** What exactly does the oscillator bank at pc `0x122620` compute (phase, index, table, mixing), which numbers does the ARM feed it (increments, weights, rate factor), and does it behave like the B3 tonewheel generator?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command in this note, **EMPIRICAL** = structure or fit, **ASSUMED** = reading of a purpose.

> **Update (2026-11-13):** the mix stage was driven with the real record layout in [organ-oracle-audio](organ-oracle-audio.md): all **92** weights (elements 0..183) are used (the "91 pairs, 92nd spare" reading of s5 came from a synthetic record), the weights/gains/level layout is decoded and a Python model reproduces the oracle chain to 1.8e-5. The x table coefficients are exactly `-10^(dB/20)` for -19, -36, -44, -48 dB.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Algorithm of the bank | `phase[j] += fracmul(inc[j], tune)`; `idx = phase >> 20`; `x[j] = T[2 idx]`, `y[j] = T[2 idx + 1]`, for **184 elements** per frame (92 SIMD steps x 2 lanes), 2 frames per call. 40 oracle calls (29,440 words, random signed odd 32-bit increments, `tune = 0x3FA02A00`, increments changed between calls) match the Python model **word for word**. | VERIFIED |
| Lane assignment | Element `j = 2 w + c`: wheel `w` (0..91), copy `c` (0 = PEx, 1 = PEy). The output arrays are element ordered. The copies of one wheel differ by a fixed tuning offset (section 4), so lanes are **not** Layer A / Layer B: they are two slightly detuned copies of the same 92 pitches. | VERIFIED (order), reading ASSUMED |
| Are there note-on increments? | **No.** The ARM writes the 184 increments once, from a constant table (`bank0_CODE@0x443EA8`), when the engine part is enabled. The pitches are fixed: 92 chromatic semitones, 12-TET within 1.03 cents rms, each in two copies spread 1.89 cents rms. Notes act only through weights (not decoded). | VERIFIED |
| Waveforms | x: `sin t - 0.1122 cos 2t - 0.01585 cos 3t - 0.00631 cos 4t - 0.00398 cos 5t`, measured through the oracle on a running bank: harmonics `0, -19.0, -36.0, -44.0, -48.0` dB. y: 10 harmonics `0, -6.2, -13.6, -21.4, -24.8, -30.0, -33.3, -36.2, -38.7, -40.4` dB. | VERIFIED |
| Global rate factor | `DM(0xC27C5)`, default `0x40000000` (0.5). ARM function `0xC022A8AC` writes `table[i] << 8` from `bank0_CODE@0xC027D9D8` with `i = r1 * 64 / 100 + DM(0xC05A0464) + 0x1F3`: table step 0.78125 cent (1/128 semitone), index `0x200` = `0x400000` exactly, index 0 = -400 cent, index 1000 = +381 cent, `0x1F3` = -10.16 cent. | VERIFIED (table), units of `r1` ASSUMED |
| Mix stage (`0x12268F`) and post stage (`0x12272D`) | Structure only: per-element blend of x and y with two PM weight vectors, a per-element weight vector in DM, two accumulators per lane, a short recursive section, output two words per frame. **Not** reproduced: with a synthetic record the output is zero or depends on words after the arrays, so the real record layout is unknown. | EMPIRICAL |
| Role: B3 tonewheel generator | Evidence for: free-running fixed-pitch oscillators on a semitone ladder, near-sine wave with -19/-36/-44/-48 dB harmonics, 91 used elements per lane. Evidence against or open: no drawbar/key names in the DSP; the bank sits in the **piano** core image; an equally good reading is a string-resonance bank (unison pairs detuned by about 2 cent). The ARM has both `Edt_Sys_kToneWheelMode` and `Edt_Sys_kStringResonanceLevel` (`0xC02D57F0`, `0xC02D57D0`). | ASSUMED/disputed (superseded: about 35 % tonewheels, 65 % string resonance; was about 60 %) |

## 2. The oracle run (VERIFIED)

`sharc-bank` boots the PNO section with the documented start-up (`--entry 0x121007`, 1.5 M steps, device presets of [pno-dsp-map](pno-dsp-map.md) 3) to the idle loop, then calls the routine at `0x122620` N times, with data pokes between the calls, and writes the two output arrays (2 x 368 words) per call. After the boot, `MODE1 = 0x1E054F8` (observed: PEYEN and the broadcast bits `0xE00000` are already on after the boot, as the pass of [pno-dsp-map](pno-dsp-map.md) 3 step 5 sets them), `M1 = M9 = 1`, `M2 = M10 = 2`, `M0 = 0`. The harness does not change `core/`; it ORs a `--mode1` value (default `0x200000`) and accepts `--set`.

```bash
cargo build --release -p nord-sharc --bins
python tools/ns4_dsp_ldr.py DIR/dsp_image_1_PNO.bin --sram SRAM
python tools/organ_bank_ref.py compare  SRAM/core1_sram.bin --exe target/release/sharc-bank --arm target/fw_unpacked/bank0_CODE.bin --calls 40
# 40 calls, 29440 words compared, mismatches: 0
python tools/organ_bank_ref.py spectrum SRAM/core1_sram.bin --exe target/release/sharc-bank --arm target/fw_unpacked/bank0_CODE.bin
# element 0: 36.37 Hz @48 kHz, harmonics dB: [0. -19.01 -36. -44. -48.]   (elements 40, 90 the same)
python tools/organ_bank_ref.py chord    SRAM/core1_sram.bin --exe target/release/sharc-bank --arm target/fw_unpacked/bank0_CODE.bin
# wheel 34: weight 1.000, lane0 amp 0.998, lane1 amp 0.989 ; wheel 41 (sum of two): weight 2.000, 1.947 / 2.019
```

`compare` first writes the ARM default increments, then replaces them with random 32-bit words (any sign, odd) before call 1 and sets `DM(0xC27C5) = 0x3FA02A00`; the model must reproduce `fracmul` rounding for odd words (SSFR round to nearest) and the signed wrap of the phase. `chord` takes 16,384 oracle frames of the default increments, forms the weighted sum `sum_w weight[w] * x[2w + c]` per lane in Python (drawbar-like 3 dB steps, 8-8-8-0-0-0-0-0-0 on a C-E-G triad, wheel offsets `-12, 7, 0, 12, 19, 24, 28, 31, 36` from the played wheel) and measures each expected fundamental: amplitudes equal the weights within 3 % (the leakage of the other tones in a 16,384-point Hann window). That shows the sum of the oracle waveforms behaves as the sum of the expected harmonics; it does **not** verify the firmware's own mixer.

### 2.1 Algorithm (per call: two frames of 184 elements)

| Step | Equation | Evidence |
|---|---|---|
| State | `phase[j]`, u32, DM `0xC242D + j`, initial 0 | oracle: after 1 call with `inc[j] = (j+1) << 20` the words read `(j+1) << 20` |
| Rate | `step = fracmul(inc[j], tune)` = `inc * tune / 2^31` rounded to nearest, signed; `tune = DM(0xC27C5)` | `R1 = PM(I12); R1 = R1 * R4 (SSFR)` at `0x122643`; model equal on 29,440 words |
| Accumulate | `phase = (phase + step) mod 2^32`, **before** the table read (first sample already advanced) | oracle idx sequence `0, 1, 1, 2, 2, 3` for `inc[j] = (j+1) << 20` |
| Index | `idx = phase >> 20` (`FEXT` 12 bits at bit 20), no interpolation, no rounding | model equal |
| Table | `T` = 4096 (x, y) float pairs at DM `0xC042B`; `x = T[2 idx]`, `y = T[2 idx + 1]` | model equal |
| Output | x array `0xC24E5`, y array `0xC2655`, element order, frame stride `0xB8` = 184 words, 368 words per call | oracle |
| Cost | 1,513 instructions per call (SIMD) | `sharc-bank` |

Frequency of element `j` (frame rate `fs`): `f = fs * inc_value * tune / 2^31 / 2^32` with `inc = v << 8`; at the default `tune = 0.5`, `f = fs * v / 2^25`. The frame rate is not known from the image (open, section 6).

## 3. ARM side (VERIFIED code, roles ASSUMED)

| Item | Result |
|---|---|
| Setter class | Vtable words at `0xC05266D4..0xC05266F0`: `0xC022A8A4` (returns 92), `0xC01D0CC8`, `0xC022A8A4`, `0xC022AA48` (group 3), `0xC022A9A8` (group 2, called as slot `+0x10` of the object whose vtable pointer is `0xC05266D4`), `0xC022A87C` (array pointers `0xC24E5` / `0xC2655` for ids 4 / 5), `0xC022A594`, `0xC022A454`. Call convention `(this, r1, group r2, index r3, value [sp])`. |
| Increments (group 2) | `0xC022A9A8`: `DM(0x955B6 + index) = value << 8` through the DSP window (`0x28000000 + 4 * word`, the object at `0xC055B150`, port `+0x44`). Index 0..183. |
| Writer of the 184 values | Loops at `0xC022DC5C..0xC022DCFC` (fast path inlined: `0x282556DC` = window of `DM(0x955B7)`) and `0xC022DB38..0xC022DB74` (virtual call), both reading **`bank0_CODE@0x443EA8` (184 u32)**, index `r8 = 0..183`. They sit in the function that also writes the constants `0x600000` / `0x300000` to groups 5..8 (an enable/disable routine; the else branch writes zeros). No other increment writer was found in that function. |
| Rate factor (group 3) | `0xC022AA48`: `DM(0x28309F14 >> 2 = 0xC27C5) = r3`. `0xC022A8AC` is the caller that maps a value to the tune table (section 1). |
| Array families (`0xC022A594`) | selector `r3` (0..9), set `r2` (0..3), DSP base: sel 1: `0x957DE + 0x5C r2` (four arrays of 92 = 368 words); sel 3: `0x9594F + 0x65 r2`; sel 4: `0x95AE2 + 0x64 r2`; sel 0: `0x9594E + 0x65 r2` (scalar); sels 5..9 are scalars next to them (`0x959AF`, `0x959B0`, `0x95B3F`, `0x95B3E`, `0x95B42`, ...). |
| Memory picture (EMPIRICAL) | `0x955B6` increments (184), then `0x9566E` and `0x95726` (two 184-word blocks = the I10/I11 weight vectors of the mix stage), then `0x957DE` (368 words = 2 frames x 184 = the per-element weight vector of the mix stage). The lengths agree exactly; the addresses of the consumer are not confirmed (section 5). |

`python tools/organ_bank_ref.py defaults target/fw_unpacked/bank0_CODE.bin` prints the fit below.

## 4. The default increments are a fixed semitone ladder (VERIFIED)

| Fact | Value |
|---|---|
| Layout | `v[2 k]`, `v[2 k + 1]` = copies 0 and 1 of semitone `k`, `k = 0..91` |
| First values | `0x6350, 0x633E, 0x6919, 0x6957, 0x6F77, 0x6F7F, ...` |
| 12-TET fit | slope 1.00000 semitone per note, `v0 = 25430.2`, residual of the pair mean 1.03 cent rms |
| Copy spread | `1200 log2(v[2k+1] / v[2k])`: mean -0.16 cent, 1.89 cent rms, sign varies |
| Octave check | `v[96] = 0x633E0 = 16 * 0x633E` (+- jitter): 4 octaves in 48 notes |
| At 48 kHz, tune 0.5 | element 0 = 36.37 Hz (D1 is 36.71 Hz, -16 cent); element 90 = 489 Hz |

Hammond gear ratios are systematic (see TD-04), these are random +-2 cent offsets: they do not look like the 196-tooth tuning. Readings, all ASSUMED: (a) per-copy detune for stereo decorrelation; (b) unison-string offsets (piano). The absolute pitch needs the frame rate and `DM(0xC27C5)` at run time; `0xC05A0464` is BSS, so the default tune index (unity `0x200`, or `0x1F3` = -10 cent) is not readable from the file.

## 5. Mix and post stage (EMPIRICAL; to finish)

Routine `0x12268F` (`sharc-disasm SRAM/core1_sram.bin 0x244D1E ...`). Entry: `I1` points into a record whose words `-0x20 .. -0x1D` hold `I2, I3, I10, I11`; they are copied to `DM(0xC27C7, 0xC27C9, 0xC27CB, 0xC27CD)` and reloaded for each of 2 frames; `DM(I2 - 1)` is a pointer `I6` that advances by `0xB8` per frame. Per frame:

1. 45 SIMD iterations: `blend = DM(I2) * PM(I11) + DM(I3) * PM(I10)` per element pair (x and y with two weight vectors; the result is stored back into the `I10` block).
2. 45 iterations of products and sums over the 91 used pairs with the DM vector at `I6` (two accumulators per lane, summed with `F0 = F8 + F9`).
3. a short section with six multiplies from words that follow the arrays (smoother or tilt), `I6 += 0xB8`, and `DM(I0, M2) = R0`: two words (PEx, PEy) per frame at the output pointer.

Oracle tests with a synthetic record (`x`, `y`, both weight blocks and the DM vector random in -1..1): the two output words change with x, y and all weights of elements 0..181, **not** with elements 182, 183 (91 pairs are used, the 92nd is a spare), the second half (words 184..367) of the DM vector changes only the second frame's output word pair, while x words of frame 1 change nothing, and a one-hot input gives exactly 0 when the words after the arrays are 0. The routine therefore reads additional coefficients behind the arrays, which means the real `I2/I3` arrays are not the contiguous 184-word frames of the bank. The record layout the ARM builds (module chain of [pno-dsp-map](pno-dsp-map.md) 6) is unknown, so this stage cannot be driven faithfully yet. The ARM has no constant `0x122620` or `0x12268F` anywhere in `bank0_CODE` (searched as u32, shifted forms and byte addresses), so the routine pointers are not stored as such.

Routine `0x12272D`: triangle LFO (`|phase| - 0.5`, `*4`, clip), `fix`, interpolated lookups in 65-point tables, 19-tap delay loops ([organ-tonegen](https://github.com/ymatrix1/nord-stage-4-emulator/pull/64) section 3): not run here.

## 6. Still ASSUMED or open

| Item | State |
|---|---|
| Role of the bank (B3 tonewheels vs string resonance vs both) | ASSUMED; decisive check: which ARM parameter ids drive the `0x957DE` weights (`Edt_Sys_kToneWheelMode`, `Edt_Sys_kStringResonanceLevel`, `Buffer_kOrgDrawbar` at `0xC02D6938`) |
| Frame rate of the bank, absolute pitch, tune index at run time | open (48 kHz gives D1 -16 cent; unity tune gives A = 436.4 Hz at element 43) |
| Drawbar to weight mapping (curve, tapering, foldback), key contacts | open: the weights are written by the ARM into `0x9566E..0x95949`; senders not decoded (see the `research/arm-param-messages` work, not on `main` when this was written) |
| Percussion, key click, key-on envelope | not in the bank (it has no per-note state); either in the weights (ARM ramps) or the ORG strip image |
| Meaning of selectors 0..9 of `0xC022A594` | only the DSP bases are decoded |
| Copy 0 / copy 1 meaning | detune pair, reading ASSUMED |
| 9-entry rows at `0xC05267F8` (cumulative, 0..~750) and the `0x7F .. 0x20` u16 table at `0xC05266F4` | not interpreted (candidates for a drawbar curve, no xref found) |

## Implications

- **No per-note oscillators.** If `organ.rs` is to mirror this design: 92 fixed-pitch accumulators per channel (two copies with the offsets of section 4), one shared 4096-point x table (and the y table for the second voicing), a weight vector per channel; key presses only change weights.
- **Wave:** use the firmware x wave for every wheel (VERIFIED table, role ASSUMED); the y wave is the candidate for the other tonewheel mode (`Edt_Sys_kToneWheelMode`).
- **Do not** read the 184 elements as Layer A / B.

## To-do for `crates/nord-core/src/organ/` (items 1 and 3 done: `firmware_bank.rs`, `docs/specs/organ-engine.md` 3.1.1; item 2 partly: two copies with a constant pair, no 92nd wheel, no per-copy weights)

1. Replace the ad hoc harmonics by the x table (formula in section 1; 4096 points, no interpolation needed for parity with the firmware; keep a linear-interpolated variant for quality).
2. Add a 92nd (spare) wheel and a second copy per wheel with the section 4 offsets (cent table from `tools/organ_bank_ref.py defaults`), driven by a mono weight vector per copy.
3. Keep `phase` as `u32`, step `= round(f / fs * 2^32)`, index `phase >> 20`, to match the firmware numerically.
4. Tag the tuning `EMPIRICAL`/`ASSUMED` until the frame rate and tune index are known; mirror the status in `docs/specs/organ-engine.md` 3.1 when `organ.rs` is changed.

## Follow-ups

1. Decode the ARM senders of the weight blocks (`0x9566E`, `0x95726`, `0x957DE`, selector 1 of `0xC022A594`) and the module-chain record install; then run `0x12268F` faithfully in `sharc-bank` (`--call 0x12268F --ret 0x12272A` already works).
2. Find the bank's frame rate (the call rate of `0x122620` per 120-sample block) by tracing the module chain.
3. Decide the role with the ARM parameter-id tables.

# Synth voice control: where the oscillators, filters and envelopes run, and what drives them

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66, ARM mode, base `0xC0000000`); the settled ARM snapshot of `tools/arm_steady.py`; the flat SRAM images of ORG core 2 and SY1SY2 (`tools/ns4_dsp_ldr.py --sram`, `tools/ns4_dsp_raw.py --sram`); ADI `elfdump` listing of ORG core 2 (`tools/adi_dis.py`, CCES 2.12.1); `sharc-run`, `sharc-ref` (`crates/nord-sharc`); [arm-organ-messages](arm-organ-messages.md) s10, [pno-module-chain](pno-module-chain.md) s5, [org-stage-map](org-stage-map.md), [dsp-rx-protocol](dsp-rx-protocol.md), [sy-dsp-map](sy-dsp-map.md), [sy-smp-audio](sy-smp-audio.md).
- **Question:** The SY1SY2 image holds no oscillator, filter or envelope ([sy-dsp-map](sy-dsp-map.md) s1). Which DSP code computes them, how does the ARM drive a synth layer, and what does `crates/nord-core/src/synth/` need to match it?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = shape of code or data, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Where do the synth edit ids go? | **Not to SY1SY2.** Every `Edt_Syn_*` id writes **channel-strip words** `DM(0xE0000 + 62 k + w)` and a 32-word per-layer common block `DM(0xE02E8 + 32 L + w)`, sent identically to **ORG core 1, ORG core 2 and PNO core 2** (path 2). Layer A uses strips 0 and 6, B strips 2 and 8, C strips 4 and 10, common blocks 0, 1, 2. The only SY words touched are 56 pair-block words `DM(0x921A6 + 0x16 p + {0, 1})`, `p = 0..27`, by the two tune ids (section 3). | VERIFIED (run, `data/synth-edit-messages.csv`) |
| Which DSP code is the synth voice? | The **Core2 stage engine** that ORG core 2 and PNO core 2 share byte for byte ([pno-module-chain](pno-module-chain.md) s5): strip word 0 selects an oscillator-side stage, strip word 1 a second-stage module. A synth strip carries word 0 = `0x061215FB` (stage `0x1215FB`) and word 1 = `0x0612566E` (stage `0x12566E`). | VERIFIED (messages), EMPIRICAL (roles) |
| Oscillator | The helper `0x12501F` that stage `0x1215FB` calls is a **band-limited falling sawtooth**: four samples per call, quadratic PolyBLEP step two increments wide, linear fade to silence between 0.1152 and 0.2304 cycles per sample. Closed form matches the oracle to 5.9e-8 (float32 rounding) over 512 phase/increment pairs. The same three constants occur at 30 sites of the shared Core2 code (28 inline in stage routines, the helpers `0x12501F` and `0x1250C9`), the routines [pno-module-chain](pno-module-chain.md) s5 counted as a "filter family" (29 of 56 word-0 stages): the saw shaping is their common core, so read them as **oscillator waveform stages**. | VERIFIED (oracle, section 5) |
| Filter | Word-1 module `0x12566E` (and its siblings `0x1257B2`, `0x125913`, `0x125A50`, 130..153 instructions each) is a **clipped, trapezoidal multi-pole ladder** with a table-driven coefficient pair (`DM(0xB0D0F)`, `DM(0xB1015)`, 8-bit index plus 24-bit fraction, linear interpolation) and `RSQRTS` normalisation. Shape read from the listing; the equations are not extracted. | EMPIRICAL |
| Envelopes | **Not found on any DSP.** The strip words that the ADR ids write (`w26..w34`, `DM(0xE001A..0xE0022)`) hold constants (`-2.0`) in the idle state and the stage code that reads them was not located. The ARM has no FPU and holds integer rate tables (section 6), so the generator is probably ARM-side at 1 kHz with a one-pole smoother on the DSP (section 4); not proven. | open |
| Gain smoothing in SY | SY smooths each voice gain with `state <- target + 0.979 (state - target)` per output frame (time constant 47.1 frames = 0.98 ms at 48 kHz); the ARM writes only the target. | VERIFIED (oracle, section 4) |
| Key down / key up, held-key series | **Not driven.** No MIDI byte path and no layer event call reached the voice engine in the harness (section 7); no time series of an attack, decay or release was captured. | open |

## 2. How the ids were swept

`arm_edit_capture.py` (already in the repo) drives every value of an id through the real synth layer client of the booted firmware (dispatcher clients 4, 5, 6 = synth A, B, C) and logs the ARM to DSP words:

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
uv run --with unicorn --with capstone python tools/arm_edit_capture.py \
    target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
    --cache target/settled.snap --filter "Edt_Syn" --layers 4 --csv syn_l4.csv      # about 9 minutes
python tools/synth_strips.py syn_l4.csv --out docs/research/data/synth-edit-messages.csv
```

Layer A: all 95 `Edt_Syn*`/`Edt_SynCom*` ids (6321 messages); layers B and C: the ids `kFltFreq, kFltRes, kFltType, kAmpEnvAtk, kOscWave, kOscCtrl, kGlideRate, kVoiceMode` only (1088 messages). Result table: [data/synth-edit-messages.csv](data/synth-edit-messages.csv) (one row per layer, id and DM word; value dependence and up to five `value:word` samples).

## 3. Destinations (VERIFIED run)

| Item | Result |
|---|---|
| Receivers | The same word set goes to ORG core 1, ORG core 2 and PNO core 2 (`chips` column `ORG1 ORG2 PNO2`) and for a few ids (mode, wave, sample id, vibrato, LFO clock) also to PNO core 1 (`DM(0xB00E6)`, `0xB00F2`, `0xB00F8`, `0xB00FE`, `0xC13A9..AC`, `0xC1B3C..46`; mostly constants in the idle state). Nothing goes to FX or SMP. |
| Strips per layer | Layer A strips 0 and 6, layer B strips 2 and 8, layer C strips 4 and 10 (checked for `kAmpEnvAtk` word 26 in strips 0/2/4, `kFltFreq` word 3 in strips 6/8/10). Strips 1, 3, 5, 7, 9, 11 are not written by these ids (ASSUMED partner strips). |
| Common block | `DM(0xE02E8 + 32 L + w)`; layer A: w1 cutoff-related (`0xCC000000`, constant), w6 osc control, w7 resonance, w10 amp decay, w11 filter decay, w12 osc-env amount, w13 filter-env amount. |
| SY | Only `kOscTuneFine` / `kOscTuneCoarse` write SY: 56 words `DM(0x921A6 + 0x16 p + {0, 1}) = 0x3D4CCCCD` (0.05), `p = 0..27`, independent of the value (image default there: `0x3D4CCCCC`). These are the words `b[20], b[21]` of the 22-word pair blocks of the ISR step 3 (section 4; `0x921A6` is the prefetched word `DM(-24, I7)`); role of the 0.05 not isolated (a second smoother coefficient, ASSUMED). |
| Value dependence | Only the common-block words `w6` (osc control) and `w7` (resonance) and the amount words `w12`, `w13` change with the value: `word = -0.5 + v / 127 * 2^-16` (v = 0: `0xBF000000`; v = 127: `0xBEFFFE00`; 7-bit fit error one float32 ulp), `kFltEnvAmt` / `kOscEnvAmt` give `0 .. +-1.5259e-05 = 2^-16`. Every other id (ADR times, cutoff, glide, filter type, wave, zone) writes the **same word for every value** in the idle state. The firmware therefore sends the knob only through a state that needs an active voice or a loaded preset: the same effect as the zero drawbar words of [arm-organ-messages](arm-organ-messages.md) s10.3. |

### Strip 0 words written by the synth ids (layer A, VERIFIED run)

| Word | DM | Ids | Idle value | Reading |
|---|---|---|---|---|
| 0 | `0xE0000` | `kMode, kSmpId, kOscBank, kOscCat, kOscWave, kOscUnison, kOscEnvDst, kOscEnvVel, kSmpDynamic, kSmpTimbre` | `0x061215FB` | `((v & 0x7F) << 24) | 0x1215FB` ([pno-module-chain](pno-module-chain.md) s5): oscillator stage; the top byte `6` is not decoded |
| 1 | `0xE0001` | `kFltType, kFltDrive, kFltEnable, kMode` | `0x0612566E` | filter-side stage `0x12566E` |
| 19 | `0xE0013` | `kFltEnvAmt, kFltEnvVel` | `1.0` | |
| 20, 22, 24 | `0xE0014/16/18` | `kOscEnvAmt` | `1.0, 0.0, -1.0` | |
| 26, 27, 28 | `0xE001A..1C` | `kAmpEnvAtk / Dcy / Rls` | `-2.0` | amp ADR |
| 29, 30, 31 | `0xE001D..1F` | `kFltEnvAtk / Dcy / Rls` | `-2.0` | filter ADR |
| 32, 33, 34 | `0xE0020..22` | `kOscEnvAtk / Dcy / Rls` | `-2.0` | osc (pitch) ADR |
| 36, 57, 58 | `0xE0024, 39, 3A` | `kVoiceMode` | `0, -0.0, -0.0` | |
| 38 | `0xE0026` | `kFltKbdTrk` | `0.0` | key tracking |
| 48..51 | `0xE0030..33` | `kKbZone, kOctaveShift, kAuxKbdControl` | `-39.5, 1.0, 48.5, -1.0` (octave shift varies the first and third word) | key zone / transpose |
| 53 | `0xE0035` | `kAmpEnvVel` | `1.0` | velocity amount |
| 55 | `0xE0037` | `kGlideRate` | `-2.0` | glide |
| 59, 60 | `0xE003B/3C` | `kMode, kSmpId, kVib*, kOsc*` | `0x02000000, 0` | |

## 4. SY1SY2: the voice gain smoother (VERIFIED, oracle)

The ISR step 4 loop of [sy-smp-audio](sy-smp-audio.md) s3 (PC `0x1C06E8..0x1C070B`, 56 iterations) reads per voice `state = DM(rec+0x36)`, `target = DM(rec+0x34)` (record base `0x92400 + 0x38 k`, `I3 = base + 0x26`), the pair-block words `b[10]` (pole) and `b[8]` (scale), and writes `DM(rec+0x36) = target + c (state - target)` and `gainL = DM(rec+0x08) = state'`. The pair block of voice pair `p` ends at `0x921BE + 0x16 p` (`I1`), so `b[10] = DM(-12, I1)`.

Measured with `tools/oracle_scenarios/sy_voice_sine.scn` plus `poke 0x92434 0x3f000000` (target 0.5, initial state 1.0): 80 frames, the ratio `(state_n - 0.5) / (state_{n-1} - 0.5)` is 0.9789994 .. 0.9790003 (mean 0.97899981); the image constant `b[10] = 0x3F7A9FBE = 0.97899997`. Series: [data/synth-sy-gain-slew.csv](data/synth-sy-gain-slew.csv).

| Quantity | Value | Tag |
|---|---|---|
| Law | `s' = t + 0.979 (s - t)`, once per audio ISR (one output frame) | VERIFIED |
| Time constant | `-1 / ln 0.979 = 47.1` frames = 0.98 ms at 48 kHz (ASSUMED rate) | VERIFIED (frames), ASSUMED (kHz) |
| Initial block (image) | `b[0..3] = 0` (`b[0]` is the pitch ratio that becomes `step = FIX(b[0] 2^31)`, zero until the ARM fills the pitch table), `b[8]` the gain scale that step 3 rewrites every ISR (it is 1.0 in the scenario: `gainL = state`), `b[10] = 0.979`, `b[12,13] = 0x04000036` (pitch-table index pair), `b[14,15] = 1.0`, `b[16..19] = 0.0046346 (0x3B98676A)`, `b[20,21] = 0.05 (0x3D4CCCCC)`; the ARM overwrites `b[-24] = 0.05 (0x3D4CCCCD)` when a tune id is applied | VERIFIED (image) |
| Reading | A 1 ms de-zipper under the ARM's 1 kHz control tick ([arm-organ-messages](arm-organ-messages.md) s10.1: timer irq 22, 1 ms). The ARM streams `target`, the DSP smooths. Same smoother class for `step` (pitch) is ASSUMED; the code path that writes `rec+0x2D` was read, its smoothing law not isolated | EMPIRICAL |

## 5. The synth oscillator helper `0x12501F` (VERIFIED, oracle)

Stage `0x1215FB` (65 instructions, `0x1215FB..0x12169C`) loads its parameter block through `I6 = DM(-32, I2)`, calls `0x12501F` with `R12 = increment` (Q32 cycles), `R8 = phase`, `R10 = 0`, `R6 = 1.0`, and sums the result of two calls (a second oscillator or a shifted saw; the second call's arguments were not decoded). The helper (listing `0x12501F..0x1250B0`, 75 instructions, straight line) returns four samples in `R0..R3` and the advanced phase in `R4`.

Executed in `sharc-run` (entry `0x12501F`, 74 steps, strict) for 512 phase/increment pairs and compared with `tools/synth_saw_model.py`: worst difference **5.9e-8**.

```text
p_k = frac(phase + k inc),  k = 1..4            (inc = increment / 2^32 cycles per sample)
t_k = p_k / inc  (p_k < 0.5)   or   (p_k - 1) / inc  (p_k >= 0.5)       distance to the wrap in increments
d(t) = -(1 - t/2)^2 for 0 <= t < 2;   +(1 + t/2)^2 for -2 < t < 0;   0 otherwise
y_k = g(inc) * amp * (1 - 2 p_k + d(t_k))
g(inc) = C (B + clip(A - 2 inc, +-B)),   A = 0x3EB0F34D = 0.3456062,  B = 0x3DEBEF11 = 0.1152021,  C = 0x408AE2EA = 4.340199
```

| Property | Value |
|---|---|
| Waveform | falling saw, `y = 0` exactly on the wrap sample, `d(1) = -0.25`, `d(-1) = +0.25` |
| Step width | two increments (a PolyBLEP of the textbook one-increment form with `t -> t/2`), the same shape at every increment up to the fade (checked at 1/256, 1/32, 0.125, 0.2: ratio to the model constant over t); the fade `g` multiplies the whole sample |
| Fade | `g = 1` for `inc <= (A - B)/2 = 0.1152` cycles per sample (5.53 kHz at 48 kHz), linear to `0` at `(A + B)/2 = 0.2304` (11.06 kHz); `2 B C = 1` |
| Measured fade | inc = 0.125: 0.91495; 0.15: 0.69794; 0.20: 0.26392; 0.25: 0.0 (model: same to 5 digits) |
| Block size | four samples per call; sample `k` is evaluated at `phase + k inc` (the returned phase is `phase + 4 inc`) |
| Other stages | the prologue (`float r12 by -31`, the three constants, `RECIPS`) appears at 28 inline sites (ORG core 2 listing offsets, pc = offset + `0x120300`: `0xAC7, 0xC6D, 0xE9D, 0xFF1, 0x117C, 0x15BC, 0x16DF, 0x199B, 0x1A7A, 0x1BAA, 0x212D, 0x216E, 0x22EC, 0x241C, 0x25CB, 0x2772, 0x29FA, 0x2A43, 0x3495, 0x3720, 0x3780, 0x39EB, 0x3A1A, 0x3A7D, 0x3D50, 0x3DB0, 0x44DB, 0x4699`) and in the two helpers (`0x4D1F` = `0x12501F`, `0x4DC9` = `0x1250C9`); `call 0x12501F` occurs 9 times (`0x121618, 0x121651, 0x1216CC, 0x1216E3, 0x12172B, 0x121742, 0x1217EA, 0x121A45, 0x1250B1`). Only `0x12501F` was executed; the inline sites are the same text, not run |

Comparison with `docs/specs/synth-engine.md` s3: the spec's PolyBLEP is one increment wide and rising; the firmware's is two increments wide, falling, with the high-pitch fade.

## 6. ARM side (EMPIRICAL, read from code and the settled snapshot)

| Item | Result | Tag |
|---|---|---|
| Voice objects | 56 objects of `0xB0` bytes, list `0xC055D764` (count `0x38`, objects `0x90670230 + 0xB0 k`, voice id at `+0`), the same count as the SY voice records | VERIFIED (dump) |
| Event path | Note events are 32-byte records in per-type rings (`0xC05613AC`, 14 types) handled by `0xC01E00AC(this, msg)`, a 2 kB routine that applies velocity and key-tracking tables, ends in the SY record writes (`prepare` `0xC01E1198/193C`, `go` `0xC01E2C2C/2F54` of [sy-smp-audio](sy-smp-audio.md) s3) and schedules with the 64-bit tick (`+2400 ticks`) | EMPIRICAL |
| Voice engine objects | six objects (`0xC05A0390 + 4 i`, `i = 1..6`, vtable `0xC02FDB78`, 0x5A70 bytes apart); slot 2 `0xC01EC690(this, hdr, records, kind)` posts event records (kind 0..7 selects flag word `8, 1, 2, 0, 4, 0, 0, 7`) | EMPIRICAL |
| No FPU | ARM926EJ-S: all ARM-side level and rate maths is integer. The 128-entry Q31 table at `0xC02FB054` (`0x7FFFFFFF`, then `0x0144D65D` falling by a factor 0.9654 to 0.9661 per step to `0x0004238A`, 37.9 dB over 127 steps, steps of about 0.30 dB) is indexed by a 7-bit level `clip((base + vel * depth >> 7 + 0x8000) >> 16, 0, 127)` in `0xC01E00AC` and stored clipped to `0x10000000` at voice `+0x70` | VERIFIED (table, index maths), ASSUMED (role: velocity / level to gain) |
| Setter functions | `kAmpEnvAtk`, `kGlideRate` call the generic float sender `0xC0062D60` from the layer apply function (`0xC00192FC`, `0xC001911C`); `kFltRes` and `kOscCtrl` call `0xC0062D28` (`0xC0018FDC`); `kFltFreq` writes two words (`0xC01C5A48`, `0xC01C5ABC`). The 74-id stage table (strip word 0) is `0xC02FA050` ([pno-module-chain](pno-module-chain.md) s5) | VERIFIED (run stack) |

## 7. What was tried and did not work

| Attempt | Outcome |
|---|---|
| MIDI note on written to the UART2 receive ring (`0x90165770`, buffer `0x90165780`, size `0x1000`, head `+8`) for channels 1..4, then 12 ticks | 0 messages: nothing reads the ring in the idle main loop (read watch on the ring over 60 M instructions: 0 reads). The MIDI consumer is not polled; it needs the receive notification the UART handler `0x80000434` does not post in this state |
| `0xC01EC690(engine, hdr, record {0, 60, 100, 0...}, kind)` for the six engines, kinds 1, 2, 4 | one message during the call for kind 2 only, nothing in the following 6 M instructions: the records are posted but the layer / voice allocation state of a loaded preset is missing |
| Running the stages inside the ORG core 2 pass (`sharc-org`) | The pass calls the six strip stages through records `DM(0xC13A6 + 0x28 s)` (`I8 = DM(0,I7)`, call at `0x127008..0x1270C7`; default stage `0x12538C`; the ARM words `DM(0xC13A9)` and `DM(0xC13AC)` of the sweep are fields +3 and +6 of the record of strip 0). `--poke 1:0xC13A6=0x1215fb` (and `0x12566E`, `0x120CD0`) installs the stage in strict mode without a fault (pass cost +205, +126, +151 instructions), but the frame behind `I2 = DM(0xB2EF8)` is not initialised in the harness, so the stage writes `0xFFFFFFFF` into its buffers; no usable output |
| Reading values from the sweep | idle state (section 3, value dependence) |

## 8. Rust parity to-do (`crates/nord-core/src/synth/`, not edited here)

1. **VA saw** (done, opt-in `SawModel::Firmware`, spec s3.1): replace the one-increment PolyBLEP by the model of section 5 (`d(t)`, width two increments, falling orientation, `g(inc)` fade above 0.1152 cycles per sample). Test against the 512-point grid of `tools/synth_saw_model.py`. Pulse and triangle shapes: ASSUMED to be sums or shifted copies of the same helper (`R10` is a phase offset, `R6` an amplitude); verify the two-call stage `0x1215FB` and a pulse-width stage in the oracle before changing them.
2. **Voice gain smoothing** (done, opt-in `SynthLayer::gain_dezipper`, `synth/gain_dezip.rs`): one-pole `s' = t + 0.979 (s - t)` per output frame on the voice gain (0.98 ms at 48 kHz). Control updates at 1 kHz (timer irq 22, ASSUMED 1 ms).
3. **Filter placement**: the filter is a per-strip stage selected by strip word 1; the four siblings `0x12566E, 0x1257B2, 0x125913, 0x125A50` are the type variants (the ARM sends only `0x12566E` in the idle state; the selection by `Edt_Syn_kFltType` was not observed). Model after extracting the equations (section 9, item 1). Until then keep the ZDF ladder of the spec, but note the firmware **clips every stage** (`CLIP f10 BY f8` per pole) and normalises with `RSQRTS`.
4. **Coefficient tables** at ORG core 2 `DM(0xB0D0F)` and `DM(0xB1015)` (8-bit index, 24-bit fraction, linear interpolation): first families, 0..127: `A[i] = 1.0` for `i < 64`, `A[i] = 0.99 (128 - i) / 64` for `64 <= i < 128` (max error 2.1e-7); `B[i] = 0.70710678 (128 - i) / 64` for `38 <= i < 128` (max error 4.7e-7), `B[0] = 2.0` falling smoothly to `B[37] = 1.01086` (no closed form fitted); second families from 129: `A2[j] = 1.98 (128 - j) / 128` (max error 3.4e-7) and `B2[j]` rising from `0.0019885` (ratio per step falls from 1.0594 to 1.022) to `1.0` at `j = 128` with a linear last 24 steps of 0.02174 (`tan(u0 2^(j/12))` does not fit: 14 % off at `j = 96`). Role (damping, drive, cutoff prewarp) not assigned.
5. **Envelopes**: keep `ENV_*_MS_TABLE` (display strings, [synth-0x2f-filter-env-adr](synth-0x2f-filter-env-adr.md)); the DSP-side law is unknown. The only ARM-side integer rate table found (`0xC02FB054`, 0.30 dB per step) is not shown to be the envelope.
6. **Sample path** unchanged: Kaiser sinc resampler (`sy-dsp-map` TD-08); SY has no filter or envelope, so a sample synth voice is `resampler -> (strip filter) -> gain`.

## 9. Open points

1. Equations of the clipped ladder `0x12566E`: needs the stage frame (`I2`, `I3`, `I5` layout) so the stage runs in `sharc-org` with the strip engine; then impulse and sine sweeps against the cutoff index.
2. Strip word 0 top byte (`6`) and word 1 top byte, the type index `[this + 0xC8]` of the preset (which of the 74 stage ids a given `Edt_Syn_kOscWave` selects).
3. The ADR words: the stage reading `DM(0xE001A..0xE0022)` and the time law; an ARM key-down is needed to see the values the firmware really sends.
4. Key event dispatcher: either the layer event call with a note record after a preset is loaded, or the UART consumer; then record `DM(0xE0000..)` and SY `rec+0x34` series on key down, hold, key up and the first control ticks.
5. Where the second oscillator call of stage `0x1215FB` takes its arguments (unison detune, PWM).

## Implications

- The synth voice is not in the SY1SY2 image: the voice chain is SY (sample playback, resampler, 1 ms gain de-zipper) plus the Core2 strip engine (VA oscillators and the filter stage). A Rust synth that follows `docs/specs/synth-engine.md` as written has the right blocks but not the right oscillator.
- The VA saw of the firmware is fully specified (section 5) and can be implemented and tested without hardware.
- The "filter family" reading of the 29 word-0 stages in [pno-module-chain](pno-module-chain.md) s5 should be read as oscillator stages.

## Follow-ups

1. Section 9 items 1 and 4 (filter equations, key-down series).
2. Done as opt-in `SawModel::Firmware` (`synth/hw_saw.rs`, grid golden `tests_hw_saw_golden.rs`); making it the default needs the open points of spec section 3.1 (which stage the `Saw` wave selects).
3. Run `arm_edit_capture.py` again after a key-down harness exists, so the ADR words carry values.

## Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram SRAM_ORG        # core2_sram.bin
python tools/ns4_dsp_raw.py DSP/dsp_image_3_SY1SY2.bin --sram sy.bin
cargo build --release -p nord-sharc --bin sharc-run --bin sharc-ref
# oscillator helper, 512 points
python tools/synth_saw_model.py check SRAM_ORG/core2_sram.bin target/release/sharc-run
# one grid row by hand: phase 0, increment 2^24
target/release/sharc-run SRAM_ORG/core2_sram.bin 0x12501F 74 --set R12=0x01000000 --set R8=0 --set R10=0 --set R6=0x3f800000 --regs
# gain smoother: the sine scenario with a target poke and extra captures
sed -e 's/^loop 240/poke 0x92434 0x3f000000\nloop 80/' \
    -e 's/^capture gainR.*/capture gainR 0x92409 1\ncapture st 0x92436 1\ncapture tg 0x92434 1/' \
    tools/oracle_scenarios/sy_voice_sine.scn > slew.scn
target/release/sharc-ref sy.bin slew.scn --out slew
# ORG core 2 listing for the addresses above (offset = pc - 0x120300)
python tools/adi_dis.py SRAM_ORG/core2_sram.bin 0x240600 0x24F000 > org_c2.txt
python tools/sharc_routines.py org_c2.txt 0x120300 --min 40
```

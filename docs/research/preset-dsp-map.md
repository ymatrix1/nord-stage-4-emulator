# Research: preset parameters against the DSP strips, packed words and conversion tables

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (384 programs, 64 organ, 96 piano, 384 synth presets, read with `tools/ns4_param_inventory.py`); the flat SRAM images `sram_{0_FX,1_PNO,2_ORG}/core{1,2}_sram.bin` written by `python tools/ns4_dsp_ldr.py dsp_image_*.bin --sram DIR` (OS v1.66; DM word `A` at byte `4 A`, checked on the strips at `DM(0xE0000 + 62 k)`); [org-dsp-map](org-dsp-map.md), [org-stage-map](org-stage-map.md), [dsp-rx-protocol](dsp-rx-protocol.md), [sy-dsp-map](sy-dsp-map.md), [pno-dsp-map](pno-dsp-map.md); [cbin-and-presets spec](../specs/cbin-and-presets.md) section 4.1; the Rust parsers in `crates/nord-core/src/cbin/*_preset.rs`.
- **Question:** For each preset parameter of the organ (and as far as the format and the DSP images allow, piano and synth): which bit field holds it, what value range does it take, and which strip word, packed controller word or conversion table of the DSP side does it correspond to? Which closed forms do the DSP-side tables follow, and where do the curves used in `crates/nord-core` disagree with them?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = a fit or an observed range, **ASSUMED** = reading without a measurement.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Is there a direct, proven link from a preset field to a strip word or packed word? | **No.** The ARM sender that writes `DM(0xE0000 + 62 k + w)` and `0xC1DCC..0xC1DDF` is still unidentified ([dsp-rx-protocol](dsp-rx-protocol.md) 5). What exists is a shape match: every continuous preset field is 7 bits (`0..127`), the DSP unpacks a 7-bit packed value to a float `v/128` (127 saturates to 1.0), and the DSP images carry 128-entry (129 with a guard word) tables. Drawbars are only 4 bits (`0..8`), so the ARM must scale them. | ASSUMED (shape only) |
| What does the DSP do with a 7-bit controller value? | `FEXT BY 8:7` takes the value, `<< 24` places it at the top of a Q31 word, `0x7F000000` is clamped to `0x7FFFFFFF`, `FLOAT BY -31` gives `u = v / 128` for `v < 127` and exactly `1.0` for `v = 127`. | VERIFIED (listing, [org-stage-map](org-stage-map.md) 4, `0x12082F`) |
| Which conversion tables exist in the DSP images, and are they closed forms? | Four are exact to float32 rounding (sections 4.1 to 4.4): a **tangent prewarp table `g = tan(u0 2^(k/12))`** (semitone axis), a **gain table `2^((j-353)/24)` then `16^(k/127)`**, **linear ramps** and a **complementary pair `B = 2 - A`**. Two more are exact in their first part only. About ten 129-entry coefficient families are smooth but have no closed form yet (section 4.6). | VERIFIED (tool output, tests on synthetic copies) |
| Are the DSP tables functions of the ARM display curves (cutoff, attack, decay, LFO rate, EQ frequency, delay time)? | **No.** `relate` found no table whose `y`, `1/y`, `-ln(1-y)`, `y/(1-y)`, `atan y` or `y^2` is a constant multiple or inverse multiple of any of the seven display curves within 12 % (log spread), ORG and PNO images. | VERIFIED (negative, section 4.7) |
| Do the Rust curves disagree with the firmware tables? | Only where a table is both exact and tied to a parameter by more than shape: the EQ gain step (`0.25 dB` in Rust, `6.0206/24 = 0.2509 dB` in the table) is the one numeric disagreement (0.34 % per step). Everything else is *unconfirmed* rather than contradicted (section 7). | VERIFIED (numbers), ASSUMED (the link) |

## 2. Organ preset fields (bit offsets, widths, observed ranges)

Command (reads only the 64 `.ns4o` and 384 `.ns4p` presets of the backup; offsets are block-relative, an `.ns4p` stores the organ block at payload byte 45):

```bash
python tools/ns4_param_inventory.py "Nord Stage 4 Factory Restore RevK v1.66.ns4b" --section organ
python tools/ns4_param_inventory.py FACTORY.ns4b --list      # the field map without a corpus
```

`u7m` = 7-bit base of a `4W+3` morph field (wheel, aftertouch and pedal deltas follow, each `W+1` bits biased by the maximum). The "DSP counterpart" column is a hypothesis unless marked.

| Field | Bit offset (A / B) | Width | Observed (factory corpus) | DSP counterpart | Status |
|---|---|---|---|---|---|
| `drawbar_1..9` | `184 + 19 i` / `432 + 19 i` | 4 (`u4m`, max 8) | `0..8`, all 9 values | 4-bit, not 7-bit: the ARM must scale `0..8` to the 7-bit packed field. `v = 16 d` with the saturation of section 1 would give `u = d/8`, the linear law `organ.rs` uses (hypothesis H1) | ASSUMED |
| `level_a` / `level_b` | `14` / `45` | 7 (`u7m`) | `0..127` (19 and 9 distinct) | 7-bit packed value or a strip weight; candidate gain tapers in 4.5 | ASSUMED |
| `rotary_enabled` | `80` | 1 | `0..1` | none found | open |
| `leslie_speed` | `81` | 2 | `0, 1, 3` (52 slow, 8 stop, 4 fast in `.ns4o`) | none found | open |
| `rotary_drive` | `92` | 7 | `0..102` (31 distinct) | 7-bit packed value | ASSUMED |
| `vibrato_type` | `130` | 3 | `0..5` (V1 C1 V2 C2 V3 C3) | selects one of the six LFO records `0xC13A6 + 0x28 k` ([org-dsp-map](org-dsp-map.md) 3)? | ASSUMED |
| `vibrato_on` | `376` / `624` | 1 | `0..1` | none | open |
| `perc_on, third, fast, soft` | `377..380` / `625..628` | 1 each | `0..1` | none | open |
| `octave` | `156` / `404` | 4 signed | `-1..2` (A), `0..2` (B); Rust clamps `-2..2` | key transposition, ARM side | open |
| `model` | `161` / `409` | 3 | `.ns4o`: `0..4` (41 B3, 9 Vox, 10 Farfisa, 1 Pipe 1, 3 Pipe 2); in programs up to `5` | none | see follow-up 4 |
| FX `eq_treble/mid/bass` | chain `+144/+151/+158` (organ chain at byte 80) | 7 signed | `-38..20`, `-37..46`, `-33..60` (x 0.25 dB); piano chains `-60..60` | gain table 4.2 (steps of `2^(1/24)`) | ASSUMED |
| FX `eq_mid_freq` | `+165` | 7 (`u7m`) | `14..127` | semitone-axis table 4.1 at half-semitone spacing (hypothesis H2) | ASSUMED |
| FX `drive`, `comp_amt`, `reverb_amt`, `delay_*`, `mod*_rate/amt` | see `--list` | 7 / 14 | full ranges | none found | open |

The corpus shows that the widths and the biased morph zero in the spec are right (no field exceeds its width, drawbars never exceed 8), and that `rotary_drive` is never used above 102/127. These are EMPIRICAL (one corpus), VERIFIED as a reproducible measurement.

## 3. DSP side: strips, packed words, controller unpack

From the existing maps, restated so the tables below can be read against them (all VERIFIED there, not re-measured here):

| Item | Content |
|---|---|
| 12 channel strips | `DM(0xE0000 + 62 k)`, `k = 0..11`; strips `s` and `s + 6` are blended by the 7-bit weight (`F0 = F5 - F0; F0 * F1; CLIP`), so strips `0..5` / `6..11` are A/B pairs |
| Default strip (ORG, both cores) | word 0 `0x80120BF7`, word 1 `0x80125B79`, word 7 `0x7F000000`, words 14, 15, 18, 19 = 1.0, word 25 = -1.0, words 37, 38 = `0x80000000`, word 40 = -1, words 41..43 = 1.0, **words 48..52 = `-100.0, 0.083333, 100.0, -0.166667, 1.23e-36`** (`0xC2C80000 0x3DAAAAAA 0x42C80000 0xBE2AAAAA 0x03D70A3D`), word 59 `0xC0000000`, word 60 `0x7FFF`; strips 0..5 hold this default (22 non-zero words each) and strips 6..11 are all zero, in both ORG cores and PNO Core2 |
| Packed controller words | `DM 0xC1DCC..0xC1DDF`, four 8-bit fields per word (`FEXT 8:7` value, `16:8`, type `5:3`, slot `28:3`); the slot records at `0xC10B8 + 0x18 s` select one byte per slot with the masks `0x8000, 0x800000, 0x80000000, 0x80` |
| SY1SY2 command FIFO | `[opcode:4 | DM address:28]`, `value`; opcode 0 stores `value` at `DM(address)` ([sy-dsp-map](sy-dsp-map.md) 4) |
| Smoothing | `c = 0x3F7DA3C2 = 0.99083` per pass; if a pass is one 120-frame block at 48 kHz (PNO/SY: `0x78`, ASSUMED for ORG) the time constant is `120 / (48000 (-ln c)) = 0.27 s` |

Reading of words 48..51 (ASSUMED, no ARM sender found): `-100 / 100` bound a signed 7-bit-derived value in hundredths (cents, percent), `1/12` converts a semitone to an octave fraction and `-1/6` is two of them. The two preset fields that carry cents or semitones are synth `pitch_fine` (7 signed bits, factory `-17..9`) and `pitch_coarse` (6 signed bits, `-24..24`); no organ field has a pitch range, so the strip is generic channel code, not organ-specific. This does not decide which field feeds which strip word.

## 4. Conversion tables found in the DSP images

Tool: `python tools/ns4_param_curves.py scan core2_sram.bin --min-len 40 --lo 0x90000 --segments --exact` (also `at IMAGE ADDR COUNT --segments` for one table). A table is a one-direction run of finite floats; trailing equal words are interpolation guards and are dropped; leading zeros are padding. Models: `linear`, `geometric` (`y = y0 r^i`), `prewarp` (`atan y = u0 r^i`), `power`. Error is in cents of ratio for the geometric kinds and in percent of the span for linear and power. "Exact" means at most 1.2 cents (or 1e-3 % of span) over at least 16 entries; the fragments the segmenter finds in non-closed curves sit at 2.4 to 3.0 cents (its tolerance), so they are never reported as exact.

Identity of the images: ORG Core1 and Core2 hold the same tables at the same DM addresses (the `--exact` listings are identical); PNO Core2 holds the ORG tables (same program); PNO Core1 and FX hold other sets, at other addresses.

### 4.1 Tangent prewarp over a semitone axis (ORG `DM 0xB0A63`, 188 entries)

| Property | Value |
|---|---|
| Closed form | `y_k = tan(u_k)`, `u_k = 2.19924e-5 * 2^(k/12)`, `k = 0..187`; entries from `y >= 1.98` are clipped to `1.99` (`k = 188..212` are `1.99`, then zeros) |
| Fit | `atan(y)` geometric with `1.0000003` semitone per step, max error **0.30 cents** over 188 entries; the plain log fit of `y` leaves 805 cents, so the tangent is required (`y/u - 1 = u^2/3` to 1 % at `u = 0.11`) |
| Meaning | `g = tan(pi f / fs)`: the prewarped integrator gain of a zero-delay-feedback (TPT) filter, tabulated per semitone of cutoff. With `fs = 48 kHz`, `f_0 = 0.3360 Hz` and `f_k = 0.3360 * 2^(k/12)`; entry 124 is 433 Hz, entry 188 is the clip |
| Anchor | no integer-semitone fit to A440 or to MIDI note 0 for `fs` = 44.1, 48 or 96 kHz (`f_0` is 55.26 semitones above MIDI 0 at 48 kHz); the table origin is **open** |
| Use | the unpacked 7-bit value times a range, then this table, is the natural path for a filter or EQ frequency; not shown to be wired that way |
| Status | closed form VERIFIED (`ns4_param_curves.py at ORG/core2_sram.bin 0xB0A63 213 --segments`), role ASSUMED |

The same shape exists as a 128-step cutoff table in the FX image: `DM 0xE3D81` (129 entries, `tan(u)` with `u0 = 7.57941e-4`, ratio `1.05659` = 0.953 semitone per step = 10.17 octaves over 128 steps, max error 0.72 cents, last entry `1.1859`) and `DM 0xE3E02` (the same with `u0 / 2`, the half-rate version, 1.7 cents). In the PNO image they sit at `DM 0xE38DC` and `0xE395D`. 11.6 Hz to 13.3 kHz at 48 kHz does not equal the display curve 14 Hz to 32 kHz of `FILTER_CUTOFF_HZ_TABLE`, so this is not the synth cutoff knob (see 4.7). Status: closed form VERIFIED, role open.

PNO Core1 `DM 0x95CC6`: 84 entries `128.8 * 2^(-k/12)` (max error **0.003 cents**, ends at 1.0661): a semitone ratio ladder over 7 octaves, role open (period or step table).

### 4.2 Gain table (ORG `DM 0xB05EB`, 481 entries)

| Part | Entries | Closed form | Error |
|---|---|---|---|
| Low | `j = 0..353` | `2^((j - 353) / 24)`: `3.7378e-5` (-88.5 dB) up to exactly `1.0` at `j = 353`; **6.0206 dB / 24 = 0.2509 dB per step** | 1.25 cents max (a roughly constant +0.9 to +1.25 cents offset for `j < 353`, 0 at `j = 353`) |
| High | `j = 354..480` | `16^(k / 127)`, `k = 1..127`; `1.02207` first, `16.0` last (+24.08 dB), then one guard `16.0` | 0.0002 cents |

So the range is -88.5 dB to +24.08 dB, with 24 steps per amplitude octave below unity and 127 steps for the last four octaves. The EQ gain field is a signed 7-bit count of 0.25 dB (`-64..63`, factory `-60..60` = +-15 dB), which sits inside the low part at +-60 steps. Status: closed form VERIFIED; the link to the EQ gain is ASSUMED (range and step size match, no ARM sender seen).

### 4.3 Linear ramps (ORG)

| DM | Entries | Form | Error |
|---|---|---|---|
| `0xB0B75` | 128 | `y = 0.00397527 + 0.00793011 i` (ends at `1.0111`) | 7e-6 % of span |
| `0xB0D90` | 128 | `y = 1.98 - 0.0154687 i = 1.98 (1 - i/128)` | 9e-6 % |
| `0xB0D4F` | 64 | `y = 0.99 - 0.0154687 i` (the first half of the previous, scaled 0.5) | 1e-5 % |

Mix or crossfade weights over a 7-bit index (dry weight `1.98 (1 - i/128)`); the factor `1.98` is two times `0.99`. Status: forms VERIFIED, role ASSUMED.

### 4.4 Complementary pair (ORG `DM 0xB1117` / `0xB1198`)

`B_i = 2 - A_i` to 1.2e-7 relative over 129 entries (`A` runs `0.00099477 .. 1.0235`, `B` runs `1.999 .. 0.9765`). The first 48 entries of `A` follow `0.5 dB` per step (`10^(0.5/20)`, within 3 cents), then the slope falls from 0.50 to 0.38 dB per step. A wet/dry or pan pair that sums to 2 (equal-gain). Status: relation VERIFIED, role ASSUMED.

### 4.5 Gain tapers (ORG `DM 0xB1096`, `0xB1117`)

`dB(y)` is a straight line of `0.5017 dB` per entry (`2^(1/12)`, exact) from -54.03 dB (`0xB1096`) or -60.05 dB (`0xB1117`), then bends toward 0 dB: slope 0.501 at entry 0, 0.489 at entry 56, 0.448 at 80, 0.203 at 112. `-ln(1 - y)` is geometric with ratio `1.0595` up to about entry 80 (1.0590 at 80, 1.0553 at 100), so `y ~ 1 - exp(-u)` with `u = u0 2^(k/12)` describes the first 60 % and nothing more (best of 14 candidate shapes, 6.6 % residual over all 128). A 7-bit level or volume taper is the obvious use. Status: low part EMPIRICAL, the rest open.

### 4.6 Smooth families without a closed form yet

ORG `DM 0x95019 .. 0x9529E` (six 129-entry tables in three pairs: rising `0.0254 .. 0.698` / falling `0.0177 .. 0.00247`; rising `1.17 .. 16.75` / falling `0.461 .. 0.138`; rising `0.82 .. 33.5` / falling `1.29 .. 0.193`), `0xB04E9` (`0.0036652 .. 0.69796`; entry 0 equals `2 pi * 14 Hz / 24 kHz`, the first `FILTER_CUTOFF_HZ_TABLE` value, but no later entry follows that curve, see 4.7), `0xB0569`, `0xB0C0E`, `0xB0C8E` (`2 .. 0.6069`, +6.02 dB to -4.3 dB), `0xB0F94` (`2 .. 1.2e-4`), `0xB1015`; PNO `0xB0174 .. 0xB0377` (`0.00257 .. 1`, `1 .. 0.002`, `0.002 .. 44.67` = +33.0 dB, `44.67 .. 0.0527`, `0.0527 .. 4.032`). They have the 129-point size of a 7-bit lookup and a dB range of 50 to 90 dB, i.e. biquad or one-pole coefficient families (the PNO cores' Butterworth biquad at fs/4 is a different, constant set), but none of the tested forms (linear, geometric, tangent, `2 sin`, `1 - exp(-2 pi x)`, `g/(1+g)`, power) reaches 1 % residual. Status: EMPIRICAL (shape), role open. Part of the ORG region `0xB0867..0xB0990` and `0xB0E92..0xB0F12` is partly uninitialised scratch (values near 1e-35 to 1e-39 and the leftovers of `0xB0805/0xB0813/0xB088E/0xB090F`, which setup routine `0x1206BE` fills at run time), so tables there are not constants.

### 4.7 No link to the ARM display curves (negative result)

`python tools/ns4_param_curves.py relate core2_sram.bin KNOWN.txt` compares every 100+ entry table with a known 128-value curve through `g(y) / k` and `g(y) * k` (both directions, `g` in `y, 1/y, -ln(1-y), y/(1-y), atan y, y^2`) and reports a log spread below 0.12. Known curves tried (all from `crates/nord-core`): `ENV_ATTACK_MS_TABLE`, `ENV_DECAY_MS_TABLE`, `FILTER_CUTOFF_HZ_TABLE`, `LFO_RATE_HZ_TABLE`, `LFO_PITCH_SEMITONES`, `EQ_FREQ_HZ`, `DELAY_TIME_MS`. Result on ORG Core2 and PNO Core1: no relation (one spurious spread of 0.079 on the almost-constant table `0xB0950`). The displays and the DSP coefficients are therefore built from different parameterisations; the ARM must convert the display index to what the DSP table expects, and the conversion is still unseen. Status: VERIFIED (negative within the forms listed).

## 5. Piano

| Preset field (bit offset A / B) | Width, observed | DSP evidence | Status |
|---|---|---|---|
| `category` (`122` / `218`), `model_id` (`125` / `221`), `name_hash` (`132` / `228`, 32 bit) | 3 bit `0..5`, 5 bit `0..17` | selects an `.npno` bank on the ARM; no DSP field | n/a |
| `variation` (`164`), `acoustic_level` (`166`: bit 1 string resonance, bit 0 soft release), `pedal_noise` (`168`), `unison` (`169`), `dyn_comp` (`171`), `timbre` (`174`), `kb_touch` (`177`) | 2, 2, 1, 2, 2, 3, 2 bits; all values occur, `kb_touch` is `0` in every factory layer | The PNO image holds a stereo insert chain (8-slot table `0xC01D6`, module chain `0xC2F48`), biquads (Butterworth fs/4: `b = 0.29289, 0.58579, 0.29289, a2 = 0.17157`), a delay-line reader with loop gain `0.9908` and 65-point tables; all slots are empty in the image. No routine is tied to string resonance, pedal noise or dynamics compression | open |
| FX chain A/B EQ (chains at bytes 37 and 92) | `eq_*` `-60..60`, `eq_mid_freq` `14..127` | 4.1 and 4.2 | ASSUMED |

The PNO 129-entry tables (4.6) are the only piano-image data with the size of a 7-bit lookup. The sample voice (decode, 6-tap Q31 interpolation, gain) is the SMP image and has no preset-driven conversion table in the parts documented.

## 6. Synth

| Preset field | Width, observed | Conversion in the repository | DSP evidence | Status |
|---|---|---|---|---|
| `osc_ctrl`, `lfo_rate`, `lfo_amount`, `filter_freq`, `filter_res`, `filter_env_amt` (morph bases) | 7, full `0..127` | display tables `FILTER_CUTOFF_HZ_TABLE` etc. (VERIFIED against the ARM display strings) | The SY1SY2 image has no filter, envelope or oscillator ([sy-dsp-map](sy-dsp-map.md) 1); it holds sample playback, six 12-tap sinc kernels and a 56-voice record array. The synth filter/envelope maths is not in any image analysed, so no DSP table can be tied to these fields | open |
| `osc_env_*`, `amp_*`, `filter_*` A/D/R | 7, `0..127` | attack/decay/release tables | same | open |
| `pitch_fine` (`1528` in layer a) / `pitch_coarse` (`1535`) | 7 signed (`-17..9` cents), 6 signed (`-24..24` st) | cents `/100`, semitones | 4.1: a semitone axis with exact `2^(k/12)` ratios is the DSP's pitch grid | ASSUMED |
| `lfo_amount` to pitch | 7 | `LFO_PITCH_SEMITONES` (ARM float table, VERIFIED) | not in a DSP image | n/a |

## 7. Rust engine curves against the firmware tables

| Rust item | Curve used | Firmware evidence | Disagreement |
|---|---|---|---|
| `cbin/fx_preset.rs:307-309`, `fx.rs:850-864` EQ treble/mid/bass | `dB = count * 0.25`, clamp +-15 | table 4.2: `2^(1/24)` = 0.25086 dB per step, so count `n` is `0.25086 n` dB | **0.34 % per step**, 0.05 dB at +-15 dB; cosmetic but exact |
| `fx.rs:168` `EQ_FREQ_HZ[128]` | display values, 200 Hz to 8 kHz, ratio about 0.43 to 0.57 semitone per step (EMPIRICAL) | semitone grid of 4.1; 128 steps over 64 semitones fits half-semitone spacing | none provable; table is display-rounded (~1 to 5 %) |
| `fx.rs:29` `firmware_q`, `Q = 0.1 * 100^(i/127)` | log scale | ARM table `0x2f970c` (VERIFIED in its docstring) | none; the DSP-side coefficient tables of 4.6 are not shown to use it |
| `engine.rs:678` layer levels | linear `raw / 127` | candidate tapers 4.5 (`0.5 dB` per step at the low end) | unconfirmed; if 4.5 is the level law the Rust law is too loud at low levels (linear `1/127` is -42 dB, taper entry 1 is about -53.5 dB) |
| `organ.rs:530` drawbar | linear `level / 8` (spec says -3 dB per notch) | none (4-bit, ARM scaling unseen) | code and spec disagree with each other (`7/8` is -1.2 dB, not -3 dB); neither is verified |
| `organ.rs:400`, `fx_preset.rs:358` rotary/amp drive | `raw / 127 * 10`, `tanh(1 + 3 d)` | none | ASSUMED on both sides |
| `fx.rs` comp, mod, delay mix/feedback, reverb amounts | `raw / 127` linear | none (4.6 families unidentified) | unconfirmed |
| `piano.rs:975-978` `dyn_comp` `powf(0.82, 0.68, 0.55)` | power laws | none | ASSUMED |
| `synth_params.rs` envelope, cutoff, LFO rate | ARM display tables | the DSP side is absent from the images | none provable |
| `organ.rs` scanner LFO | single phase | six triangle LFOs, bit-exact `2|x + 0.25| - 1`, one-pole `c = 0.9908` per pass ([org-dsp-map](org-dsp-map.md) 3) | listed there |

## Implications

- The preset side is now fully tabulated (offsets, widths, ranges); the missing half is the ARM function that turns a preset value into a strip word or packed byte. Without it every "counterpart" above stays a hypothesis, which is why this note tags none of them VERIFIED.
- Three DSP shapes are firm and reusable in the Rust engine without knowing the ARM path: the tangent prewarp over a semitone axis (4.1), the dB ladder `2^(j/24)` / `16^(k/127)` (4.2) and the `v/128` unpack (section 3). A ZDF filter or EQ in `fx.rs` can use `g = tan(pi f / fs)` and a gain from 4.2 directly.
- The display curves (cutoff, attack, decay, LFO rate, EQ frequency, delay time) are ARM-side only: the DSP tables are not functions of them, so replicating the display curves in Rust is right and independent of the DSP tables.

## Follow-ups

1. Identify the ARM writer of the strips and packed words (the 904 `0xC006CA48` call sites): with it, H1 (drawbar `16 d`), H2 (EQ frequency on the semitone grid) and the level taper can be confirmed or dropped.
2. Find the code that indexes `0xB0A63`, `0xB05EB`, `0xB1096` and `0xB1117` (xref on the immediate bases and `+ 0x41` offsets of [org-stage-map](org-stage-map.md)) to learn the index scaling and the clip of 4.1.
3. Find closed forms for 4.5 and 4.6 (try a soft-knee `u/(1 + u)`-family fit with the knee as a free parameter, and the PNO `0xB0174` set against `Q`).
4. Check which programs use organ `model = 5` (the `.ns4o` corpus uses `0..4`; `decode_organ_model` maps everything above 3 to Pipe 2).
5. Decide the drawbar law (-3 dB per notch of the spec against the linear `level / 8`) with a hardware sweep of one drawbar ([synth-hardware-sweep-plan](synth-hardware-sweep-plan.md) method).
6. Wire 4.2 into the EQ (0.25086 dB per count) when the EQ is next touched; tracked as TD-10.

## Reproduce

```bash
python tools/ns4_dsp_ldr.py dsp_image_2_ORG.bin --sram SRAM/ORG      # also 1_PNO, 0_FX
python tools/ns4_param_curves.py scan SRAM/ORG/core2_sram.bin --min-len 40 --lo 0x90000 --segments --exact
python tools/ns4_param_curves.py at SRAM/ORG/core2_sram.bin 0xB0A63 213 --segments     # 4.1
python tools/ns4_param_curves.py at SRAM/ORG/core2_sram.bin 0xB05EB 481 --segments     # 4.2
python tools/ns4_param_curves.py at SRAM/0_FX/core1_sram.bin 0xE3D81 129 --segments
python tools/ns4_param_curves.py relate SRAM/ORG/core2_sram.bin known_cutoff.txt       # 4.7
python tools/ns4_param_inventory.py FACTORY.ns4b --section organ                       # section 2
```

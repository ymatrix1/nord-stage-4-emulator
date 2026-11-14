# Synth stages, ninth batch: the Ladder24 filter stage `0x12549D`

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (cut from `bank0_CODE.bin` with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--msg-script`, `--buf`), `sharc-disasm` (listing `0x24A93A..0x24ACDA`), the image words `DM(0xC28DA..0xC28FA)` (common block `I5`), `DM(0xC02DE + i)` (shaper), `DM(0xC1360 / 0xC1361)` and the 129 word rows at `0xC08E3 + 129 r`. Builds on [synth-stage-models-3](synth-stage-models-3.md) s3 and [synth-filter-type-map](synth-filter-type-map.md).
- **Question:** What does the stage behind `kFltType = 2` (`Ladder24`, 68 of 442 filter layers) compute, who writes its coefficients, and can it be ported word for word?

Tags follow `.agents/rules/provenance-in-code.md`. Result: **`0x12549D` is bit exact (1840 of 1840 words over 40 trials, both SIMD lanes, three passes each: 960 output samples, 560 state words and 320 coefficient words; `tools/synth_filter_ladder_oracle.py`)**, and the open question of s3 of batch 3 is closed: the stage writes its own coefficients, the "fuller frame" is the cutoff and resonance words alone. SUPER 2..4 (`0x124985`) and SHAPE 3 / 4 were not started (time box spent on the filter and the split).

## 1. Who writes the coefficients: the stage itself, one call ahead

The frame words the batch 3 scan found strong (`-18, -16, -14, -12`) are the **coefficients of this call, stored by the previous call**: the stage reads them in its sample loop and at the end overwrites them (`DM(-18..-12, I3)` stores at `0x24ACA0..0x24ACBC`) with values computed in the interleaved prologue from the two frame words every filter stage reads:

| Word | Content | Law (VERIFIED run) |
|---|---|---|
| `DM(-30)` | cutoff position `p`, a float `0..1` (the strip cutoff word) | `P = FIX(p, 31)`, `idx = (P >> 24) & 0x7F`, `fr = (P & 0xFFFFFF) / 2^24` |
| `DM(-28)` | resonance word, `idx.fr` in the format of the other stages | `q = FLOAT(word, -31) = (idx + fr) / 128` |
| `-18` `b` | section input weight | `row0(p)` |
| `-16` `a` | section previous input weight | `row1(p)` |
| `-14` `c` | section pole term (negative) | `row2(p)` |
| `-12` `kf` | feedback gain | `row4(p) (q / 4)` (`DM(0xC28DA) = 0.25`) |

`rowr(p) = (1 - fr) T[idx] + fr T[idx+1]` (two multiplies and an add in 40 bit registers), rows of the 129 word tables at `DM(0xC1360) + 129 r = 0xC08E3 + 129 r` ([data/synth-ladder-coef-table.csv](data/synth-ladder-coef-table.csv), rows 0, 1, 2, 4). The cutoff word of **lane x serves both lanes** (changing `DM(-30)` of lane y leaves every coefficient of both lanes unchanged: the SIMD pair indexes the tables once); the resonance word is per lane. The listing also stores `DM(16) = 1 + row3(p) q` and `DM(18) = tableB0E11(q)` (scratch shared with the other stages, no reader found, not modelled in Rust). The first call after a reset uses whatever the frame holds (the oracle pokes it), so the audio lags the parameters by one call (4 samples).

The earlier "not driven by the shared prologue" finding was right: the other four stages compute `-26, -22, -20` in a prologue that `0x12549D` does not have; this module has its own, spread over its four sample blocks.

## 2. The audio path (VERIFIED run)

Per sample, per lane, with the common words `h0 = DM(0xC28F6) = -0.99946`, `h1 = DM(0xC28F4) = 0.99946`, `h2 = DM(0xC28F8) = -0.99891` and the shaper `T = DM(0xC02DE + i)`, `i = -512..512` ([data/synth-ladder-shaper-table.csv](data/synth-ladder-shaper-table.csv), 1025 words, odd, linear with slope 6 near 0, flat at `+-0.6316` beyond `|i| = 255`):

```text
P  = FIX(kf y, 31)    i = P >> 22 (10 bit signed)    fr = (P & 0x3FFFFF) / 2^22
t  = T[i] + (T[i+1] - T[i]) fr
fb = (h0 s0 + h1 t) - h2 s2       s0 <- t        s2 <- fb        (DC blocker of the feedback)
u  = x - fb                        n0 = u
n(j+1) = (a o(j) + b n(j)) - c o(j+1)      j = 0..3     o = old states, o4 = last y
y  = n4
```

Frame words: states `-10, -8, -6, -4` (old `o0..o3`, new `n0..n3`), `-2` last output, `0` shaper output `s0`, `2` feedback state `s2`. Register-versus-memory rounding matters and is modelled: the new states and `y` are stored truncated to 24 bits and the old ones reloaded truncated, while the products of the same sample use the full register value. A section is a one-pole low pass, `H(z) = (b + a z^-1) / (1 + c z^-1)` with `b + a = 1 + c` (DC gain 1); the loop (4 poles, gain `kf` through a clipping table) is a Moog style ladder with the resonance peak at the pole frequency.

The pole `-c` falls from 0.9990 at `p = 0` to 0.59 at `p = 0.8` and 0.02 at `p = 0.95`: at 48 kHz the one-pole frequency runs 8 Hz, 95 Hz (`p = 0.3`), 418 Hz (`0.5`), 4.0 kHz (`0.8`), 28 kHz: the table is a **log-frequency position**, not the tangent coefficient `R` of the other filter stages, so the strip word the ARM writes for `kFltType = 2` is not `R` (ASSUMED below).

## 3. Rust port, wiring (opt-in)

`synth/fw_ladder.rs` (`LadderCoefs`, `FwLadder`), tables `fw_ladder_tables.rs` (generated by `tools/synth_filter_ladder_golden.py tables`), golden `tests_fw_ladder_golden.rs` (five rows of coefficient words and 64 output samples each, generated from the Python model): coefficients within `2e-6` relative, outputs within `2e-5` absolute of the model words. `FwFilterType::Ladder` is selected for `FilterModel::Ladder24` only by `FwFilterType::from_model_stages`, used in the strip chain when `strip_stages` is on (`--synth-stages`, default for the firmware strip chain, off in `legacy_all`); `FwFilterType::from_model` keeps the `Lp4` assumption for `FilterEngine::Firmware` alone. The cutoff of the panel reaches the stage as `p` chosen so that `-c(p) = exp(-2 atan(R))` (`LadderCoefs::from_r`, binary search over the monotone row 2): ASSUMED, because the strip word for type 2 and the `+0x74 -> +0x7C` selector link are still untraced (s4). Coefficients are applied per sample (the firmware lags one call, 4 samples).

## 4. What is open for the filter

| Item | State |
|---|---|
| ARM strip word of the cutoff for type 2 (is it the same word as `R`, or a log-frequency position?) | not traced; the Rust cutoff law is the pole match above (ASSUMED); a sweep of `Edt_Syn_kFltCutoff` with `kFltType = 2` in the ARM harness would read the word at `0xC2942 - 30` |
| `+0x74 -> +0x7C` selector copy | not traced (unchanged since [synth-stage-models-3](synth-stage-models-3.md) s4) |
| Resonance scale of `q` and the real `kf` range (0..3 fed in the oracle trials; the table gives `kf <= 1.2` for `q < 0.31`) | EMPIRICAL none: no hardware clip of a Ladder24 resonance sweep |
| `DM(16)`, `DM(18)` scratch consumers | none found |
| Whether the 129 word rows at `0xC08E3` are rewritten at run time (they are RAM in the image; the pointer word `DM(0xC1360) = 0xC08E3` is the image value) | not checked (the oracle runs the image values) |

## 5. Coverage

ANALOG layers with a bit-exact stage model: **302 of 312 (96.8 %)**, unchanged. Filter layers with a bit-exact stage: 374 + 68 = **442 of 442 (100 %)**: Lp12 218, Lp24 118, BP 24, HP 14 and now Ladder24 68 (layer counts of [synth-filter-type-map](synth-filter-type-map.md)); the raw type 3 stage `0x1257B2` has no factory layer. The Ladder24 *selection and cutoff law* is ASSUMED (s3), so the filter default stays unchanged (D22) and the stage is opt-in under the strip chain.

## 6. What remains

| Stage | Layers | Notes |
|---|---|---|
| `0x124985` SUPER 2..4 | 8 | delay-line / filter-like module with PM table reads (`I9 = 0x95000`) and an `MRF` / `MRB` loop: not started; the lesson of this batch is that these modules often compute their own coefficients from the frame words, so the first step is a sample-variation scan of the frame words |
| `0x120F55`, `0x12145E` SHAPE 3 / 4 | 1 + 1 | 264 and 198 instructions, not started |

## Implications

- Every filter type of the factory presets now has a verified stage; what is left for filters is the panel to strip word law (s4), not the DSP.
- The Ladder24 default stays `Lp4` outside `--synth-stages`; with it on, the Ladder24 layers change sound (D40).

## Follow-ups

1. Trace the ARM cutoff word for `kFltType = 2` (ARM harness, `tools/arm_edit_capture.py` with the type edit id, then read `DM(0xC2942 - 30)` in the PNO / ORG pass) and replace `from_r`.
2. SUPER 2..4 and SHAPE 3 / 4 (start with a frame word variation scan, see s6).
3. A hardware clip of a Ladder24 resonance sweep for the `q` scale.

# FX Mod 1 types: A-PAN `0x122D93` and TREM `0x122C39` decoded and ported

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66), the FX DSP image (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`), ADI `elfdump` listings (`tools/adi_dis.py`), the `sharc-fx` oracle, the boot log of `tools/arm_bulk_tables.py capture`. Derived data: [data/fx-type-words.csv](data/fx-type-words.csv), [data/fx-param-fits.csv](data/fx-param-fits.csv), [data/fx-type-modules.csv](data/fx-type-modules.csv). Builds on [fx-contexts](fx-contexts.md), [fx-routine-models](fx-routine-models.md), [fx-type-map](fx-type-map.md), [program-coverage](program-coverage.md) s6.
- **Question:** which SHARC routine and which words implement the Mod 1 types (`kFx1Type` 0..11), can the most used ones be verified register by register against the oracle, and can they be ported behind `FirmwareFx`?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = shape of code or data, **ASSUMED** = a reading.

## 1. Short answer

| Mod 1 type (`kFx1Type`) | Routine (core 2 pc) | Status | Factory programs / occurrences |
|---|---|---|---|
| 0, 1 **A-PAN** soft, hard | `0x122D93`, module `0xC02BAF54` | **model VERIFIED, ported** (`FwAutoPan`) | 50 / 70 |
| 2, 3 **TREM** soft, hard | `0x122C39` (earlier called the all-pass / phaser), module `0xC02BB030` | **role identified (EMPIRICAL), routine already VERIFIED, wired** (`FwAllpass` at mix 0) | 20 / 22 |
| 4, 5 RM | `0x1239D0`, module `0xC02BB10C` | **model VERIFIED 500/500 twice, ported, wired** (`FwMod1Rest`, s7) | 3 / 3 |
| 6, 7 A-WAH | `0x123E9B`, module `0xC0279290` | **model VERIFIED, ported, wired** (s7) | 6 / 7 |
| 8, 9 WAH | `0x123DD2` (what s1 of the first version called the waveshaper: an LFO or pedal driven state variable filter; module `0xC04414C4`) | **model VERIFIED, ported, wired** (s7) | 2 / 2 |
| 10, 11 PUMP | `0x122D2C` (`0x124374` is the module's reset routine, not audio), module `0xC02BB5E4` | **model VERIFIED, ported, wired** (s7) | 9 / 19 |

Update (s7): all 12 Mod 1 types now run a firmware routine under `FirmwareFx::mod1`, so every one of the 89 programs with Mod 1 enabled (123 occurrences) is covered.

Coverage of the first pass: A-PAN plus TREM are used by **70 of the 89 programs** with Mod 1 enabled (79 %) and by **92 of 123 Mod 1 occurrences** (75 %), counting `fx.mod1=A-PAN` 70, `TREM` 22 (`program-coverage.md` s6). The wiring is opt-in (`FirmwareFx::mod1`, D22/D26); the default render is unchanged.

Not done in this pass (stopped at what could be verified): the **delay type** (`kDlyFbFxType`) routines, the **Mod 2 types** (Phaser `0x12379B`, Flanger, Chorus, Vibe, Ensemble, Spin: only the module map of [fx-contexts](fx-contexts.md) s4 exists) and tempo sync / loop period. See s6.

## 2. A-PAN `0x122D93` (VERIFIED against the oracle)

```bash
python tools/adi_dis.py SRAM/core2_sram.bin 0x245B26 0x245D00      # 66 instructions, 0x122D93 .. 0x122E20
cargo build -p nord-sharc --release
python tools/fx_models.py trace apan SRAM/core2_sram.bin            # 68 / 80 instructions, registers and DAG state identical before each
python tools/fx_models.py check apan SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 bit-identical
python tools/fx_models.py check apan SRAM/core2_sram.bin --scenarios 500 --seed 7   # 500/500 bit-identical
python tools/fx_golden.py apan > crates/nord-core/src/fx/tests_apan_golden.rs
cargo test -p nord-core --lib fx::fw_autopan
```

`tools/fx_model_apan.py` is an instruction by instruction transliteration (`tools/fx_pe.py`). 1,000 random scenarios give 0 differing words and two per-instruction traces match; `tools/tests/test_fx_model_apan.py` covers the structure. The Rust port `FwAutoPan` reproduces the 3-call golden vector (outputs and final context) to the f32 tolerance `5e-6`.

Equations (`c[o]` = word at `I1 + o`; `k = c[-7]`; `sm(t, s) = t + k (s - t)`):

| Stage | Equation | Words |
|---|---|---|
| Phase | `ph = c[-0x1F] + c[-0x20]` (32-bit), stored in `c[-0x1F]`, `c[-0x1E]`; the pass pointer `c[-0x1B]` is the address of `c[-0x1F]` itself, so the counter is the context's own word | rate `c[-0x20]`, scale `c[-0x1A]`, offset `c[-0x19]` |
| LFO | `x = int32(ph c[-0x1A] - c[-0x19])`, `u = 2 abs(x 2^c[-8]) + c[-5]` (triangle, shift `-31`, offset `-1`), `s = a1 u + a3 u^3 + a5 u^5` | poly `c[-0xB..-9]` |
| Broadcast | the **left** value `s` is copied to both elements: the right phase word never reaches the audio | `r7 <-> s0` |
| Smoothers | `S = sm(s, c[-0x1D])` (pair state `c[-0x1D..-0x1C]`); `A = sm(c[-0x18], c[-0x11])`, `P = sm(c[-0x16], c[-0xF])`, `B = sm(c[-0x17], c[-0xD])`; each stored back as a pair | |
| Gain | `w = clip(A S c[-6], B) c[-4]`, `g_L = P + w`, `g_R = P - w` | `c[-6] = 4`, `c[-4] = 0.5` |
| Block | `c[-3]` blocks of two stereo frames: `(l, r) *= (g_L, g_R)` in place | `c[-3] = 2` (boot log) |

### 2.1 Parameter words (VERIFIED, `fx-param-fits.csv`, `fx-type-words.csv`)

| Id | Word | Type 0 (soft) | Type 1 (hard) |
|---|---|---|---|
| `kFx1RateClkDiv` `v` | `c[-0x20]` (integer) | `12800 + 16384 v` | same |
| `kFx1Amt` `v` | depth `A`, `c[-0x18]` | `v / 508` | `v / 127` (`v >= 2`; `v = 1`: `1 / 508`) |
| | clip bound `B`, `c[-0x17]` | `1.0` (image) | `v / 127` (`v >= 2`) |
| | offset `P`, `c[-0x16]` | `1 - v / 254` | `1 - v / 254` |
| type word | offset `c[-0x19]` | `0x40000000` (image) | `0xC0000000` |

Reading (EMPIRICAL, from the words): the soft type gives `A S c[-6] = (v / 127) S`, below the bound `B = 1`, so `w = (v / 254) S`, so `g_L = 1 - v/254 + (v/254) S` sweeps `[1 - v/127, 1]` and `g_R` runs opposite (`g_L + g_R = 2 P`): a sine auto-pan whose depth at full amount is a full left-right pan. The hard type has `A c[-6] = 4 v / 127` against the bound `B = v / 127`, so `w` is clipped to `+-v / 254`: a near square pan (`UI "harder pan waveform"`, manual p.49). The offset word differs (`0xC0000000`) so the hard wave starts at the other phase.

### 2.2 Instance words not written by the ARM (VERIFIED)

Static DM of the FX image, core 1, row of the synth unit L0 (base `I1 = 0xC1528`), checked by `python tools/fx_mod_words.py SRAM/core1_sram.bin` (22 of 22 words match): polynomial `1.57033, -0.641942, 0.0716143` (`0x3FC90069, 0xBF245655, 0x3D92AA8A`), shift `-31`, `k = 0.99078` (`0x3F7DA3C2`, the delay's `K`), `c[-6] = 4`, `c[-5] = -1`, `c[-4] = 0.5`, initial depth state `0.25`. The block count `c[-3] = 2` and the scale `c[-0x1A] = 1` are in the boot log (`fx_core1.script`), the self pointer `c[-0x1B] = 0xC1509` too.

### 2.3 LFO frequency

One call is 4 frames (2 blocks) at the DSP frame rate `F = 43,402.34375 Hz` ([dsp-frame-rate](dsp-frame-rate.md)), so `f = RATE F / (4 2^32)`: `0.0323 Hz` at `v = 0`, `2.68 Hz` at 64, `5.29 Hz` at 127. The engine models used `0.1 .. 15 Hz` (ASSUMED). EMPIRICAL (the call length of 4 frames follows from `c[-3] = 2`; no recording of the hardware rate was compared).

## 3. TREM = `0x122C39` at mix 0 (EMPIRICAL role)

The routine of [fx-routine-models](fx-routine-models.md) s5 (`FwAllpass`) is bound to Fx1 types 2 and 3. Its gain is `g = clip(S_L A_L, S_R) + A_R` and the output `g ((1 - m) x + m F(x)) + m N(x)` with `m = c[2]`, which is **0** in the image and is not written by any type or amount id, so the output is `g x` on both channels (mono gain): a tremolo.

| Id | Word | Type 2 (soft) | Type 3 (hard) |
|---|---|---|---|
| `kFx1RateClkDiv` | `c[-0x20]` | `12800 + 16384 v` | same |
| `kFx1Amt` | `A_L` target `c[-0x1A]` | `v / 254` | `8 v / 254` (`v >= 2`; `v = 1`: `1 / 254`) |
| | `A_R` target `c[-0x19]` | `1 - v / 254` | `1 - v / 254` |
| | clip bound `c[-0x16]` | `0.5` | `v / 254` |
| type word | counter offset `c[-0x1B]` | `0xC0000000` | `0xC0000000` |

Hence `g` sweeps `[1 - v/127, 1]`: **maximum level 1 at any amount, minimum `1 - amount`**, which is the manual's description ("maximum level at zero amount", p.49). The hard type clips the sine at `v/254` after a gain of 8 (a square wave). Image words (VERIFIED, `tools/fx_mod_words.py`): the same polynomial, `k` at `c[3]`, block count `c[4] = 2`, one-pole sections `F` (`0.0801, 0.0801, 0, 0.8398, 0`) and `N` (`0.9199, -0.9199, 0, 0.8398, 0`) that only matter if `c[2]` were non zero. The role is EMPIRICAL because no hardware recording of a tremolo was compared; the previous label "all-pass / phaser" of [fx-contexts](fx-contexts.md) s5.4 is withdrawn (the routine has no all-pass lattice, `docs/research/fx-contexts.md` updated).

## 4. Rust

| Item | Result |
|---|---|
| `fx/fw_autopan.rs` (`FwAutoPan`) | port of `0x122D93`, 32 context words, golden `tests_apan_golden.rs` (generated by `tools/fx_golden.py apan`) |
| `fx/fw_mod1.rs` (`FwMod1Stage`) | 4-frame adapter for `FwAutoPan` and `FwAllpass` with the image words and the `kFx1Amt` / `kFx1RateClkDiv` laws; phase increment scaled by `F / fs`; smoothers start settled; **latency 4 frames** |
| `FirmwareFx::mod1` | opt-in (`FirmwareFx::ALL` includes it, `DEFAULT` does not); needs `LayerFxChain::mod1_type_raw` and `mod1_rate_raw` (set by `apply_fx_chain`), falls back to the engine for types 4..11, Master Clock sync and missing raw values |
| Tests | `fw_autopan.rs` (golden, phase sum, gain symmetry), `fw_mod1.rs` (pan range, hard vs soft, tremolo range and mono, LFO frequency, depth words), `tests_fw_mod1_chain.rs` (chain switch, fallbacks, rebuild), `tools/tests/test_fx_model_apan.py`, `test_fx_mod_words.py` |

## 5. Not verified end to end

* The 4-frame block call rate and the resulting Hz are read from the loop count word; the UI rate to hertz law has no hardware recording behind it (`fx.mod1` programs in `program-coverage` stay `assumed` until one exists).
* Initial smoother states: the hardware starts them at the image values (A `0.25`, others 0) and glides for about 9 ms; the stage starts settled (ASSUMED harmless).
* The engine's Mod 1 amount is the preset `raw7 / 127`, the stage rounds it back to `0..=127` (exact).

## 6. Remaining (open, see OQ21)

| Item | State |
|---|---|
| Delay types (`kDlyFbFxType`) | module map only ([fx-contexts](fx-contexts.md) s4: types 2, 3 `0x123A7B / 0x123BD0`, 4, 5 `0x123BEE`, 7 `0x122F77`, 9 `0x1230CA`, 10, 11 packed multi-tap `0x12314A / 0x12329B / 0x123466`); the base words of those instances are not in the image (the Dly row of the synth unit holds only the delay itself) and the ring variant `0x120AB5` is not modelled, so a register check needs invented pointers |
| Mod 2 types | CHOR (types 4, 5, `0x122E24`, `0x122F77`) decoded, verified and ported: [fx-mod2-types](fx-mod2-types.md). Open: PHAS `0x12379B` / `0x1238FC`, FLANG `0x12348D` / `0x123539`, VIBE `0x123C84` / `0x123DB4`, ENS, SPIN |
| Mod 1 RM, A-WAH, WAH, PUMP | done in s7 |
| Tempo sync and loop period | not decoded (the stage only runs free) |

## 7. RM, A-WAH, WAH and PUMP (Mod 1 types 4 to 11) and the boot words

Checked 2026-11-14. The pipeline is the one of [fx-mod2-types](fx-mod2-types.md) s7: `tools/fx_listing_gen.py` (four new `ROUTINES`) turns the ADI listing into `tools/fx_model_mod2_gen.py`, `tools/fx_model_mod2.py` describes the scenarios, `tools/fx_models.py` compares every written word with `sharc-fx`, `tools/fx_mod2_rust_gen.py` writes `fw_rm.rs`, `fw_wah.rs`, `fw_awah.rs`, `fw_pump.rs`.

```bash
python tools/fx_models.py check rm   SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 bit-identical (seed 7: 500/500)
python tools/fx_models.py check wah  SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 (seed 7: 500/500)
python tools/fx_models.py check awah SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 (seed 7: 500/500)
python tools/fx_models.py check pump SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 (seed 7: 500/500)
python tools/fx_models.py trace rm|wah|awah|pump SRAM/core2_sram.bin                # 76 / 85 / 83 / 49 instructions identical
python tools/fx_golden_mod1.py > crates/nord-core/src/fx/tests_mod1_golden.rs
```

| Routine | pc range | What it does (from the model and the words) | Words |
|---|---|---|---|
| `rm` (4, 5) | `0x1239D0..0x123A7A` | carrier phase `c[-0x1F] += c[-0x20] << c[-16]` when the pointer `c[-0x1D]` is the phase word itself; two sine polynomial samples per block (triangle, `c[-0x14..-0x12]`) with smoothed depth `c[-27]`, `c[-25]`, a ramp `c[-22] += c[-23]` towards -1 (reset to 0 when the sum is positive: the sign of `c[-23]` at rate 0 holds it), gain `1 - c[-26] ...` on the input pair | `c[-32]` carrier increment (table, `0x38400 .. 0xFB63B00` piano), `c[-27]`, `c[-26]` depth `v / 127`, `1 - v / 127` |
| `awah` (6, 7) | `0x123E9B..0x123F60` | envelope of the input `c[-4]` (smoothed square, cleared on entry), clipped sweep `f2`, state variable filter with two integrators per channel (`c[-0x1A..]`, `c[-12..]`) | `c[-2]` amount (`-0.5 + v / 254`), `c[-20]` envelope coefficient (quadratic in the rate), type words `c[-6..-22]` (Q, bounds) differ between 6 and 7 |
| `wah` (8, 9) | `0x123DD2..0x123E9A` | LFO phase `c[-0x1F] = [c[-0x1D]] + c[-0x20]`, triangle to sine polynomial, **or the position word `c[-16]` when `c[3]` is 0** (type 9, the PED variation: the rate id carries the pedal `v / 128`), then the same state variable filter as A-WAH | `c[-32]` increment `12800 + 16384 v`, `c[-16]` position, `c[0]` amount (table), `c[3] = 1 / 0` |
| `pump` (10, 11) | `0x122D2C..0x122D92` | phase `c[-0x1F] += c[-0x20]` (reset to `0x80000000` when the counter `c[-0x15]` differs from its copy `c[-0x14]`: the Master Clock tick; free running otherwise), capped by `c[-0x16]` (`0x7FFFFFFF` type 10, `0x7F000000` type 11), shaped by a clipped parabola, gain `1 - depth (1 - shape)` on both channels | `c[-32]` increment, `c[-27]` depth `v / 127`, `c[-18]` offset (`0x80000000` / 0), `c[-22]` cap |

**WAH found.** The first pass labelled `0x123DD2` a waveshaper because of its polynomial. It is the routine of module `0xC04414C4`, which the type switch writes for every Fx1 type (2 words to park it, 12 words for type 8, 3 for type 9: `fx-type-modules.csv`), so it is the WAH. **PUMP**: `0x124374` (`r0 = emuclk`, clears the ring and state, `r0 - emuclk` stored at `c[0x3F]`) is the reset run at activation; the audio routine is `0x122D2C`.

**Boot words (VERIFIED).** The boot message log of the harness (96,111 messages in this capture) holds the words the ARM sends to the DSPs; the DSP words of the FX instances are reduced to the last value per word by `python tools/arm_bulk_tables.py capture target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --out target/bulk` (`fx_core1.script`, 3,499 words). Applied over the static image they give the words that no type or parameter write lists: the **self pointers** (`c[-29]` of RM, WAH, PUMP: the address of `c[-31]`; PHAS 1 `c[-29]`; ENS `c[-5]`; SPIN `c[-8]`: `c[-10]`), the **block counts (1 block of 2 frames for the piano unit)**, and the instance constants (PHAS 1 all-pass gains `c[-26] = 0.3125`, `c[-25] = 0.5`, `c[-23] = 0.7383`, `c[-17] = -0.7`, `c[-16] = -0.7299`; SPIN speed clamps and tap words; PUMP `c[-17..-15]`; A-WAH `c[-5] = 1.44`). `python tools/fx_mod1_laws.py rows SRAM/core1_sram.bin target/bulk/fx_core1.script` writes [data/fx-mod1-rows.csv](data/fx-mod1-rows.csv) (name, offset, word, source `image` or `boot`); `python tools/fx_mod1_laws.py rust` writes `fx/fw_mod1_data.rs` (rows, type words and the 11 sample laws of types 4 to 11). Tests: `tools/tests/test_fx_mod1_laws.py`, `fx::fw_mod1_rest`, `fx::fw_mod2_run` (golden), `fx::tests_fw_mod1_chain`.

**Stage (`fx/fw_mod1_rest.rs`, `FwMod1Rest`).** Row (relocated self pointers), then type words, then the laws at `v` (linear between the samples, exact at them); phase increments x `F / fs`; **latency 2 frames** (the piano unit runs one block of two frames per call; the first Mod 1 stages of s2 / s3 assume 4 frames from the synth boot row: the piano boot word `c[-3]` of A-PAN is 1, so the LFO rate of `FwMod1Stage` may be a factor 2 low for the piano and organ units: open, OQ21). Wiring: `process_fw_mod1` sends types 4 to 11 to the new stage; Master Clock sync (`mod1_sync_beats`) and missing raw values keep the engine path; WAH type 9 with `mod1_ped` takes `mod1_ped_pos * 127` as its rate value (ASSUMED: the pedal reaches the DSP through the rate id).

### 7.1 Not verified

* No hardware clip contains RM, A-WAH, WAH or PUMP (`docs/research/data/emu_vs_hw_*.json`: the Vox clip has Mod 2 VIBE only); the stages are compared with the oracle word for word, not with hardware.
* The piano unit words are used for every layer (ASSUMED); the synth unit has its own rate tables (RM `0x1C200 .. 0x7DB1D00`).
* The PUMP counter `c[-0x15]` (Master Clock tick) is not driven: the stage free runs; sync stays on the engine.
* Filter coefficients (A-WAH, WAH) are those of 43.4 kHz: ASSUMED about 10 % cutoff shift at 48 kHz.

## Implications

* The two most used Mod 1 types now have the firmware's own LFO shapes, depth laws and rate scale; the engine's 0.1 .. 15 Hz rate range for A-PAN and TREM is wrong by a factor of about 3 at the top.
* Fx1 types 2, 3 are a tremolo; the module name "all-pass" came from the shape of the routine, not from a role.

## Follow-ups

0. Compare a hardware recording of a WAH or PUMP program; drive the PUMP counter from the Master Clock; check the 2-frame versus 4-frame call of the A-PAN / TREM stages on the piano unit (s7).

1. Model `0x12379B` (Fx2 type 0, Phaser) and `0x12348D` / `0x123539` (Fx2 types 2, 3) with the same tools: their instance words are static image DM like the A-PAN row.
2. Find the Dly feedback-effect instance words (activation path, [fx-contexts](fx-contexts.md) s7) so the `0x123A7B` family can be modelled on a real instance.
3. Compare a hardware recording of A-PAN and TREM at two rates against `ns4 render-patch --firmware-fx`.

## Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_0_FX.bin --sram SRAM
python tools/fx_mod_words.py SRAM/core1_sram.bin                    # 22/22 instance words match
python tools/fx_models.py check apan SRAM/core2_sram.bin --scenarios 500
uv run pytest tools/tests/test_fx_model_apan.py tools/tests/test_fx_mod_words.py tools/tests/test_fx_golden.py
cargo test -p nord-core --lib fx::
```

# FX Mod 2 types: the CHOR routines `0x122E24` and `0x122F77` decoded and ported

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66), the FX DSP image (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`), the `tools/adi_dis.py` listing of core 2 SRAM, the `sharc-fx` oracle. Derived data: [data/fx-type-words.csv](data/fx-type-words.csv), [data/fx-param-fits.csv](data/fx-param-fits.csv), [data/fx-type-modules.csv](data/fx-type-modules.csv). Builds on [fx-mod-types](fx-mod-types.md) (method), [fx-contexts](fx-contexts.md) s4 (module map), [program-coverage](program-coverage.md) s6.
- **Question:** which SHARC routines implement the Mod 2 types (`kFx2Type` 0..11, odd = variation), can the most used ones (CHOR: 93 factory programs) be reproduced instruction by instruction against the oracle, and can they run behind `FirmwareFx`?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = shape of code or data, **ASSUMED** = a reading.

## 1. Short answer

`kFx2Type` order (effects spec): 0/1 PHAS, 2/3 FLANG, **4/5 CHOR**, 6/7 VIBE, 8/9 ENS, 10/11 SPIN. The module map of [fx-contexts](fx-contexts.md) s4 pairs a **process routine and an init routine** per type; CHOR is **not** the pair `0x12348D / 0x123539` (that is FLANG, types 2, 3):

| Type | Process / init (core 2 pc) | Module | Status | Factory programs |
|---|---|---|---|---|
| 4 CHOR | `0x122E24` / `0x122F55` | `0xC0443740` | **model VERIFIED, ported** | 93 (with 5) |
| 5 CHOR variation | `0x122F77` / `0x1230AC` | `0xC0443694` | **model VERIFIED, ported** | (above) |
| 0 PHAS | `0x12379B` | `0xC02BB1E8` | **model VERIFIED, ported, wired (s8)** | 11 (with 1) |
| 1 PHAS | `0x1238FC` | `0xC02BB2C4` | model VERIFIED, ported; **not wired** (s8.3) | (above) |
| 2, 3 FLANG | `0x12348D` / `0x123539` | `0xC02BB3A0` | **model VERIFIED, ported, wired (s8)** | 9 |
| 6, 7 VIBE | `0x123C84` / `0x123DB4` | `0xC0279128` | **model VERIFIED, ported, wired** | 45 |
| 8, 9 ENS | `0x123A7B`, `0x123BD0` | `0xC0441558` | model VERIFIED 500/500, ported; **not wired** (s8.3) | 3 |
| 10, 11 SPIN | `0x123557`, `0x12377D` | `0xC0278FAC` | model VERIFIED 500/500, ported; **not wired** (s8.3) | 11 |

CHOR is used by **93 of 156 programs with Mod 2 enabled (60 %)** and 153 of 255 occurrences (`program-coverage` s6). The wiring is opt-in (`FirmwareFx::mod2`, D22; D32): `ALL` includes it, `DEFAULT` does not.

## 2. Models (VERIFIED against the oracle)

```bash
python tools/adi_dis.py SRAM/core2_sram.bin 0x245C48 0x2461B0         # 0x122E24 .. 0x1230AB
cargo build -p nord-sharc --release
python tools/fx_models.py trace chor4 SRAM/core2_sram.bin              # 139 / 205 / 271 instructions (1, 2, 3 blocks), registers and DAG state identical before each
python tools/fx_models.py check chor4 SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 bit-identical
python tools/fx_models.py check chor4 SRAM/core2_sram.bin --scenarios 500 --seed 7   # 500/500
python tools/fx_models.py check chor5 SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500
python tools/fx_golden_mod2.py > crates/nord-core/src/fx/tests_chorus_golden.rs
cargo test -p nord-core --lib fx::fw_chorus
```

`tools/fx_model_chorus.py` is an instruction by instruction transliteration (`tools/fx_pe.py`; the step source was generated once from the `adi_dis.py` listing and then checked by the trace). The scenarios randomise every context word, the 0x410-word ring, the ring pointer (`L6 = 0x200` circular buffer, `M1 = 1`, `M5 = 0x200`, `M6 = 0x202` as the routines set them) and 1 to 3 blocks of input. `tools/tests/test_fx_model_chorus.py` covers the structure.

Structure of the routines (EMPIRICAL roles, VERIFIED data flow):

| Part | Type 4 `0x122E24` | Type 5 `0x122F77` |
|---|---|---|
| Phase | `c[-0x18] += c[-0x20]` (the self pointer `c[-0x16]` addresses `c[-0x18]`), `t = abs(int(phase + c[0x12]) 2^-31)` | same, increment `c[-0x1D]`, no offset word |
| LFO | triangle `t`, or `0.5 (1 + poly(2t - 1))` (sine) when `c[0xB] != 0`; poly `c[0xC..0xE]` | triangle low-passed by `c[-0x1E]` (state `c[-0x20]`) |
| Smoothers (`s' = t + k (s - t)`, `k = c[0x14]` / `c[6]`) | delay words `c[-0x1E, -0x1D] -> c[-0x1B, -0x1A]`, mix `c[-3, -2] -> c[-1, 0]`, dry `c[1] -> c[2, 3]` | delay words `c[-0x1C, -0x1B] -> c[-0x1A, -0x19]`, gain `c[-6] -> c[-5, -4]` |
| Delay line | circular `L6 = 0x200` ring written as two mirrored pairs per frame, a guard copy of the first words at `+0x200` / `+0x402`; two fractional taps read by linear interpolation (`TRUNC`, fraction `F`, taps `i, i+1`) | same ring, taps offset by `c[-0x1A]` plus `t c[-0x19]` samples |
| Block | `c[9]` blocks of two stereo frames, in place | `c[2]` blocks |

Delay times are in DSP frames (`F = 43,402.34375 Hz`, [dsp-frame-rate](dsp-frame-rate.md)); a call is 4 frames (2 blocks), so the LFO frequency is `RATE F / (4 2^32)`: type 4 `0.032 .. 1.35 Hz` (`12800 + 4096 v`), type 5 `0.25 .. 4.0 Hz` (the quadratic word of s3) (EMPIRICAL: no hardware recording of a chorus was compared).

## 3. Parameter laws (`fx-param-fits.csv`, organ and piano rows; 11 samples each)

| Id | Type 4 | Type 5 |
|---|---|---|
| `kFx2Rate` `v` | `c[-0x20] = 12800 + 4096 v` (exact integers) | `c[-0x1D] = 98956.9 - 0.01836 v + 92.03 v^2` (err `3e-7`), `c[-0x1E]` falls linearly from `0x3F7FF2E5` to `0x3F7F3B64` |
| `kFx2Amt` `v` | delay targets `c[-0x1E] = 117 + 39 r`, `c[-0x1D] = 171 + 9 r`, `r = clamp((v - 63.5) / 63.5, 0, 1)`; mix `c[-3] = min(0.7071, 0.011129 v)`, `c[-2] = clamp((v - 63.5) 0.011129, 0, 0.7071)`; dry `c[1] = 1 - 0.0023071 v` | `c[-0x1C] = 116.02 - 0.63612 v`, `c[-0x1B] = 17.953 + 1.27224 v` (err `8e-8`) |

VERIFIED at the 11 captured amounts (0, 1, 2, 15, 31, 47, 63, 79, 95, 111, 127); the knee at 63.5 and the ramps between the samples are **EMPIRICAL** (the laws are piecewise linear through the samples; the captures do not cover `v = 64 .. 78`). The synth unit writes doubled delay words (`234`, `342`, ...) and the piano row zeroes the delay targets at `v = 0, 1`: the stage uses the organ and piano values for every layer (**ASSUMED**; OQ21).

## 4. Static instance words

Image row of the synth unit L0 (core 1 SRAM, `I1 = 0xC1302` type 4, `0xC1410` type 5; VERIFIED by dump): polynomial `1.57033, -0.641942, 0.0716143` (`0x3FC90069, 0xBF245655, 0x3D92AA8A`), `1.0`, `-1.0`, `0.5`, smoothing `k = 0.99078` (`0x3F7DA3C2`), block count `2`. Words the ARM writes at the type switch (`fx-type-words.csv`): type 4 `c[0xA] = 1`, `c[0xB] = 0`, `c[-0x1F] = 35` (piano; synth 70), `c[-0x19] = 62` (piano; synth 124); type 5 `c[-9] = 1.0`, `c[-6] = 0` and the constants `c[7..0x14]` (`0x3F7A8EEC, 0x3DB09D72, 0x3F751DD9, 0xBF53D8A3, 0x3F69EC52, 0x3F53D8A3, 0x3CAE2280, 0xBF751DD9`, piano row). The ring base, the running pointer and the self pointer are **allocated by the ARM at run time** (zero or a placeholder in the image): the port places them in its own arrays.

## 5. Rust

| Item | Result |
|---|---|
| `fx/fw_pe.rs` | pair-register machine (`Pe::apply` evaluates every effect of an instruction before applying any), circular `MODIFY`, broadcast loads; tests |
| `fx/fw_chorus4.rs`, `fw_chorus5.rs` | one `apply` per executed instruction of `0x122E24` / `0x122F77` |
| `fx/fw_chorus.rs` | memory map (context, ring, frame block, scratch pairs), `FwChorus::process`, golden tests (relative `5e-6` on outputs, final context and the 19 written ring words of each type, `tests_chorus_golden.rs`) |
| `fx/fw_mod2.rs` (`FwMod2Stage`) | 4-frame adapter, image words, the laws of s3, phase increment scaled by `F / fs`; **latency 4 frames** |
| `FirmwareFx::mod2` | opt-in (`ALL`, not `DEFAULT`); needs `LayerFxChain::mod2_type_raw` and `mod2_rate_raw` (set by `apply_fx_chain`); other types fall back to the engine; tests `tests_fw_mod2_chain.rs` |

## 6. Not verified end to end

* The rate in hertz and the delay in milliseconds (DSP frames at 43.4 kHz, not rescaled to the engine rate) have no hardware recording behind them.
* Smoothers start settled (the hardware glides for about 9 ms); the ring starts silent.
* Organ and synth rows differ from the piano words in a few constants (s3, s4).

## 7. VIBE, FLANG and PHAS (generated models)

Checked 2026-11-14. The models of this section are **generated** from the ADI listing (no hand translation), so the generators are committed tools:

```bash
python tools/adi_dis.py SRAM/core2_sram.bin 0x240000 0x250000 > core2.txt   # 0 = pc 0x120000
python tools/fx_listing_gen.py core2.txt > tools/fx_model_mod2_gen.py       # listing to Machine.step calls
python tools/fx_models.py trace vibe SRAM/core2_sram.bin                    # 162 instructions, registers and DAG identical before each
python tools/fx_models.py check vibe SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 bit-identical
python tools/fx_models.py check vibe SRAM/core2_sram.bin --scenarios 500 --seed 7   # 500/500
python tools/fx_models.py check flang SRAM/core2_sram.bin --scenarios 100           # 100/100
python tools/fx_models.py check phas0 SRAM/core2_sram.bin --scenarios 300           # 300/300
python tools/fx_models.py check phas1 SRAM/core2_sram.bin --scenarios 100           # 100/100
python tools/fx_mod2_rust_gen.py            # models to Rust (fw_vibe.rs, fw_flang.rs, fw_phas0.rs, fw_phas1.rs)
python tools/fx_golden_mod2_rest.py > crates/nord-core/src/fx/tests_mod2_golden.rs
cargo test -p nord-core --lib fx::fw_mod2_run fx::fw_vibe_stage
```

| Routine | pc range | Instructions per call (trace, 1 block) | Oracle check | Rust |
|---|---|---|---|---|
| `vibe` (types 6, 7) | `0x123C84..0x123DB3` | 162 | 500/500 (seeds 5 and 7), trace identical | `fw_vibe.rs`, wired (`fw_vibe_stage.rs`) |
| `flang` (types 2, 3) | `0x12348D..0x123538` | 113 | 100/100, trace identical | `fw_flang.rs`, wired (s8.2) |
| `phas0` (type 0) | `0x12379B..0x1238FB` | 206 | 300/300, trace identical | `fw_phas0.rs`, wired (s8.2) |
| `phas1` (type 1) | `0x1238FC..0x1239CF` | 149 | 100/100, trace identical | `fw_phas1.rs`, not wired (s8.3) |
| `ens` (types 8, 9) | `0x123A7B..0x123BCF` | 147 | 500/500 twice, trace identical | `fw_ens.rs`, not wired (s8) |
| `spin` (types 10, 11) | `0x123557..0x12377C` | 251 | 500/500 twice, trace identical | `fw_spin.rs`, not wired (s8) |

The translator is `tools/fx_leslie_gen.py`, extended for `lshift`, `max` / `min`, `recips`, `IF EQ / AV / AC`, a conditional exchange and, with the `AVAC` switch, the AV / AC flags of a fixed add or subtract and the AZ / AN flags of float results; `tools/fx_listing_gen.py` selects the routines and sets the switch. The harness (`tools/fx_model_mod2.py`) fills the context window around `I1`, the 0x400 word ring, the self pointer of the phase word and 1 to 3 blocks; scenarios whose float state would overflow (the models have no infinity) are redrawn. One scenario in six zeroes the PHAS 1 phase so that the `IF NE` branch is taken. `ENS` (`0x123A7B`, table reads at `0xC0BD4`) and `SPIN` (`0x123557`, scratch words `0xC2861..0xC286B`, a second ring `L7 = 0x26C`) have their scenarios since s8: verified and ported.

### 7.1 VIBE

Structure (EMPIRICAL roles, VERIFIED data flow): a phase accumulator `c[-0x19] += c[-0x17]` (self pointer `c[-0x16]`) drives a polynomial LFO (`c[3] = 1.5703`, `c[0]`, `c[2]`); the first loop runs `2 c[0x14]` times and applies three all-pass sections to every stereo frame in place; the second loop writes the frame into the 0x400 word ring (`c[-0x1F]` base, `c[-0x1E]` pointer), reads one tap at `c[-0x20]` words (image row `-600`) and runs two biquads (`c[-0x15 .. -0x8]`, the coefficients of CHOR type 5). `0x123DB4` clears the ring.

| Id | Law (`fx-param-fits.csv`, types 6 and 7 identical) |
|---|---|
| `kFx2Amt` `v` | `c[-2] = v / 128` (exact at 0, 1, 2, 15, 31, 47, 63, 79, 95, 111), `0x3F7FFFFE` at 127 |
| `kFx2Rate` `v` | `c[-23] = 0x3200 + 512 (v^2 >> 2)` (exact integers at the 11 samples) |

Words (`fx-type-words.csv`, piano row): `c[-3] = 0`, `c[-8] = 0xBF751DD9`, `c[-9] = 0x3CAE2280`, `c[-12] = 0x3F53D8A3`, `c[-13] = 0x3F69EC52`, `c[-18] = 0xBF53D8A3`, `c[-19] = 0x3F751DD9`, `c[-20] = 0x3DB09D72`, `c[-21] = 0x3F7A8EEC`, `c[3] = 0x3FC9006A`; image row (core 1 SRAM `0xC17A0`, `0xC16FE`): `c[-5] = 1.0`, `c[-4] = -1.0`, `c[-1] = 0x3D800000`, `c[0] = 0x3D9FFF79`, `c[2] = 0x3FBE0029`, `c[-0x20] = 0xFFFFFDA8`. The phase increment word gives `word F / (4 2^32)` Hz: `0.032 Hz` at `v = 0` to `5.2 Hz` at 127 (EMPIRICAL: no recording).

Open: type 7 writes only 3 of the 12 words (`c[-2]`, `c[-23]`, `c[3]`); the stage uses the type 6 words for both (EMPIRICAL, module delta: s8.2). The tap word `c[-0x20]` comes from the image row; the ARM writer at run time is not decoded. Amounts and rates between the 11 samples follow the closed forms (EMPIRICAL). Organ and synth rows are not compared.

### 7.2 Rust

| Item | Result |
|---|---|
| `fx/fw_pe_ops.rs` | fixed ALU flags (AV, AC), `IF` conditions, shifts, `RECIPS`, `CLIP`, pre-modify addressing; tests |
| `fx/fw_vibe.rs`, `fw_flang.rs`, `fw_phas0.rs`, `fw_phas1.rs` | generated, one `Pe::apply` per executed instruction; `tools/fx_mod2_rust_gen.py --check` and `tools/tests/test_fx_mod2_tools.py` keep them equal to the generator |
| `fx/fw_mod2_run.rs` | memory map, `process`, golden tests (`tests_mod2_golden.rs`: relative `1e-4` for VIBE and FLANG, `5e-6` for PHAS; f32 against the 40-bit registers) |
| `fx/fw_vibe_stage.rs` | 4-frame adapter, the words and laws above; `FirmwareFx::mod2` dispatches types 6, 7; tests |



## 8. FLANG and PHAS 0 wired; ENS and SPIN verified and ported

Checked 2026-11-14. Commands (SRAM images from `tools/ns4_dsp_images.py` + `tools/ns4_dsp_ldr.py --sram`):

```bash
python tools/fx_models.py check ens  SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500 bit-identical
python tools/fx_models.py check ens  SRAM/core2_sram.bin --scenarios 500 --seed 7   # 500/500
python tools/fx_models.py trace ens  SRAM/core2_sram.bin                            # 147 instructions identical
python tools/fx_models.py check spin SRAM/core2_sram.bin --scenarios 500 --seed 5   # 500/500
python tools/fx_models.py check spin SRAM/core2_sram.bin --scenarios 500 --seed 7   # 500/500
python tools/fx_models.py trace spin SRAM/core2_sram.bin                            # 251 instructions identical
python tools/fx_mod2_tables.py extract SRAM/core2_sram.bin   # ENS / PHAS tables to data/fx-mod2-tables.csv
python tools/fx_mod2_laws.py rows SRAM/core1_sram.bin        # instance rows to data/fx-mod2-rows.csv
python tools/fx_mod2_laws.py rust && python tools/fx_mod2_tables.py rust   # fw_mod2_data.rs, fw_mod2_tables.rs
python tools/fx_mod2_rust_gen.py && python tools/fx_golden_mod2_rest.py > crates/nord-core/src/fx/tests_mod2_golden.rs
cargo test -p nord-core --lib fx::
```

### 8.1 Verified models (ENS, SPIN)

| Routine | pc range | Scenario (tools/fx_model_mod2.py) | Oracle check | Rust |
|---|---|---|---|---|
| `ens` (types 8, 9) | `0x123A7B..0x123BCF` | phase `c[-8] += c[-6]` read back through `c[-5]`; ring base `c[-0x20]`, pointer `c[-0x1E]` (`L6 = 0x200`); table `0xC0BD4` (1025 words, `docs/research/data/fx-mod2-tables.csv`); `c[0x11]` blocks | 500/500 twice, trace identical | `fw_ens.rs`, golden |
| `spin` (types 10, 11) | `0x123557..0x12377C` | phase pointer `c[-8]`; ring 1 `L6 = 0xC8` (pointer `c[-0x1D]`, base `c[-0x1C]`); ring 2 `L7 = 0x26C` (base `c[0x1D]`, pointer `c[0x1B]`, modifier `c[0x1E]`); scratch words `0xC2861..0xC286B` (written before read); `c[0x21]` blocks | 500/500 twice, trace identical | `fw_spin.rs`, golden |

Two harness findings. (1) A pre-modify address `dm(m6, i6)` wraps in a circular buffer exactly like the post-modify (`dag_next`); the translator treated it as a plain add, which only mattered once a routine used `m6 = 0x202` with `L6 = 0x200` (ENS). `fx_leslie_ops.plain` and `Pe::plain` now wrap (VIBE, FLANG, PHAS re-checked 60/60). (2) The scenarios redraw a context whose float state turns NaN as well as one that overflows (the models have neither). `fext` / `fdep` / `max` / `min` / the PEx-PEy exchange are new ops of `fw_pe_ops.rs`.

### 8.2 Wired stage: PHAS 0 and FLANG (`fx/fw_mod2_law_stage.rs`)

`FwLawStage` (types 0, 2, 3; same 4 frame buffering as VIBE, latency 4 frames) takes its words from three derived sources, all generated into `fx/fw_mod2_data.rs` by `tools/fx_mod2_laws.py`: the **instance row** of the synth / piano unit in the static FX image (`fx-mod2-rows.csv`, rows at `I1 = 0xB024C` FLANG, `0xB0348` PHAS 0; the ring and self pointers are placeholders that the stage replaces), the **type words** (`fx-type-words.csv`, piano) and the **laws**: the 11 samples of every `kFx2Amt` / `kFx2Rate` write (`fx-param-fits.csv`, piano rows), linear between the samples.

| Type | Words written by a law | Notes |
|---|---|---|
| 0 PHAS 0 | Amt: `c[-28]` (table), `c[-27] = 92.84 + 0.2593 v`, `c[-26] = v / 128`; Rate: `c[-32]` phase increment (`0x27CF` at 0, 1 to `0x256145` at 127), `c[-28] = 3.413 + 0.6258 v` | `c[-28]` is written by both ids: `A(amt) R(rate) / R(64)` (ASSUMED, the captures vary one id at a time; 64 is the value of the type switch, which the two laws reproduce within 0.6) |
| 2, 3 FLANG | Amt: `c[-32]` (feedback, 0 below 47 to `-0.85`), `c[-16]` (swing), `c[-15]`; Rate: `c[-20]` increment (quadratic: `19791 - 0.011 v + 146.02 v^2`), `c[-16]`, `c[-15]` | `c[-16] + c[-15] = -100` at all 22 samples (VERIFIED, `flang_center_plus_swing_is_constant`), so `c[-15] = -100 - c[-16]`; `c[-16]` is `A(amt) R(rate) / R(64)` (the type switch value `-14.285 = -100 / 7` is both laws at 64); type 3 starts from the type 2 words (the capture switched inside module `0xC02BB3A0`: 8 words, then 4) |

PHAS 0 reads a 129 entry gain table through `c[0x1A]` (`0xE3D81`; two tables of 129 values, `fx-mod2-tables.csv`); the stage copies it into the ring array and points `c[0x1A]` there. The self pointers (`c[-0x1F]`, `c[-0x11]`) are set by the stage.

**Engine-rate rescale.** `scale = FIRMWARE_FRAME_RATE_HZ / fs`: the phase increment words are multiplied by `scale`, the FLANG swing and centre (delays in DSP frames) by `1 / scale`, the VIBE tap word (`-600` words, now even and rescaled) by `1 / scale`, so a delay keeps its duration in seconds and an LFO its frequency (tests `phase_increments_scale_with_the_rate_and_delays_with_the_inverse`, `the_tap_keeps_its_duration_when_the_engine_rate_changes`). The coefficients of the biquads and all-passes stay those of 43.4 kHz (**ASSUMED**: a 10 % shift in cutoff at 48 kHz, no resampling). The CHOR delays are rescaled in s8.3 (the ring stays 256 frames).

**VIBE type 7.** The type switch to 7 writes 3 words (`c[-2]`, `c[-23]`, `c[3]`) because the capture switched from type 6 inside the same module `0xC0279128` (`fx-type-modules.csv`: 12 words for type 6, 3 for type 7); the other nine (`c[-3]`, the biquad coefficients) are the type 6 words, which the stage uses for both. EMPIRICAL (module-delta reading); a capture of a direct load of type 7 from another module would close it.

### 8.3 PHAS 1 and SPIN wired from the boot words; ENS not (update of the same day)

The blockers of the first version were instance words that no capture lists. The boot log of the ARM (`python tools/arm_bulk_tables.py capture ...`, [fx-mod-types](fx-mod-types.md) s7) lists them: `docs/research/data/fx-mod1-rows.csv` rows `ens`, `spin`, `phas1` are the image under the boot words, and `fx/fw_mod1_data.rs` carries them.

| Type | State | Evidence |
|---|---|---|
| 1 PHAS 1 | **wired** (`FwLawStage`, `ROW_PHAS1`) | the all-pass gains `c[-26] = 0.3125`, `c[-25] = 0.5`, `c[-23] = 0.7383`, `c[-17] = -0.7`, `c[-16] = -0.7299`, the phase pointer `c[-29]` (the address of `c[-31]`) and the block count come from the boot; output non-zero, bounded, rate and amount move it. The module `0xC02BB2C4` is shared with Amp types but the instance row is the Phaser's (0xB0432), so the words are the Fx2 ones. |
| 10, 11 SPIN | **wired** (`ROW_SPIN`) | the earlier "rotor phase driver outside the routine" was wrong: the routine itself clamps the speed `c[-12] = clamp(c[-12] + c[-15], c[-17], c[-16])` and advances `c[-10]` (the type words carry `c[-17]`, `c[-16]`, `c[-15] = -22`); what was missing is the phase pointer `c[-8]` (boot: the address of `c[-10]`), `c[0x21] = 1` and the ring 2 pointers. 200,000 frames at rate 100 and amount 127 stay below 8 (`the_rotor_stays_bounded_over_long_runs`). |
| 8, 9 ENS | **not wired** | with the boot row the stage is no longer silent, but only the right output element is non-zero (left exactly 0 at every rate and amount, type 9 as well): the words of the left voice are still missing (`c[6]`, `c[7]`, `c[10]` = -167.0, the ring pointers `c[0x12..0x17]` are all mapped to one ring). The boot gives `c[-5]` (phase pointer, the address of `c[-8]`) and `c[0x11] = 1`. Not wired rather than guessed. |

The 11 PHAS programs, 9 FLANG, SPIN (11) and with 4 to 7 the CHOR / VIBE programs run the firmware stage under `FirmwareFx::mod2`; ENS (3 programs) stays on the engine. Counts from [program-coverage](program-coverage.md) s6: `fx.mod2=PHAS` 12 programs, `FLANG` 10, `CHOR` 93, `VIBE` 45, `ENS` 3, `SPIN` 11.

**CHOR rescale.** `FwMod2Stage` scales the six delay and swing words (soft `c[-0x1F]`, `c[-0x19]`, `c[-0x1E]`, `c[-0x1D]`, variation `c[-0x1C]`, `c[-0x1B]`) by `fs / F`, capped at 250 frames (the ring holds 256): at 48 kHz the soft chorus centre is 172.5 frames for the same 3.6 ms (test `delays_keep_their_duration_when_the_engine_rate_changes`). ASSUMED: those words are the delays (the doc range 117 .. 180 frames) and the swings; the biquads and smoothing poles stay.

### 8.4 Hardware clips

`tools/emu_vs_hw.py` and `docs/research/data/emu_vs_hw_*.json` hold one clip with a Mod 2 firmware type: the organ Vox program of `emu_vs_hw_amp_vox.json` (mod2 type 6, VIBE, with delay and reverb on). No clip isolates FLANG or PHAS 0, none has ENS or SPIN. The only other source in `.reference/` is the demo video of the presets (webm, whole programs, not isolated effects). The wired stages are therefore compared with the oracle (word for word) and not with hardware.

## Implications

* The engine's chorus (`9 ms`, `4.5 ms` depth, assumed) is not the firmware's: the firmware delays of the soft type are about `2.7 .. 4.2 ms` (`117 .. 180` DSP frames) and its LFO runs at `0.03 .. 1.35 Hz`.
* CHOR needs a 256-frame ring at the DSP frame rate; its delays are now rescaled to the engine rate (s8.3), the ring is not.

## Follow-ups

1. ENS: find the left voice words (s8.3). SPIN, PHAS 1 and the Mod 1 routines are done ([fx-mod-types](fx-mod-types.md) s7).
2. Capture the missing amounts `v = 64 .. 78` and the organ / synth type-switch words.
3. Compare a hardware recording of a chorus program against `ns4 render-patch --firmware-fx`.

## Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_0_FX.bin --sram SRAM
python tools/fx_models.py check chor5 SRAM/core2_sram.bin --scenarios 500
uv run pytest tools/tests/test_fx_model_chorus.py
cargo test -p nord-core --lib fx::
```

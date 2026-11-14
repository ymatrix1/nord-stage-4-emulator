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
| 0, 1 PHAS | `0x12379B`, `0x1238FC` | `0xC02BB1E8`, `0xC02BB2C4` | not modelled | 11 |
| 2, 3 FLANG | `0x12348D` / `0x123539` | `0xC02BB3A0` | not modelled | 9 |
| 6, 7 VIBE | `0x123C84`, `0x123DB4` | `0xC0279128` | not modelled | 45 |
| 8, 9 ENS | `0x123A7B`, `0x123BD0` | `0xC0441558` | not modelled | 3 |
| 10, 11 SPIN | `0x123557`, `0x12377D` | `0xC0278FAC` | not modelled | 11 |

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

## Implications

* The engine's chorus (`9 ms`, `4.5 ms` depth, assumed) is not the firmware's: the firmware delays of the soft type are about `2.7 .. 4.2 ms` (`117 .. 180` DSP frames) and its LFO runs at `0.03 .. 1.35 Hz`.
* CHOR needs a 256-frame ring at the DSP frame rate; a sample-rate independent port would need a rescaled ring (not done).

## Follow-ups

1. Model FLANG `0x12348D` / `0x123539`, PHAS `0x12379B` / `0x1238FC`, VIBE `0x123C84` / `0x123DB4`, ENS and SPIN with `tools/adi_dis.py` and the generator pattern used here (listing to `Machine.step` source, trace, oracle check).
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

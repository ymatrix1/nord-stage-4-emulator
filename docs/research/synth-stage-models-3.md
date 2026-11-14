# Synth stages, fourth batch: SHAPE SINE waves 1, 2, 5; Ladder24 and the type link (negative)

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image, `sharc-org`, `sharc-disasm` (listings `0x246BC6..`, `0x246526..`, `0x245E3E..0x2461BC`), `docs/research/data/program-sweep.json`. Builds on [synth-stage-models-2](synth-stage-models-2.md) and [synth-filter-type-map](synth-filter-type-map.md).
- **Question:** Which further stages can be modelled word for word, and what is `0x12549D` (the stage behind `Ladder24`)?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. SHAPE SINE warps (all `tools/synth_stage_shape_sine.py`, oracle `synth_wave_oracle.py --stage`)

All share the skeleton of wave 0 (increments, sine fade `g`, two oscillators, `mix_lanes`, scale `g P[16]`) and feed a warped phase word to the sine polynomial of `0x12522E`.

| Wave | Stage | Layers | Warp | Oracle result |
|---|---|---|---|---|
| 1 | `0x1235C9` | 3 | half-cycle magnitude `x = (ph & 0x7FFFFFFF) 2^-31` stretched by `rho = 1 / (1 - w)` (`w` as wave 0), clipped at 1; the sign half is restored by subtracting `2^31` from the warped word | 320 of 320 (40 trials) |
| 2 | `0x1232A5` | 4 | `x = clip(flt(ph) rho, 1)`, `rho = min(max(0.1818 / a, 1), 1 + 15 s5^2)`; output `sine - (1 - 1/rho)`; level `-g P[16]` (the listing negates `g`) | 320 of 320 |
| 5 | `0x122F1F` | 8 | `cos / (1 + k^2 + 2 k sin)` of the phase, `k = max(0.987 - 4 a, 0) s5`, `cos` = sine of the phase plus `0x40000000`, the denominator reciprocal refined with numerator `cos`; level `g (1 - 0.85 k)` with `1 - 0.85 k` taken from the **register** `k`, not the stored word | 320 of 320 |

Tag: VERIFIED (run); the spread word `s5` takes only its idle value 0.5 in the pass. Six recorded cases each in `data/synth-stage-oracle-cases-rest.json`.

Rust: `synth/fw_sine_warps.rs` (`shape_sine_wave`, `shape_sine_fold`, `shape_sine_gain`, `shape_sine_res`; the wave 0 function moved there), goldens `tests_fw_stage_golden2.rs` (wave 0) and `tests_fw_stage_golden3.rs` (waves 1, 2, 5; `tools/synth_stage_golden.py 3`); worst deviation below 2e-6 (waves 0 to 2) and 2e-5 (wave 5, a division) against the Python words. `FwStage::{ShapeSineFold, ShapeSineGain, ShapeSineRes}` map category 4 waves 1, 2, 5; opt-in as before.

Not done: waves 3 (`0x12377D`, 3 layers: two-pass loop, four sub-warps selected by COMP) and 4 (`0x120DAB`, 4 layers), same skeleton.

## 2. Coverage

ANALOG layers with a bit-exact stage model: 250 + 3 + 4 + 8 = **265 of 312 (84.9 %)**. Filter layers with a stage: 374 of 442 (84.6 %, unchanged).

## 3. `Ladder24` / `0x12549D` (open, characterised)

With the frame words `-30` (R) and `-28` (W28) poked and everything else idle, `0x12549D` is not a low pass: impulse response starts at 1e-5 and grows slowly, DC gain +19.7 dB, -6 dB per octave, almost independent of `W28` (18.7 to 20.6 dB over the whole range). A sensitivity scan of the frame words (`-32 .. +18`) shows it reads the states at `-10 .. +2` and the words `-16`, `-14`, `-12` strongly, plus the tables `0xC02DE` (interpolated, index from `-12 x -2`) and `DM(0xC1360 / 0xC1361)`; the code is a 5-state chain with a table lookup per sample (nonlinear, 182 instructions, 77 multiplies). Its coefficient words are written by something other than the filter prologue the other filter stages share, so the harness frame that suffices for entries 0, 1, 4, 5 does not drive it. Whether it is the `LPM` ladder of the panel is **not established**; `Ladder24` stays on the Lp4 approximation (ASSUMED).

## 4. The link `[strip + 0x74]` to `[strip + 0x7C]` (open, more negative results)

- The type case writes `[strip + 0x74] = enable ? type : 6` (watched again); no store to `strip + 0x7C` (`layer + 0xE4`) happens during 40,000 main-loop steps after a `kFltType` apply, so it is not a deferred refresh in the harness.
- The sender `0xC01CCF84` has the only reads of `[this + 0x7C]` in the filter path; the only static stores to a `+0x7C` field in the code are constructor / copy sites (`0xC0011D98`, `0xC001D864`, `0xC001ED18`, `0xC01D0974` copy) of other objects.
- The common edit `Edt_SynCom_kMultiLayerFlt` (`0xFD`) does not write it either.

Practical consequence: the Rust selection stays on the measured identity for the stages that are filters (Lp12 -> `0x1253BD`, Lp24 -> `0x12566E`, BP -> `0x125913`, HP -> `0x125A50`); `Ladder24` -> `0x12566E` (ASSUMED), raw type 3 -> Lp12 as the decoder does, filter off = no filter in the strip chain (the selector 6 null stage is the bypass).

## Implications

- 85 % of the factory ANALOG layers have a verified stage; the remaining 47 are SYNC (18), SHAPE SINE 3 / 4 (7), MISC (8), SUPER 1..4 (11), SHAPE 3 / 4 (2), SUB OSC (1).

## Follow-ups

1. SYNC, SHAPE SINE 3 / 4, MISC, SUPER 1..4 (the SUPER 1 listing is a seven-voice triangle-like inline wave with the same mix), SHAPE 3 / 4, SUB OSC.
2. Drive `0x12549D` with the real strip frame, or find the writer of its coefficient words.
3. Find the writer of `strip + 0x7C` on a preset-load path.

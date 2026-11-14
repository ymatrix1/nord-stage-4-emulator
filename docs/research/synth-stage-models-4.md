# Synth stages, fifth batch: SHAPE SINE wave 4 and the survey of the rest

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (cut from `bank0_CODE.bin` with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--trace`, `--watch`, `--dump`), `sharc-disasm` (listings `0x241B56..0x241EAA`, `0x2425A6..`, `0x2454..0x2461BC`, `0x243D3A..0x24405E`, `0x24930A..0x2496D2`), `docs/research/data/program-sweep.json`. Builds on [synth-stage-models-3](synth-stage-models-3.md).
- **Question:** Which of the 47 remaining ANALOG layers can be modelled word for word, in order of layer count per effort?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. SHAPE SINE wave 4, `0x120DAB` (4 layers): VERIFIED (run)

`tools/synth_stage_shape_sine2.py`, oracle `python tools/synth_wave_oracle.py --stage 0x120DAB`: **320 of 320** words (40 trials, random phases, ratios, weights, output high pass states). Six recorded cases in `data/synth-stage-oracle-cases-rest.json`. Per lane, `a = inc1 2^-31`, `s5 = DM(-5, I2)`:

| Quantity | Law (listing) |
|---|---|
| fade `g` | saw fade `C (B + clip(A - a, B))` (`0x408AE2EA`, `0x3DEBEF11`, `0x3EB0F34D`), not the sine fade constants of waves 0..2 |
| width `q` | `max(0x358637BD, 1 - (1 - clip(10 s5, 1))^2)`; `pq = f q` (stored), `ir = (1/2 pi) / pq` (two Newton steps, stored), `k = (pi 2/3) pq` |
| offset `p` | `(((c0 s5 + c1) s5 + c2) s5 + c3) s5 + 1`, `c = 0x3EFC9646, 0xBFBB9043, 0x3FEC7011, 0xBF1A171A` (stored) |
| sample | `y = \|sine(ph) (1 + s5) + (1 - s5)\|` (sine = `0x12522E` on the raw phase word), `w = 1 - min(1, y ir)`, `out = w^3 k + y_stored - p` |
| mix | the usual `mix_lanes` with scale `g P[16]` |
| **tail** | **a one-pole high pass over the stored output words**, `y_k = (x_k - x_(k-1)) + c y_(k-1)`, state words `x_-1`, `y_-1` and `c` at `DM(-32 / -30 / -28, I7)`, `I7 = DM(-9, I3) = 0xC22F6`; state 0 = last input word, state 1 = last output word |

The first run matched only sample 0 of each lane: the sample-0 output equals the input because the high pass state is zero at the start. A `--watch` of the output words found the extra store sites (`0x120F44` after the scale), and the tail `0x241E4E..0x241EA6` was the missing recurrence. The idle `c = 0x3F7F9DB2` (0.99927 per sample, a 5.6 Hz corner at 48 kHz) is read from the memory dump after a pass (ASSUMED to stay constant in the Synth; the writer is not located). Spread-word scale open as before (idle 0.5).

Rust: `synth/fw_sine_hp.rs` (`shape_sine_hp_core`, `OutputHighPass`), `FwStage::ShapeSineHp` = category 4 wave 4, golden `tests_fw_stage_golden4.rs` (`tools/synth_stage_golden4.py`), worst deviation below 2e-5 (a division).

## 2. Coverage

ANALOG layers with a bit-exact stage model: 265 + 4 = **269 of 312 (86.2 %)**. Filter layers unchanged (374 of 442, 84.6 %).

## 3. Survey of what remains (43 layers)

| Stage | Layers | Size (instr. to `RTS`) | Structure seen in the listing |
|---|---|---|---|
| `0x124985` SUPER 2..4 | 8 (2 + 5 + 1) | ~200 | not an oscillator of the saw family: a delay-line / filter module with `PM` table reads (`I9 = 0x95000`), a 4-pass loop of `MRF`/`MRB` multiply-accumulates, state at `0xB6F80..` and `0xB00E6`; needs the full harness frame |
| `0x122A32` SYNC 2 | 6 | 259 | table interpolation (`I7 = 0xB074C`, index from `FIX(s5, 30)` split by `FEXT` / `FDEP`), integer clip / `ASHIFT 7` / `BTGL 31` stages, `CLIP`/`COPYSIGN` parabolas; two oscillator passes, the usual `mix` tail |
| `0x122C5C` SYNC 3..5 | 5 | 311 | same family as `0x122A32` (three waves differ in the P words) |
| `0x1239BE` SYNC 6 | 3 | 306 | not read |
| `0x12377D` SHAPE SINE 3 | 3 | 273 | two-pass loop with two blocks of the `0x120DAB` form (cubic `w^3` terms of the phase and of the phase minus `0x80000000`, the second shifted by `FIX(DM(-4, I4))`), `COMP` / `IF GE` selection of the four sub-phases before the sine helper, no output high pass (`RTS` after the scale) |
| `0x121E9D` SUPER 1 | 3 | 138 | seven-voice inline unison (`LCNTR = 7`), triangle-like fold `\|x\| - 0.5` per voice, same detune law `u` as `0x121D6D`, weights read through `DM(I5, M2)` |
| `0x122030`, `0x122151`, `0x122417` MISC | 3 + 2 + 3 | not read | |
| `0x120F55`, `0x12145E` SHAPE 3 / 4 | 1 + 1 | not read | |
| `0x12179A` SUB OSC | 1 | not read | |
| `0x123C7C` SYNC 7, `0x123FE9` SYNC 8 | 2 + 1 | 358, 282 | not read |

Per-wave layer counts come from `program-sweep.json` paths `synth.analog.wave=<CAT>:<n>`: SYNC 1 / 2 / 3 / 4 / 6 / 7 / 8 = 1 / 6 / 3 / 2 / 3 / 2 / 1; SUPER 1..4 = 3 / 2 / 5 / 1; MISC 0..2 = 3 / 2 / 3; SHAPE SINE 3 / 4 = 3 / 4; SHAPE 3 / 4 = 1 / 1. SYNC waves 3, 4, 5 share `0x122C5C` (the wave index maps to the stage by `data/synth-wave-types.csv`).

## 4. Method notes

- The pieces of the oracle harness that made this fast: `--trace LO:HI` (register dump per step with `pc` in a short-word range), `--watch LO:LEN` (store sites on an output word) and `--dump`. When a model equals the oracle at sample 0 only, look for stores to the output words after the stage's scale (a post filter).
- The models stay hand-written from the listing; a listing-to-steps generator was not built (no time in the box). The hard part of every remaining stage is the harness frame (table pointers and memory states), not the arithmetic.

## Implications

- 86 % of the factory ANALOG layers have a verified stage; the remaining 43 are SYNC (18), SUPER 1..4 (11), MISC (8), SHAPE SINE 3 (3), SHAPE 3 / 4 (2), SUB OSC (1).

## Follow-ups

1. SHAPE SINE 3 (`0x12377D`): reuse `synth_stage_shape_sine2.osc` pieces; check whether the `FIX(DM(-4, I4))` offset is `pi / 2`-scaled.
2. SUPER 1 (`0x121E9D`) next to `synth_stage_unison.py`.
3. SYNC family (`0x122A32`, `0x122C5C`) with the table at `0xB074C`.
4. `0x12549D` (Ladder24) not touched this round, see [synth-stage-models-3](synth-stage-models-3.md) s3.

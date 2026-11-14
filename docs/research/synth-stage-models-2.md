# Synth stages, third batch: SHAPE SINE 0 and the 12 dB low pass

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image, `sharc-org`, `sharc-disasm` (listings `0x24684E..0x246B90`, `0x24A77A..0x24A93A`), `docs/research/data/program-sweep.json`. Builds on [synth-stage-models](synth-stage-models.md) and [synth-filter-type-map](synth-filter-type-map.md).
- **Question:** Which of the next stages by factory-layer use can be modelled bit exactly, and what is the coverage now?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. New stages

| Stage | What | Result | Tag |
|---|---|---|---|
| `0x123427` SHAPE SINE wave 0 (10 layers) | phase-warped sine: `x = ph 2^-31`, `y = (x + 1) / (1 - w)` if below 1, else `(x - 1) / (1 + w)`, `w = (1 - (1 - s5)^2)(1 - min(1, 5.5 a))`, warped word `2 FIX(y 2^30)` into the sine polynomial of `0x12522E` (`t = \|2 y\| - 1`), fade `C (B + clip(A - a, B))` of the sine stages, mix by `mix_lanes` | `tools/synth_stage_shape_sine.py`: **320 of 320** oracle words, 40 trials, first run; six recorded cases in `data/synth-stage-oracle-cases-rest.json`; Rust `shape_sine_wave` against the golden grid (256 points) within 2e-6 | VERIFIED (run); shape word idle only |
| `0x1253BD` 12 dB low pass (218 layers) | `0x12566E` prologue with damping table `D` at `DM(0xB0F94)` (129 words, [data/synth-filter-table-d.csv](data/synth-filter-table-d.csv)) and a **single** section, input gain `A` | `tools/synth_filter_lp12.py`: **1280 of 1280** output words over 20 random trials (random `R`, table position, 64 samples); Rust `FwFilterType::Lp2` within 1e-6 of the model words | VERIFIED (run) |

The other six SHAPE SINE routines (`0x1235C9`, `0x1232A5`, `0x12377D`, `0x120DAB`, `0x122F1F`, `0x1230DF`) share the skeleton (setup of increments, width, reciprocals; two sine-helper calls; `mix_lanes`) and differ in the warp: `0x1235C9` splits the sign bit (`BCLR R0 BY 31`) and uses one reciprocal, `0x1232A5` builds the width from `s5^2 x 15 + 1` (constant `0x41700000`) and `MIN` / `MAX` of a reciprocal (`0x3E3A2E7F`). Their models were not written (time box); each is a variation of `warp_word` in `synth_stage_shape_sine.py`.

## 2. Coverage

| Set | With a bit-exact stage model | Total | Share |
|---|---|---|---|
| ANALOG layers (factory bank, `program-sweep.json`) | 250 (107 earlier + 133 batch 2 + 10 SHAPE SINE 0) | 312 | **80.1 %** |
| Filter layers (Lp12 218, Lp24 118, Ladder24 68, BandPass 24, HighPass 14) | 374 (Lp12 `0x1253BD`, Lp24 `0x12566E`, BP `0x125913`, HP `0x125A50`) | 442 | **84.6 %** |

The 68 `Ladder24` layers remain on the LP24 approximation: raw filter type 2 selects table index 2 = `0x12549D` under the identity map, which is a delay-line module (it reads the delay pointer `I2` and the table `0xC02DE`), not a ladder low pass ([synth-filter-type-map](synth-filter-type-map.md) s3).

## 3. Not done (stop point)

SHAPE SINE waves 1..5 (22 layers), SYNC (18), MISC (8), SUPER 1..4 (11), SHAPE 3 and 4 (2), SUB OSC (1), the SAMPLE / WAVE bank stages, the `0x12549D` and `0x1257B2` filter-table entries. Next by value: SHAPE SINE 5 (`0x122F1F`, 8 layers) and the other SHAPE SINE warps, then SYNC.

## Implications

- The most used filter (Lp12) now runs the right stage in the opt-in firmware filter engine: a one-section 12 dB low pass instead of the 24 dB one.

## Follow-ups

1. Close the ladder (type 2) entry and the SHAPE SINE warps 1..5.

# Synth filter type factors: what the SHARC stages say, and what they do not

> **Update 2026-11-16 (D121, [synth-filter-links](synth-filter-links.md)):** the resonance position range 47.4 .. 80.6 below is the 60-pass transient of the pass smoother; the settled position is `128 knob / 127`. The `kFltType` link and the stage rate are measured there; the clip table of s3 is superseded by its s5 (settled resonance 2.17, uniform `2 F` 3.36 against 2.18).

- **Checked:** 2026-11-15
- **Sources (local, never committed):** ORG core 2 image with `sharc-org` (`tools/synth_filter_res_probe.py`, `tools/synth_cutoff_note_probe.py`); the bit-exact Python models of the four stages (`tools/synth_filter_model.py`, `tools/synth_filter_lp12.py`, equal to the oracle word for word, [synth-stage-equations](synth-stage-equations.md), [synth-stage-models-2](synth-stage-models-2.md)); derived tables [data/synth-filter-type-corners.csv](data/synth-filter-type-corners.csv) (`tools/synth_filter_type_corners.py`); Part A video clips (`tools/emu_vs_hw.py`).
- **Question:** The fitted cutoff factors (LP12 / BP 1.4, LP24 / HP 1.0, Ladder24 0.8, SUPER LP24 4.4, `synth/p_type.rs`) sit on top of the verified cutoff law ([synth-cutoff-voice-law](synth-cutoff-voice-law.md)). Which part of them does the SHARC stage of each type explain?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Item | Result | Tag |
|---|---|---|
| Nominal cutoff per type | all four filter stages are bilinear sections of the frame word `R` with the nominal cutoff `atan(R) / pi` cycles per sample (band pass peak at 1.000 x nominal, every damping); the ARM cutoff word is type independent ([synth-cutoff-voice-law](synth-cutoff-voice-law.md) s1). **There is no per-type cutoff factor in the firmware**: the types differ in the shape around the nominal cutoff, not in where it sits | VERIFIED (closed forms equal the bit-exact models, test `test_closed_forms_match_the_bit_exact_models`) |
| Shape per type at equal `R` and table position | the -3 dB corner over the nominal cutoff: LP24 1.22 / 1.32 / 1.39, LP12 1.37 / 1.48 / 1.53 (positions 47 / 64 / 81), so LP12 sits 1.12 to 1.10 times above LP24 and **not** 1.4; high pass 0.82 / 0.76 / 0.72; band pass -3 dB band 0.75..1.33 / 0.80..1.25 / 0.85..1.18 of the nominal; full grid in [data/synth-filter-type-corners.csv](data/synth-filter-type-corners.csv) | VERIFIED (table, generator `--check`) |
| Resonance word to table position | the pass writes `W28 = DM(0xC2926)` with `idx = 64 + 33.25 w`, `w = knob / 127 - 0.5` (the ARM word of `kFltRes`, common word 7): **47.38 at knob 0, 64.13 at 64, 80.62 at 127**, fit error 0 over 15 knobs. The engine map `TablePos::from_resonance` (0..127, `Q` 0.5..90) is wrong: the stage never goes beyond `Q = 1.9` at the idle LFO scale | VERIFIED (oracle run); idle LFO scale 0.5 ASSUMED neutral |
| Corner against the position | the LP24 corner over the nominal moves from 0.44 (idx 0) to 1.49 (idx 127): the position decides a factor of 3, so the fitted per-type factors absorb the wrong resonance map first | VERIFIED (table) |
| Fitted type factors | the stage shapes do **not** reproduce 1.4 (LP12, BP), 0.8 (Ladder24) or 4.4 (SUPER LP24): at equal position LP12 / LP24 is 1.1, BP and LP24 sit at the same nominal. The ratios of the fits come from the resonance map, the rate unit and terms not run yet | VERIFIED (negative) |
| Unit | display Hz over the natural frequency `F atan(R) / pi` is 1.971; whether the stage runs at `F` or `2 F` is still open, so the absolute factor is `1 / 1.971 = 0.507` (rate `F`) or `2 / 1.971 = 1.015` (rate `2 F`) for **every** type | ASSUMED |

## 2. What was run

1. **Corner table.** `tools/synth_filter_type_corners.py` evaluates the section equations (`H(z) = ...`, input average `(u + b) / 2`, tables A, B, C, D) on a 6000 point grid for the four stages, three `R` and eleven positions, and writes the CSV; `--check` fails on drift. Test: closed form against the bit-exact model driven by an impulse (700 samples, DFT at five frequencies, relative error below 0.3 %).
2. **Resonance position.** `tools/synth_filter_res_probe.py` writes the ARM word `-0.5 + knob / 127` into common word 7, runs 60 passes with a voice word and reads `W28`: `0x2F6070C0`, `0x40218223`, `0x509F8F40` at knobs 0, 64, 127 (idx 47.377, 64.131, 80.623). Strip word 10 does not change it. The ASSUMED part is the LFO scale `DM(0xC1E34 ..)` (0.5 idle), which multiplies the formula `W28 = FIX((w10 + C271C) lfo)` ([synth-stage-equations](synth-stage-equations.md) s5).
3. **Opt-in engine path.** `--synth-filter-law <scale>` (`SynthLayer::fw_filter_law`, `synth/fw_filter_law.rs`): the row and type factors are replaced by the one factor `scale` on the display Hz, and the resonance goes through the firmware position. Default off.

## 3. Clips

`tools/emu_vs_hw.py`, Part A video, audio-detected notes (gap score, lower is better; 12 synth and layered clips, one run per variant, the same `ns4` build). `--synth-filter-law 1.015` is the `2 F` reading with every row and type factor replaced; `--synth-fw-resonance` changes only the resonance position.

| Clip (filter) | default (fitted) | `--synth-filter-law 1.015` | `--synth-filter-law 0.507` | `--synth-fw-resonance` |
|---|---|---|---|---|
| `synth_lead` (LP24) | **0.80** | 3.50 | 7.56 | 0.81 |
| `synth_crunch` (LP12) | **0.89** | 1.06 | 1.38 | 1.11 |
| `synth_silkpad` (LP12, SUPER) | **1.64** | 5.75 | 2.31 | 2.03 |
| `synth_filterbank` (BP) | **5.39** | 8.25 | 12.63 | 6.73 |
| `synth_sineplk` (LP12) | 4.73 | 6.82 | 7.67 | **4.03** |
| `synth_megaline` (LP12) | **2.00** | 3.34 | 6.07 | 3.75 |
| `synth_horizon` (LP24, SUPER) | **1.89** | 2.11 | 4.53 | 5.64 |
| `synth_12db` (LP12) | **1.39** | 3.47 | 7.47 | 2.38 |
| `synth_phat3` (Ladder24) | **2.38** | 4.06 | 4.33 | 4.19 |
| `synth_80s` (Ladder24) | **0.90** | 1.38 | 4.69 | 1.04 |
| `synth_leaky` (Ladder24) | **1.95** | 4.58 | 4.23 | 4.49 |
| `pianopad_grand` (LP, SUPER) | 2.19 | 2.21 | 2.15 | **2.16** |
| mean of 12 | **2.18** | 3.88 | 5.42 | 3.20 |

The derived uniform factor does not reproduce the clips under either unit reading (`2 F` is nearer than `F`: mean 3.88 against 5.42), the resonance position alone helps one clip (`synth_sineplk` 4.73 to 4.03) and hurts five (`synth_horizon` 1.89 to 5.64, `synth_filterbank`, `synth_megaline`, the Ladder24 clips). No default moves (D22). Reasons that fit the data without being proved: (a) the row factors of `p_table.rs` (0.45 .. 2.0, EMPIRICAL) were fitted together with the type factors and the old resonance map, so replacing one layer of the fit breaks the others; (b) the Ladder24 stage takes its position from `LadderCoefs::from_r`, ASSUMED, and the 12 dB level drop of the two Ladder24 clips with the new map shows the ladder is sensitive to it; (c) the position range 47..81 is the idle one.

## 4. What is still missing

| # | Capture | Why |
|---|---|---|
| 1 | Stage calls per DSP frame (rate `F` or `2 F`): **answered, 2F** ([synth-super-lp24-clips](synth-super-lp24-clips.md) s7: the oscillator advances `note / (2 F)` per stage sample with the verified ratio 1.0; 4 samples per call, 2 frames per pass) | the absolute factor 0.507 or 1.015 (and with it the 1.4 / 1.0 / 0.8 anchor); `sharc-org` has no pc hit counter, only `--coverage` ranges and `--watch` stores |
| 2 | LFO scale and strip words 10 / 20 in `W28` of a sounding voice | the position range 47..81 is the idle one; modulation and the filter envelope amount widen it |
| 3 | `kFltType` to stage link (`[strip + 0x74]` to `[strip + 0x7C]`) | which stage a layer runs is ASSUMED (`FwFilterType::from_model`) |
| 4 | Ladder24 `0x12549D`: the ARM word to `DM(-30)` (its own log-frequency position) | the ladder takes no `R`; `LadderCoefs::from_r` is ASSUMED, so its 0.8 cannot be derived here |
| 5 | SUPER wave 0 on LP24 (4.4): the P words of the unison stage plus items 1 to 4 | one clip, entangled with the row factor 0.45; three more clips checked in [synth-super-lp24-clips](synth-super-lp24-clips.md) (D116): 4.4 does not hold, 2.0 fits better, default unchanged |

## Implications

- The fitted type factors are not stage physics. The derived replacement is one uniform factor (`0.507` or `1.015`) plus the verified resonance position; it stays opt-in until the unit (item 1) is captured and the clips agree (D22).
- `TablePos::from_resonance` is ASSUMED and contradicted by the oracle; it is left alone because every default render uses it ([TD](../TECH_DEBT.md) not opened, OQ22).

## Follow-ups

Section 4, items 1 to 5.

`agy` (`gemini-4-hydrogen-medium`, plan mode, isolated docs-only directory, answer from files) was given this note, the voice-law note and the CSV and asked for ranked hypotheses. Hypotheses only, none is evidence. It returned five, all restating this note: (1) co-fitted compensation for the wrong resonance map and the row factors (tested: `--synth-fw-resonance` alone worsens 5 clips, so the fits are entangled, the cause is not isolated); (2) the `kFltType` to stage link (open item 3; note `from_model` already maps LP12 to the 12 dB stage `0x1253BD`, so the "4-pole LP12" premise it cites is out of date); (3) the Ladder24 position word (open item 4); (4) the stage rate `F` against `2 F` (open item 1; the clips favour `2 F`, 3.88 against 5.42); (5) SUPER LP24 4.4 entangled with the row factor 0.45 (consistent with `synth_horizon` scoring 2.11 under the uniform 1.015, near the fitted 1.89). It proposed no measurement beyond the captures already listed.

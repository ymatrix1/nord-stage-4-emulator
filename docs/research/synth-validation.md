# Synth default validation: legacy vs strip vs strip + per-wave P

- **Checked:** 2026-11-14
- **Sources (local, never committed):** Nord Stage 4 demo Part A (`_zksT69H30c`, nine synth programs, `tools/emu_vs_hw.py` segments), the Audible Genius lesson clips in `.reference/audible-genius-nord-4/` (pitch blip `s0OhmMpbscU`, laser zap `0NMvKWDc-Lo`, Super Saw detune `MjkV3qz8Gqk`), the factory backup, `tools/synth_validate.py` (new), example `ag_synth`, `ns4 render-patch`, the program sweep (`crates/nord-core/examples/program_sweep.rs`). Nothing from the clips (audio, video, frames) is committed; the derived numbers are in `docs/research/data/synth_validation_results.json`, the clip set in `docs/research/data/synth_validation.json`.
- **Question:** Does the firmware strip chain with the stage oscillators and an EMPIRICAL per-wave P table reproduce hardware synth sounds better than the legacy chain on clips that were not used for the fit, so that it can become the synth default (D22)? Which P values are per wave and which are common?

Tags follow `.agents/rules/provenance-in-code.md`. The audio is lossy YouTube audio, the replay velocity is one value (100) for every Part A note, and the lesson clips fix no absolute level, so every number is shape evidence with the stated uncertainty.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| D22 flip | **Done**: `SynthLayer::new` now plays the strip chain, the stage oscillators and the P table (`FirmwareOptions::default()` the same, `--legacy-all` / `SynthLayer::legacy` keep the classic chain). Over 15 clips: mean score 6.6 (legacy) to 4.1 (default), 9 clips better, 3 equal, 3 worse; over the 9 hold-out clips mean gain 3.8 dB of score, 7 better; no clip worse than 3 dB in presence (all) or RMS (Part A) | EMPIRICAL |
| Common law of the SHAPE oscillator | the shape word equals the **Osc Ctrl knob** (`shape = ctrl`), no second oscillator: Smooth Lead (A:87, fit) 0.69, Crunch Synth (A:66, hold-out) 1.18. The octave-up second oscillator of the earlier fit (ratio 2.0, weight 0.35) was compensating a wrong shape word (idle 0.5) and is dropped; the "factor 2 for pitch" is therefore not needed | EMPIRICAL (two programs) |
| Per-wave level | saw 0.4, pulse 0.27 (= saw x 0.68), SHAPE 0.6, MULTI 0.26, SUPER 2.0 (linear, velocity 100, Part A level convention) | EMPIRICAL |
| Per-wave cutoff factor | SHAPE 1.75, MULTI 1.4, saw / pulse 0.9, SUPER 1.5: **only SHAPE and MULTI are stable**; saw (filter_bank 1.25 vs laser-zap 0.8) and SUPER (pad_silk 0.5 vs mega_line 2.0) had clips that disagree; section 7 splits the saw disagreement into a band pass type factor (saw 0.85, BP 1.4) and shows the SUPER one is the filter envelope of pad_silk, not a factor (SUPER 1.07 x LP12 1.4 = 1.5) | EMPIRICAL, weak |
| Super Saw level against Detune | the SUPER stage keeps the level within 1.2 dB from Detune 0 to 10 (hardware: flat), the legacy 7-voice saw falls 8 dB (-20.1 to -11.9 dB against the hardware) | EMPIRICAL (3 settings) |
| Program sweep, 384 factory programs | 0 NaN / Inf, 0 newly silent, programs with more than 0.1 % clipped samples 18 to 9, none clips more than before; 279 of 384 programs change by 3 dB or less in RMS, the extremes are -23.1 and +15.7 dB (section 6) | measured |

## 2. The validation set

Defined in `docs/research/data/synth_validation.json`; `role` = `fit` when the clip fixed a row of the P table, `holdout` when it never did. Part A clips are the confirmed notes of the program's OLED window rendered with `ns4 render-patch` (same program, same events, replay velocity 100); lesson clips are one steady note rendered with `ag_synth` at the panel values read off the OLED ([audible-genius-synth-lessons](audible-genius-synth-lessons.md) s2).

| Clip | Source | Wave (P row) | Role | What |
|---|---|---|---|---|
| lead_smooth | A:87 Smooth Lead, 22 s | SHAPE triangle | fit | LP24 370 Hz, Osc Ctrl 7.4 |
| lead_crunch | A:66 Crunch Synth, 26 s | SHAPE triangle | holdout | LP12 622 Hz, Osc Ctrl 9.4 |
| pad_silk | A:86 Silk Pad, 18 s | SUPER wave 0 | fit | LP12 294 Hz pad |
| mega_line | A:48 Mega Line Whl, 40 s | MULTI + SUPER x2 | holdout | LP12 1200 Hz, wave 1 of SUPER plays the legacy saw |
| sci_fi_80s | A:44 80s Sci-Fi, 50 s | MULTI | fit | Ladder24 1500 Hz |
| filter_bank | A:75 Filter Bank Ld, 40 s | saw x3 | fit | band pass 165 / 466 / 1300 Hz |
| phat_3osc | A:67 Phat 3 Osc MiniM, 18 s | saw x3 | holdout | Ladder24 13 kHz (open) |
| sweep_12db | A:57 12dB Sweep, 20 s | SHAPE square x2 + saw | holdout | LP12 |
| sine_pluck | A:26 Sine-Saw Plk Whl, 40 s | SHAPE SINE wave 0 | holdout | LP12 185 Hz |
| ag_blip_saw / ag_blip_square | pitch blip clip 20.65 to 21.15 s / 26.5 to 27.25 s, G4 | saw / pulse | fit (level ratio only) | filter open |
| ag_zap_saw | laser zap clip 55.75 to 57.75 s, G4 | saw | holdout | LP24 415 Hz, Res 7.3, Env Amt 7.7, decay 2.07 s |
| ag_supersaw_0 / _3 / _10 | Super Saw clip 0.5 to 1.2 s, 21 to 22.2 s, 46.5 to 48 s, G4 | SUPER wave 0 | holdout | Detune 0.0 / 3.0 / 10.0, filter open |

Part A programs with a clean single synth part were chosen from the 67 programs of the video; programs that mix the synth with organ or piano (Space Drama II, Pinky Lead) cannot be isolated on the hardware audio (the scene / section edits of `emu_vs_hw.py` isolate only the emulator side), so they are **not** in the set. Not run: the DSoundman LFO clip (it measures the LFO depth law, which the strip chain does not change, [dsoundman-synth-lfo](dsoundman-synth-lfo.md): a P table cannot move it) and the DSoundman pad-alone clip (Soft Pad 1 is not a factory program and its panel values are not read), see Follow-ups.

Metrics (`emu_vs_hw.segment_metrics`, emulator minus hardware, level-free band deltas): presence = 1.5 to 3 kHz band, brilliance = 3 to 8 kHz band, centroid (Hz), RMS (dB), attack (10 to 90 % rise of the median onset). **Score** = mean absolute delta of the sub (20 to 120 Hz), low, mid, presence and brilliance bands (the 8 to 20 kHz band is excluded, YouTube audio is low-passed). Variants: `legacy` = classic chain; `strip` = firmware strip chain, unit P, legacy waveforms; `strip+p` = the single Smooth Lead block of [synth-p-fit](synth-p-fit.md) (`--synth-p fitted`); `strip+st` = plus the stage oscillators; **default** = `strip+st` plus the per-wave table (`strip+st+tbl`).

## 3. Result: score of every variant (lower is better)

| Clip | Role | Wave | legacy | strip | strip+p | strip+st | **default** |
|---|---|---|---|---|---|---|---|
| lead_smooth | fit | SHAPE | 4.93 | 4.05 | 0.97 | 4.05 | **0.69** |
| lead_crunch | holdout | SHAPE | 8.33 | 2.07 | 5.04 | 2.07 | **1.18** |
| pad_silk | fit | SUPER | 6.53 | 6.24 | 9.17 | 5.39 | **7.63** |
| mega_line | holdout | MULTI + SUPER | 2.98 | 3.22 | n/a | 3.64 | **2.03** |
| sci_fi_80s | fit | MULTI | 2.03 | 2.01 | n/a | 0.83 | **0.24** |
| filter_bank | fit | saw | 3.15 | 3.82 | 6.65 | 3.82 | **4.46** |
| phat_3osc | holdout | saw | 3.21 | 2.91 | n/a | 2.91 | **2.91** |
| sweep_12db | holdout | SHAPE square | 21.38 | 12.50 | n/a | 4.85 | **4.59** |
| sine_pluck | holdout | SHAPE SINE | 5.99 | 6.54 | 5.15 | 7.03 | **7.03** |
| ag_blip_saw | fit | saw | 5.28 | 5.31 | n/a | 5.31 | **5.31** |
| ag_blip_square | fit | pulse | 3.29 | 3.29 | n/a | 3.29 | **3.29** |
| ag_zap_saw | holdout | saw | 8.99 | 7.27 | n/a | 7.27 | **6.93** |
| ag_supersaw_0 | holdout | SUPER | 11.31 | 11.36 | n/a | 4.38 | **4.38** |
| ag_supersaw_3 | holdout | SUPER | 6.83 | 6.62 | n/a | 6.00 | **6.02** |
| ag_supersaw_10 | holdout | SUPER | 4.96 | 4.79 | n/a | 4.95 | **4.96** |

`strip+p` is only available for the clips of the first run (it is the single-block opt-in of the earlier PR: it overfits Smooth Lead and is worse than plain strip on Crunch Synth, 5.04 against 2.07, because its fixed shape word 0.5 does not follow the Osc Ctrl knob).

Band, level and attack detail, legacy / default:

| Clip | presence dB | brilliance dB | RMS dB | centroid Hz | attack hw / legacy / default ms |
|---|---|---|---|---|---|
| lead_smooth | -16.5 / -2.0 | -6.9 / +0.2 | -6.4 / -0.9 | -232 / -38 | 19.8 / 42.3 / 35.0 |
| lead_crunch | -15.0 / +2.3 | -18.5 / +0.4 | +5.6 / +2.3 | -246 / +147 | 65.5 / 63.7 / 65.4 |
| pad_silk | +12.3 / +13.2 | +14.4 / +15.8 | -11.4 / +1.1 | +21 / -35 | 61.2 / 100.9 / 42.4 |
| mega_line | -0.1 / -1.5 | -4.8 / -3.4 | -18.5 / -5.7 | +128 / +8 | 59.1 / 63.6 / 30.8 |
| sci_fi_80s | +1.7 / +0.4 | -2.0 / -0.1 | -0.4 / +0.0 | -92 / -159 | 97.1 / 113.0 / 77.6 |
| filter_bank | -3.4 / -2.6 | -2.3 / -4.3 | +3.8 / +1.6 | -437 / -425 | 11.0 / 20.8 / 24.7 |
| phat_3osc | +3.6 / +3.5 | +2.6 / +1.4 | +12.1 / +1.0 | +563 / +486 | 26.1 / 87.2 / 66.2 |
| sweep_12db | -26.2 / -3.5 | -36.1 / -10.1 | +19.0 / +7.6 | -652 / -408 | 46.8 / 0.0 / 95.2 |
| sine_pluck | -19.0 / -17.0 | +2.8 / +7.1 | +2.0 / +4.8 | -89 / -90 | 77.8 / 58.8 / 57.4 |
| ag_blip_saw | +0.1 / +0.0 | +0.2 / -0.5 | -5.8 / -14.1 * | +72 / -75 | n/a |
| ag_blip_square | +0.1 / +0.1 | +0.2 / +0.2 | -2.4 / -14.0 * | +49 / +49 | n/a |
| ag_zap_saw | -1.4 / -0.3 | -7.0 / -2.5 | -7.2 / -3.4 * | -625 / -416 | n/a |
| ag_supersaw_0 | +5.3 / +1.0 | +8.9 / +0.8 | -20.1 / -7.9 * | +2296 / +111 | n/a |
| ag_supersaw_3 | +1.4 / -1.5 | +2.0 / -1.6 | -12.6 / -4.8 * | +446 / -186 | n/a |
| ag_supersaw_10 | +0.2 / -0.7 | +0.7 / -0.2 | -11.9 / -6.6 * | +114 / -69 | n/a |

\* RMS of lesson clips is not a level evidence (two Pure Saw notes of the same clip differ by 13 dB, velocity unknown); the P levels come from Part A only, so the lesson clips read 8 to 14 dB low. The RMS bar is applied to Part A clips only.

## 4. The P table per wave (hold-out discipline)

`synth/p_table.rs` (`PWave`, `PTable`, rows tagged in the source). Fit: level from the RMS delta of the fit clip (linear, so exact for a linear path), cutoff factor and shape law by grid on the fit-clip score, second oscillator tested and dropped.

| Row | gain | cutoff factor | Fit clip (score before to after) | Hold-out clips (legacy to default score) |
|---|---|---|---|---|
| SHAPE wave 0 (`0x1212D3`) | 0.6 | 1.75, shape word = Osc Ctrl | lead_smooth 4.05 (strip) to 0.69 | lead_crunch 8.33 to 1.18 |
| MULTI (`0x121C54`) | 0.26 | 1.4 | sci_fi_80s 0.83 to 0.24 (grid: 0.7: 1.86, 1.0: 0.83, 1.4: 0.24, 2.0: 0.34) | mega_line (with SUPER) 2.98 to 2.03 |
| SUPER wave 0 (`0x121D6D`) | 2.0 | 1.5 (compromise) | pad_silk RMS -5.1 to +1.1; cutoff 0.5 alone gives 2.03, 1.5 gives 7.63 | mega_line 2.03, ag_supersaw_0 / 3 / 10: 11.31 to 4.38, 6.83 to 6.02, 4.96 to 4.96 |
| Pure Saw (`0x1215FB`), SHAPE saw | 0.4 | 0.9 (compromise) | filter_bank RMS +5.3 to +1.6 | phat_3osc 3.21 to 2.91 (RMS +12.1 to +1.0), ag_zap_saw 8.99 to 6.93 |
| Pure Square (`0x121B99` family) | 0.27 | 0.9 (ASSUMED from the saw) | ag_blip pair: pulse 3.4 dB under the saw (one pair) | none |
| SHAPE square, SHAPE SINE, others | 1.0 | 1.0 (idle) | none | sweep_12db 12.50 to 4.59 (stage model only), sine_pluck 6.54 to 7.03 |

SHAPE shape-word grid (fit clip, gain 0.6, no second oscillator): cutoff 1.0 presence -10.1 dB, 1.35 -3.7, 1.75 -2.0. With the shape word fixed at 0.5 and the octave oscillator (earlier fit) the Crunch Synth presence reads -8.0 dB; with the shape word = Osc Ctrl (0.94) and the same cutoff, +2.3 dB. Crunch Synth with the second oscillator: score 2.11 (weight 0.35) against 1.18 (weight 0).

Disclosure: the SAW and SUPER cutoff compromises (0.9, 1.5) were chosen after seeing `ag_zap_saw` and `mega_line`, so those two hold-outs are no longer independent for those rows; the clean hold-outs are lead_crunch (SHAPE), phat_3osc (saw), sweep_12db, sine_pluck and the three Super Saw clips (filter open, so the cutoff factor does not act). Gains of the Pure Square and the SHAPE SINE / SHAPE square rows have no hold-out.

What is common across waves: the pitch factor 2 of the old fit is not needed; the SHAPE word follows the Osc Ctrl knob; the level is linear per stage family and Part A velocity 100; the unison and Super Saw spread laws of `synth/measured.rs` are untouched (the table acts on gain and cutoff only; `measured_laws` stays opt-in). The pulse sits 3.4 dB under the saw in both clips of the pitch-blip lesson, as the earlier fit found (3.3 dB).

## 5. D22 decision

Bar (task and [synth-p-fit](synth-p-fit.md) s6): the default beats legacy on most validation clips (mean score improvement), and no clip is worse by more than 3 dB in presence (all clips) or RMS (Part A clips); `tools/synth_validate.py summary` computes it.

| Variant | clips | mean score gain vs legacy | clips better | clips over the bar | D22 bar |
|---|---|---|---|---|---|
| strip (all / hold-out) | 15 / 9 | 1.15 / 1.86 | 9 / 6 | 0 / 0 | pass |
| strip + stages (all / hold-out) | 15 / 9 | 2.23 / 3.43 | 10 / 7 | 2 / 1 (mega_line presence -0.1 to -5.7, sci_fi_80s RMS -0.4 to +11.8) | fail |
| **strip + stages + P table (all / hold-out)** | 15 / 9 | **2.50 / 3.77** | **9 / 7** | **0 / 0** | **pass** |

Clips that stay worse than legacy and are named here: **pad_silk** (score 6.53 to 7.63, presence +12.3 to +13.2 dB: the Silk Pad is 13 dB too bright at LP12 294 Hz whatever the stage, the SUPER cutoff factor cannot fix it without breaking mega_line; an LP12 slope or cutoff mapping issue is suspected), **filter_bank** (3.15 to 4.46, band pass of saws, brilliance -4.3 dB), **sine_pluck** (5.99 to 7.03, presence -17 dB for every variant: the SHAPE SINE stage is bit exact but its parameter words are unknown). Three clips are equal (ag_blip x2, ag_supersaw_10).

Decision: the bar is met, so `SynthLayer::new` and `FirmwareOptions::default()` select the strip chain, the stage oscillators and the P table. The classic chain stays one call away (`SynthLayer::legacy`, `--legacy-all`, `--synth-chain legacy`), and the GUI Firmware bar shows the new defaults (Chain: "Firmware strip (default)", Stage oscillators checkbox, P table dropdown). The weak spots above are recorded in `docs/OPEN-QUESTIONS.md` (OQ22) and `docs/TECH_DEBT.md` (TD-11).

`render_hash` (example `render_hash`) changes from `25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a` to `b1c91b531d850889f169444738c3863ab5e112c0d4b62c79b2a8a28c2e5dc9aa`: every `SynthLayer::new` segment changes (the synth oscillator modes, the looped sample layers at several ratios, the full engine timeline), the organ and piano segments are unchanged. The classic chain itself is unchanged (`SynthLayer::legacy`, covered by the existing unit tests that now build on it).

## 6. Program sweep with the new default

`program_sweep` (384 factory programs, notes 48, 55, 64, 72, velocity 100, 96,000 frames), "default" mode (new default) against the committed pre-flip numbers of [program-coverage](program-coverage.md):

| Check | Before | After |
|---|---|---|
| NaN / Inf in any program | 0 | 0 |
| Silent programs, default mode | 0 | 0 |
| Silent programs, `--firmware-all --firmware-routing` mode | 9 | 9 (same programs, scene masks, OQ25) |
| Programs with more than 0.1 % clipped samples (default) | 18 | 9, none clips more than before |
| Same, firmware mode | 18 | 6, one program (Cinema Rise) 0 to 0.1 % |
| Programs whose RMS changes by at most 3 dB / 6 dB | | 279 / 324 of 384 |
| Largest RMS drops | | Danceflr Peak MW -23.1 dB, Chip Bass -21.1, Short Chords -20.4, Circus Box -20.2, PineScape Synth -18.5 |
| Largest RMS rises | | Horizon Whl +15.7 dB, Dual Gates +15.4, Silk Pad +13.3 (the hardware clip says +12.5), Synth Orchestra +13.0, Syn Flute +11.8 |

The sweep rows of `docs/research/data/program-sweep.json` and `program-coverage.md` were not regenerated (they document the pre-flip default); the per-program level shifts of up to 23 dB are the largest open risk of the flip and need hardware clips of those programs (Follow-ups).

## 7. Filter law investigation (2026-11-14, follow-up 1)

Data: `docs/research/data/synth_filter_law_results.json` (score per clip per configuration). All runs: the 15 clips of section 2, variant `strip+st+tbl`, same notes and velocity 100; the only changes are the named cutoff factors / envelope scale.

**What is established about the stages** (VERIFIED, [synth-stage-equations](synth-stage-equations.md), [synth-filter-type-map](synth-filter-type-map.md)): the stage turns `R = tan(pi fc / F)` into the natural frequency of `1 / (s^2 + P s + 1)` per section; the first -3 dB point sits at `x3(P) * fc` (`P = 2`: 0.64 fc, `P = sqrt 2`: fc, `P = 0.3`: 1.4 fc for one section). What stays unknown (ASSUMED) is the number the panel word stands for: the displayed Hz.

| Hypothesis | Test | Result |
|---|---|---|
| Displayed Hz is the first -3 dB corner (natural fc = display / x3(P)), no per-wave factor | all row factors 1.0 plus the corner law (`x1n`) | **rejected**: mean score 4.80 against 4.11; with the old rows on top 4.45; opposite sign 4.60 |
| One LP24 factor for every wave | LP24 1.3 | **rejected**: `lead_smooth` 0.69 to 2.53, `ag_zap_saw` 6.93 to 13.5 (the two LP24 clips want 1.75 and 0.85 as total factors, so the SHAPE row and the saw row differ at the same type) |
| The saw disagreement (`filter_bank` 1.25, `ag_zap_saw` 0.8) is a filter type effect | BP factor 1.4, saw row 0.85 | **supported**: `filter_bank` presence -2.6 to -0.7 dB, brilliance -4.3 to +1.0, RMS +1.6 to +0.3, `ag_zap_saw` 6.93 to 6.59; no other clip changes |
| LP12 needs a different factor from LP24 | LP12 grid 0.7 / 1.0 / 1.2 / 1.4 / 1.7 (SUPER row rescaled so SUPER x LP12 = 1.5) | **supported for 3 of 5 LP12 clips**: `sweep_12db` 4.59 to 3.04, `mega_line` 2.03 to 1.87, `sine_pluck` 7.03 to 6.40; `lead_crunch` +0.05, `pad_silk` unchanged. 1.7 is better for the first three (2.52, 1.26, 5.80) and worsens `pad_silk` (8.24) |
| The SUPER / LP12 conflict (`pad_silk` 0.5 against `mega_line` 2.0) is a cutoff factor | `NS4_ENVOCT` scan of the filter envelope octaves per full amount (8 default) | **it is the filter envelope**: `pad_silk` (env amount 0.346, decay 9.5 s, filter velocity on) reaches score 1.50, presence -0.2 dB at 2 octaves per full amount, but every other clip with an envelope wants 6 to 8 (`ag_zap_saw` 24.1 at 2, 11.1 at 4, 6.6 at 6.2; `sweep_12db` 17.1 at 2; `filter_bank` 8.5 at 2). No monotonic law in the amount, the decay or the type separates `pad_silk` from them; the replay velocity (100, amp velocity 0 and filter velocity 1 on this program) is the open suspect |

Adopted (`synth/p_table.rs::type_cutoff_scale`, applied with the P table): LP12 1.4, BP 1.4, LP24 / Ladder24 / HP 1.0 (EMPIRICAL); saw and pulse rows 0.9 to 0.85, SUPER row 1.5 to 1.07 (SUPER x LP12 = 1.5, unchanged for the two SUPER clips).

| Clip | Role | Before (PR 261 default) | After | Legacy |
|---|---|---|---|---|
| lead_smooth | fit | 0.69 | 0.69 | 4.93 |
| lead_crunch | holdout | 1.18 | 1.23 | 8.33 |
| pad_silk | fit | 7.63 | 7.62 | 6.53 |
| mega_line | holdout | 2.03 | 1.87 | 2.98 |
| sci_fi_80s | fit | 0.24 | 0.24 | 2.03 |
| filter_bank | fit | 4.46 | 4.37 | 3.15 |
| phat_3osc | holdout | 2.91 | 2.91 | 3.21 |
| sweep_12db | holdout | 4.59 | 3.04 | 21.38 |
| sine_pluck | holdout | 7.03 | 6.40 | 5.99 |
| ag_blip_saw / square | fit | 5.31 / 3.29 | 5.31 / 3.29 | 5.28 / 3.29 |
| ag_zap_saw | holdout | 6.93 | 6.59 | 8.99 |
| ag_supersaw_0 / 3 / 10 | holdout | 4.38 / 6.02 / 4.96 | unchanged | 11.31 / 6.83 / 4.96 |
| **mean (15)** | | **4.11** | **3.93** | 6.6 |

Cross-validation caveat: with 15 clips and five LP12 clips, the LP12 factor is chosen by the grid on `sweep_12db`, `mega_line` and `sine_pluck`, which were hold-outs of the earlier fit and are therefore not independent any more; `lead_crunch` (SHAPE, LP12) and `pad_silk` are the clips that argue against a larger factor (1.7). The BP factor has one clip (`filter_bank`, fit for the saw row); `ag_zap_saw` is the independent check for the saw row (improves). `render_hash` is unchanged (`b1c91b53...c9aa`): its scenarios use the idle row, Ladder24 (factor 1.0) and sample layers.

Still open: `pad_silk` (13 dB too bright, an envelope or velocity issue, not the cutoff factor), `filter_bank` and `sine_pluck` stay above legacy (4.37 against 3.15, 6.40 against 5.99; `sine_pluck` presence -17 dB is the SHAPE SINE words / wheel morph of the program, not a filter effect), the knob to strip word law needs a hardware run with an active voice (the ARM sends the cutoff only with a sounding voice, [synth-voice-control](synth-voice-control.md) s3), and `x3` above shows why the answer cannot come from the stage equations alone.

## Stop point

Time box (about 120 min) reached after: the 15-clip set and the scoring tool, the per-wave table with the SHAPE law, the flip, the workspace checks and the sweep. Not done: second hold-out clips for Pure Square, SHAPE SQUARE, SHAPE SINE and MULTI (no program with a single such wave and a clean video window was found among the 67 Part A programs); the Part A programs that mix synth with organ or piano; the DSoundman LFO and pad clips; the Ladder24 filter (type 2 is not a low pass, `phat_3osc`, `sci_fi_80s` and `mega_line` use it or LP12 with the strip law as is); a per-filter-type cutoff factor.

## Implications

- The SHAPE oscillator needs no second oscillator once the shape word follows Osc Ctrl; the P block of the firmware is probably just `ctrl` plus a level, not an octave partner.
- Level and cutoff factor are separable only per program, not per wave: SUPER and saw rows have clips with opposite optima, which points at the filter (slope or cutoff map of LP12 / LP24 / BP) rather than the oscillators.
- The SUPER stage reproduces the flat level against Detune that the legacy 7-voice saw does not.

## Follow-ups

1. Per-filter-type cutoff factor (LP12 / LP24 / BP / Ladder24): pad_silk, mega_line, filter_bank and ag_zap_saw disagree by a factor 2 to 4 on one wave, which is the signature of a filter-law error; test with the oracle response at the displayed cutoffs.
2. Hardware level of the biggest movers of the sweep (Chip Bass, Danceflr Peak MW, Horizon Whl, Dual Gates): a Part A segment or a lesson clip per program.
3. A clip with a known velocity (a controller-recorded note) to fix the saw gain, and a filtered Pure Square clip for the pulse row.
4. The DSoundman LFO and pad-alone clips once Soft Pad 1's panel values are read.
5. The legacy 7-voice Super Saw (waves 1 to 4) and SHAPE SINE waves 3, 4 still play legacy waveforms at unit P; model the stages before giving them rows.

## Reproduce

```bash
cargo build --release -p nord-core --bins --examples
python tools/synth_validate.py run --video <PartA.webm> --backup <factory.ns4b> --workdir <scratch> \
    --ag-dir <dir with s0OhmMpbscU.wav 0NMvKWDc-Lo.wav MjkV3qz8Gqk.wav> --out result.json
python tools/synth_validate.py summary result.json
# fit one row: replaces "table" in the table variants, e.g. the SUPER row
python tools/synth_validate.py run ... --only pad_silk mega_line --variants strip+st+tbl \
    --p-table "table:super=2/0/1/0.5/1.5"
ns4 render-patch "Program/Bank A/Smooth Lead.ns4p" --backup <factory.ns4b> --events events.json --wav out.wav --synth-p table:shape=0.6/0/1/1/1.75
```

The lesson WAVs are `ffmpeg -i clip.webm -vn -ac 2 -ar 48000 <id>.wav`.

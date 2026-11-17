# Synth cutoff with a sounding voice: exponential, 12 knobs per octave, key tracking in semitones

> **Update 2026-11-16 (D121, [synth-filter-links](synth-filter-links.md)):** stage calls per pass are 16 for every stage (4 samples each), supporting the `2 F` stage rate (display / natural 1.971, engine factor 1.0147); the pass = 2 frames reading stays ASSUMED. The cutoff factors per type are not in any ARM word.

- **Checked:** 2026-11-15
- **Sources (local, never committed):** ORG core 2 image (`tools/ns4_dsp_ldr.py target/DSP/dsp_image_2_ORG.bin --sram DIR`) in `sharc-org`; `tools/synth_adr_oracle.py`; new `tools/synth_cutoff_note_probe.py` (test `tools/tests/test_synth_cutoff_note_probe.py`); ARM words of [synth-knob-laws](synth-knob-laws.md) s3 and [keydown-zones](keydown-zones.md) s7; derived table [data/synth-cutoff-voice-law.csv](data/synth-cutoff-voice-law.csv). Builds on [synth-adr-and-strip-law](synth-adr-and-strip-law.md) s4, [synth-knob-laws](synth-knob-laws.md) s6.
- **Question:** The ARM word for `kFltFreq` is `(knob - 52) << 24` and the idle pass maps it to `R` linearly (4.8 kHz at knob 52, display 277 Hz). What does the pass do with a sounding voice, and what is left of the fitted per-type cutoff factors (LP12 / BP 1.4, Ladder24 0.8, SUPER LP24 4.4)?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Item | Result | Tag |
|---|---|---|
| The idle ramp `R = 0.36083 + 0.0079301 i` is **not** the cutoff law of a sounding voice | Writing the synth voice word of [keydown-zones](keydown-zones.md) s7 into the slot-0 controller `DM(0xC1DCC)` (note 60, any velocity byte) changes `R` of knob 52 from 0.36083 to **0.010179** (140.6 Hz natural) | VERIFIED (oracle run) |
| Knob law | `atan(R)` is **exactly exponential in the knob: 12 knobs per octave** (max deviation 0.0023 semitone over knobs 0..117; `R` itself deviates up to 0.4 semitone because `R = tan(...)`). `atan(R(52)) = 0.0101789`, so `fc_nat(knob) = 140.626 Hz * 2^((knob - 52) / 12)` for `fc_nat = F atan(R) / pi`, `F = 43402.34375` | VERIFIED (128 knobs, table) |
| Key tracking | with `kFltKbdTrk` word `{0, -1/3, -2/3, -1}` Q31 (strip word 38) the cutoff moves by `{0, 1/3, 2/3, 1}` semitone per semitone of `note - 60`: measured +4 / +8 / +12 semitones per octave of note (notes 36 .. 96), 0 at trk 0; reference note 60 | VERIFIED (run) |
| Display against the word | the display table (`FILTER_CUTOFF_HZ_TABLE`) over `fc_nat`: mean **1.971**, range 1.88 to 2.06 (knobs 8..116, the spread is the 2 to 3 digit rounding of the display) | VERIFIED (table), reading ASSUMED below |
| Knobs 118..127 | the ARM word is steeper there (`synth-filter-freq-law.csv`); `fc_nat` reaches 6.6 kHz at 118 and 15.3 kHz at 127 (`R` 0.52 to 1.99, the table clip `R <= 1.0111` of the idle ramp does not apply) while the display reads 12 to 45 kHz | VERIFIED (run) |
| ARM word, type | the sender reads no filter type ([synth-stage-models-10](synth-stage-models-10.md) s1): nothing in the ARM word or in this cutoff law depends on the filter type | VERIFIED |

## 2. The run

`tools/synth_cutoff_note_probe.py` writes, for channel 0 of the ORG core 2 pass (60 passes so the strip word has settled): strip word 3 and common word `DM(0xE02E9)` = `(knob - 52) << 24` (the ARM values), strip word 38 = the key tracking word, and the controller word `DM(0xC1DCC)` = `(0x0B << 24) | ((note - 60) & 0xFF) << 16 | 0x8000`; it reads the filter-lane `R` at `DM(0xC2925)`. Without the controller word the old idle ramp is reproduced (R = 0.36083 at knob 52, 0.58287 at knob 80). Results (R, natural frequency):

| Knob | trk | note | R | `fc_nat` (Hz) | display (Hz) |
|---|---|---|---|---|---|
| 52 | 0 | idle | 0.36083 | 4800 | 277 |
| 52 | 0 | 36 / 60 / 84 | 0.010179 | 140.6 | 277 |
| 52 | 3 (full) | 36 / 60 / 84 | 0.002540 / 0.010179 / 0.04074 | 35.1 / 140.6 / 560 | |
| 80 | 0 | 60 | 0.051344 | 708.7 | 1400 |
| 80 | 3 | 84 | 0.20813 | 2835 | |

So the cutoff with a voice is `fc_nat = 140.626 * 2^((knob - 52 + trk (note - 60)) / 12)` with `trk` in `{0, 1/3, 2/3, 1}`, up to the envelope and LFO terms (not run here). One sounding voice word is the only difference between the idle and the voice state; the filter-lane exponent comes from the pass tables, not from the ARM.

## 3. What this does to the fitted type factors

The default synth chain (`SynthChain::FirmwareStrip`) maps the displayed Hz to a strip word with the **idle ramp** inverse (`strip_map.rs::strip_word_for_cutoff`, ASSUMED) and multiplies the Hz by a per-type factor and a P-table row factor (`p_type.rs`, EMPIRICAL). Two consequences:

1. The mapping knob to Hz of the hardware is the exponential law above, not the ramp. Since `display = 1.971 fc_nat` (about 2: see below), a hardware stage whose natural frequency follows `R` has a corner at about half the display Hz **if the stage runs once per DSP frame**, or at about the display Hz **if it runs at `2 F`** (two samples per frame in the SIMD lanes). Which one holds decides whether the Rust stages (run at the engine rate with `R = tan(pi fc / fs)`) must be fed `display` or `display / 1.971`. This is the one open fact: the stage call rate per frame (item 1 of section 4).
2. The fitted factors LP12 1.4, BP 1.4, LP24 / HP 1.0, Ladder24 0.8, SUPER wave 0 on LP24 4.4 therefore still absorb at least (a) the stage family actually installed per filter type (LP12 maps to the 4-pole stage `0x12566E` in `FwFilterType::from_model`, hardware type 0 is the 2-pole `0x1253BD`, [synth-filter-type-map](synth-filter-type-map.md)), (b) the resonance to table position word (`TablePos::from_resonance` is ASSUMED), and (c) the `F` against `2 F` question. None of the three is the ARM cutoff word; they are not derived here, so **no factor is replaced and no default moves**.

## 4. What is still missing (exact captures)

| # | Capture | Why | How |
|---|---|---|---|
| 1 | Stage calls per DSP frame of `0x12566E`, `0x1253BD`, `0x125913`, `0x125A50`, `0x12549D` in one pass of the slot | decides `fc = F atan(R) / pi` against `2 F atan(R) / pi` (display / natural = 1.971 = 2 less 25 cent) | `sharc-org` with a pc-hit counter on each stage entry for one pass with a voice word; or the impulse spacing of the stage output against the pass output |
| 2 | Resonance word to table position (`W28`) with a voice word, for `kFltRes` 0..127 | the corner and peak of every type depend on `P` | same probe, word 7 sweep, read the frame word `DM(I3 - 28)` |
| 3 | `kFltType` to stage index for a loaded program (`[strip + 0x74]` to `[strip + 0x7C]` link) | LP12 / LP24 / Ladder selection is ASSUMED | ARM program load (`tools/arm_program_load.py`) with a synth layer, write watch on `[strip + 0x7C]` |
| 4 | Envelope and LFO terms of the filter-lane sum (`w5`, `w7`, modulation) with a voice word | `fc_nat` above is the unmodulated base | extend the probe with the filter ADR on and read `R` per pass |

**Update (D108):** item 2 of this table is half closed: the resonance word reaches the stage position as `idx = 64 + 33.25 (knob / 127 - 0.5)` (47.38 to 80.62) at the idle LFO scale ([synth-filter-type-corners](synth-filter-type-corners.md)); items 1, 3 and 4 stay open (`sharc-org` counts no pc hits, only `--coverage` ranges and `--watch` stores).

## Implications

- OQ22 and `audio-remaining` item 12 keep the factors EMPIRICAL; the cutoff law of a voice is now VERIFIED, and the `R` ramp of [synth-adr-and-strip-law](synth-adr-and-strip-law.md) s4 is the idle state only.
- A later PR can replace the display-Hz mapping with `fc_nat(knob, note, trk)` once item 1 fixes the unit; the clips to re-run are `synth_horizon`, `synth_leaky`, `pianopad_grand` (`tools/emu_vs_hw.py`).

## Follow-ups

Section 4, items 1 to 4. `agy` was asked for hypotheses on the 1.971 ratio; it returned a plan request and no hypotheses, so none are recorded.

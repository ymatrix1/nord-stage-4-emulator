# Edit knobs are Q16.16: audit of the earlier captures and the real knob laws

- **Checked:** 2026-11-14
- **Sources (local, never committed):** OS v1.66 `bank0_CODE.bin` / `bank0_SRAM.bin` in the ARM harness (settled snapshot `target/settled.snap`), `tools/arm_edit_knobs.py` (new), `tools/knob_laws.py` (new), `tools/arm_edit_capture.py` (`cell_word`, `--cell`, `--dense`). Follows [synth-stage-models-10](synth-stage-models-10.md) s1 (the `kFltFreq` sender reads a Q16.16 cell).
- **Question:** Which earlier captures fed plain integers to ids whose handlers read Q16.16 cells (so that they saw position 0 and looked value independent), and what are the real knob laws?

Tags follow `.agents/rules/provenance-in-code.md`. All laws below are **VERIFIED (run)**: the ARM writes the listed DSP word in the harness for every knob value 0..127 (ranges of up to 128 steps swept value by value, wider ranges at nine points), table [data/synth-knob-laws.csv](data/synth-knob-laws.csv), checked by `crates/nord-core/src/synth/fw_knob_laws.rs` (test module, every knob). "Idle" = the settled harness state without a loaded program, so a word that is constant here may still depend on state (see s5).

## 1. Short answer

| Question | Result |
|---|---|
| Which captures were blind? | The **synth** edit capture ([data/synth-edit-messages.csv](data/synth-edit-messages.csv), 95 ids): 7 of 55 speaking ids showed value dependence; with `knob << 16` **32 of 57** speaking ids do. The **FX small-range ids** (`fx_clients.cell_word` writes plain integers below 12 steps): `kCmpResponse`, `kDlyFbkPingPong`, `kDlyFbkFilt` speak only with the Q16.16 cell. The organ / piano capture and the FX continuous ids (range 12 or more, `cell_word` already shifts) were **not** affected (s2). |
| Envelope times | ARM writes rate index `knob - 64` (top byte of the ADR word) for amp, filter and osc ADR and for glide. With this index the pass of **two SY frames** that `strip_map.rs` assumed is **VERIFIED** (attack ramp time equals the displayed time at the first knob of each table plateau, ratio 1.00 to 1.01), and the displayed decay time is `ln 100` time constants (time to fall 40 dB), tau = 0.217 x display, not the EMPIRICAL 0.18 |
| Cutoff / resonance | `kFltFreq`: top byte `knob - 52` (knobs 0..117; steeper above) as in #324; `kFltRes`, `kOscCtrl`: `-0.5 + knob / 127` (float32 word) |
| Oscillator tuning | `kOscTuneCoarse` / `kOscTuneFine` share common word 0: `(coarse + fine / 100) x 2^24` (Q8.24 semitones, the note-term scale `0x01000000` per semitone) |
| Engine | New opt-in `--synth-knob-laws` (`SynthLayer::knob_laws`): the strip chain takes the ADR rate indices from the stored knobs (`SynthLayerDecoded::knobs`) instead of searching the table for a displayed time. Defaults and `render_hash` unchanged (D55) |

## 2. Audit: which datasets fed plain integers

| Dataset / tool | Cell value fed | Blind to Q16.16? | Action |
|---|---|---|---|
| [data/synth-edit-messages.csv](data/synth-edit-messages.csv), `arm_edit_capture.py` `--filter Edt_Syn` | plain `0..127` (and enums) | **yes** for every id that reads `(x + 0x8000) >> 16` | re-run with `arm_edit_knobs.py`; [data/synth-knob-laws.csv](data/synth-knob-laws.csv) replaces the "value dependent" column (the old file stays as the idle-state record of destinations) |
| `arm-edit-capture-summary.csv` / `-detail.csv` (organ, piano, ext layers) | plain, small ranges | partly: enum ids of range below 12 | re-run (`--filter "Edt_Org|Edt_Pno"`, both encodings): the only ids that speak value dependently are `kB3PercEnable / Speed / Level / Harmonic`; no organ / piano claim rests on the artifact. Drawbars (0..8) enter through the verified `v << 16` chain of `arm_boot.py`; Leslie angle / drive ids live in FX units |
| FX continuous ids: [data/fx-param-fits.csv](data/fx-param-fits.csv), `fx_clients.py --slots`, `fx_eq_capture.py` | `cell_word`: `v << 16` for ranges of 12 or more | **no** (checked in the code path `capture_slot` and `fx_eq_capture`; 746 fit rows have `table`, `linear` or `quad` forms, none constant) | none |
| FX small ids (`Edt_Effects` below 12 steps, unit 0) | plain | **yes**: `kCmpResponse`, `kDlyFbkPingPong`, `kDlyFbkFilt` | table [data/fx-small-id-laws.csv](data/fx-small-id-laws.csv); the type ids `kFx1Type`, `kDlyFbFxType` and `kFx1ClkEnable` speak with plain integers (their handler reads the raw cell), so the encoding is per id, not per range |
| `arm_layer_level.py`, `arm_master_level.py`, `arm_boot.py` drawbar | `v << 16` | no | none |

Corrections that follow:

| Earlier statement | Now |
|---|---|
| [fx-contexts](fx-contexts.md) s7 / [fx-eq-words](fx-eq-words.md): `kCmpResponse`, `kDlyFbkPingPong` "do not change a word" | `kCmpResponse` 0 -> 1 changes the attack pair `DM(0xB0148)` `0.10200 -> 0.02273` and the release pair `DM(0xB014A)` `4.6e-4 -> 2.3e-3` (response 1 = faster attack, about 5x faster release rate); `kDlyFbkFilt` 0 .. 3 and `kDlyFbkPingPong` change the delay feedback coefficient words (`DM(0xC1CEF..CFF, 0xC1D00)`, table) |
| [synth-voice-control](synth-voice-control.md) s3 "Every other id writes the same word for every value in the idle state" | false for 25 ids: see s3 |
| [synth-adr-and-strip-law](synth-adr-and-strip-law.md) s5 "the ARM value written for a knob position (idle ids are value independent)" | closed for ADR, cutoff, resonance, osc control, tune (s3) |

## 3. The laws (synth layer A, idle harness state)

| Id (knob range) | DSP word(s) | Law | Tag |
|---|---|---|---|
| `kAmpEnvAtk / Dcy / Rls` (0..127) | strip 0 words 26, 27, 28 (and 6 for the filter strip) | top byte = `knob - 64` (`0xC0000000` .. `0x3F000000`) | VERIFIED run |
| `kFltEnvAtk / Dcy / Rls` | strip 0 words 29, 30, 31 | same | VERIFIED run |
| `kOscEnvAtk / Dcy / Rls` | strip 0 words 32, 33, 34 | same | VERIFIED run |
| `kGlideRate` | strip 0 word 55 | same | VERIFIED run |
| `kAmpEnvDcy`, `kFltEnvDcy` Hold flag | common 0 words 10, 11 | `-0.5`, and `+0.5` at knob 127 (Hold) | VERIFIED run |
| `kFltFreq` | common 0 word 1 | top byte `knob - 52`, knobs 118..127 in `CUTOFF_TOP` (clip `0x6A000000`) | VERIFIED run |
| `kFltRes` | common 0 word 7 | float32 `-0.5 + knob / 127`, fit error 3e-8 of the range | VERIFIED run |
| `kOscCtrl` | common 0 word 6 | float32 `-0.5 + knob / 127` | VERIFIED run |
| `kFltEnvAmt` | common 0 word 13 | float32 `knob / 127`: 0 .. 1.0 linear (`0x3C010204` at 1) | VERIFIED run |
| `kOscEnvAmt` (-60..60) | common 0 word 12 | float32 `knob / 60` (`-1.0 .. +1.0`) | VERIFIED run |
| `kOscTuneCoarse` (-24..24), `kOscTuneFine` (-50..50) | common 0 word 0 | `coarse << 24`, `fine / 100 x 2^24` (the last write wins: the two ids share the word) | VERIFIED run |
| `kFltKbdTrk` (0..3, Q16.16 cell only) | strip 0 word 38 | `0`, `-1/3`, `-2/3`, `-1` in Q31 (`0xD5555555`, `0xAAAAAAAA`, `0x80000000`) | VERIFIED run |
| `kFltEnvVel` (0..1, Q16.16 only) | strip 0 / 6 word 19 | `1.0` / `0.0` swap between the two strips | VERIFIED run |
| `kAmpEnvVel` (0..3, Q16.16 only) | strip 0 / 6 word 53 | strip 0: `1.0` at 0, `0.505` at 1, `0.126` at 3; strip 6: `0`, `0.757`, `1.870`: a velocity depth pair, table | VERIFIED run |
| `kVibRate` (0..127) | PNO core 1 `DM(0xB00E6)`, `0xC1B3D` | linear: `0x60A00` at 0, `0x17DC00` at 127; `word / 0x30500` is the rate in Hz, 2.0 .. 7.9 Hz (the ASSUMED `2 + 6 raw / 127` is within 1.2 %, max error 0.05 Hz) | VERIFIED run |
| `kLFORateClkDiv` (0..127) | PNO core 1 `DM(0xB00F8)` | exponential: `0x1600` at 0 to `0x3E37300` at 127 (about 0.074 octaves per step); display table agrees in shape | VERIFIED run (shape) |
| `kLayerPanSynA` (-60..60) | PNO core 1 `DM(0xC1390 / 0xC1391)` | two gain words, `1.3333 -> 0` and `0 -> 1.3333` | VERIFIED run (table) |
| `kVoiceMode`, `kLFODst`, `kLFOWave`, `kLFOClkEnable`, `kKbZone`, `kOctaveShift` | strip 0 words 36, 48..50, 57, 58; common 0 words 2, 3, 8; `DM(0xB00F2, 0xB00FE, 0xC13A6..A9)` | enumerations that speak only with the Q16.16 cell (`q16_only` in the CSV) | VERIFIED run |

The CSV has one row per (id, DSP word, destination) with the words listed in value order (`values` column: `lo-hi`, or the list when a value sent nothing).

### Plateaus and the displayed time

Rate index `i = knob - 64` against the display tables of `cbin/presets.rs` (`ENV_ATTACK_MS_TABLE`, `ENV_DECAY_MS_TABLE`): the attack ramp time `1 / cA(i)` passes at 21,701 passes per second (two frames at 43,402.34 Hz) equals the displayed time at knobs 0, 16, 32, 48, 64, 80, 96, 112 (ratios 1.000, 1.001, 1.005, 1.002, 1.003, 1.000, 1.001, 1.011); between them the rate table repeats a value for 5 or 6 indices while the display keeps rising (ratio 0.73 to 0.91 at the end of a plateau). The decay time constant `1 / (1 - a(i))` passes is `0.217 x` the display at the same knobs (0.169 .. 0.225 over a plateau) and `1 / ln 100 = 0.2171`: the display is the time to fall by 40 dB. Test `rate_tables_reproduce_the_displayed_times`.

## 4. Engine wiring (opt-in)

| Piece | Change |
|---|---|
| `synth/fw_knob_laws.rs` | the laws above as functions (`rate_index_for_knob`, `cutoff_word_for_knob`, `unit_word_for_knob`), `EnvKnobs` / `StripKnobs`, tests against the CSV for every knob |
| `SynthLayerDecoded::knobs` | raw amp and filter ADR knobs of a decoded program (not stored, not packed) |
| `StripEnvRecords::from_knobs` | records from the knob indices; replaces the search `attack_index(seconds)` / `decay_index(0.18 x seconds)` when `SynthLayer::knob_laws` is on and the layer carries knobs |
| `--synth-knob-laws` | `FirmwareOptions::synth_knob_laws`, off by default and in `--firmware-all` |

Not wired: the cutoff. The host feeds a cutoff in Hz through the display table, and the tangent reading `R = tan(pi fc / fs)` of the strip law is **not consistent** with the displayed Hz at all (s6), so changing the cutoff path needs the hardware sweep first.

## 5. Ids that stay silent or constant

25 of the 57 speaking ids send the same words in the idle state in both encodings: `kFltType`, `kFltEnable`, `kFltDrive`, `kOscBank / Cat / Wave`, `kOscUnison`, `kOscEnvDst / Vel`, `kLFOAmt`, `kVibAmt`, `kVibDly`, `kSmpId`, `kSmpIdPosStatus`, `kSmpDynamic`, `kSmpTimbre`, `kPStickEnable`, `kPitchStickRange`, `kSustainEnable`, the layer focus / enable ids. They are either enumerations that the harness settles (wave and category go to the stage word `0x0612xxxx`, one value per stage), or amounts that the ARM applies on the ARM side (vibrato and LFO depth), or need a loaded program (sample ids). A further 38 of the 95 ids send no message at all. The oscillator P block, the SHAPE word (`kOscCtrl` word 6 is the Osc Ctrl knob itself, [synth-validation](synth-validation.md) s1), ratios and weights are therefore **not** written by any of these ids: the P block stays EMPIRICAL (`synth/p_table.rs`) and is not replaced.

## 6. The tangent reading of `R` against the display (open, needs hardware)

The strip law is `R(i) = 0.36083 + 0.0079301 i` with `i = knob - 52` and `fc = fs atan(R) / pi` for the four-pole filters. At knob 52 that is 4.8 kHz, at knob 100 8.8 kHz, at knob 20 1.5 kHz, while the cutoff display shows 277 Hz, 4.4 kHz and 44 Hz. The difference is not a constant octave offset: `R` is linear in the knob and the display exponential, so no key-tracking or note term (one table index per semitone) can reconcile them. The Ladder24 stage reads `R` as a table position (exponential pole, [synth-stage-models-10](synth-stage-models-10.md)), and the other stages may also map `R` through tables the model replaced by the tangent. The reading stays open: sweep a hardware clip of a low-pass 12 / 24, band-pass and high-pass over the cutoff knob (OQ22). No default changes without it.

## 7. Clips

Re-scored in D57 (harness restored as `crates/nord-tools`, [synth-validation](synth-validation.md) s "Knob-law re-score"): the 15-clip mean falls from 4.03 to 3.96, three Part A clips move (`sweep_12db` 3.04 -> 1.97, `filter_bank` 4.37 -> 4.33, `sine_pluck` 6.40 -> 6.52), the other twelve are identical (their stored knob equals the table search, and the six lesson clips are layers built in code with no knobs). No clip gets worse by more than 0.2 dB in presence or RMS, so the D22 bar holds for the increment and `--synth-knob-laws` became the default (`--no-synth-knob-laws` turns it off). `render_hash` is unchanged.

## Implications

- Edit-capture tools must write `knob << 16`; the encoding is per id (`arm_edit_knobs.py` runs both).
- The ASSUMED pass length of the strip chain (2 frames) and the Hold flag are now VERIFIED; the decay time constant is `display / ln 100`.
- Cutoff, P block and wave program words are not closed by these ids.

## Follow-ups

1. ~~Re-run the clip set with `--synth-knob-laws` and flip per D22 if the bar holds.~~ Done in D57 (flipped).
2. Hardware sweep of the cutoff knob per filter type (s6).
3. Wire `kCmpResponse` (attack / release pairs) and the delay feedback filter words into `FirmwareFx` (s2 table).
4. A loaded-program capture for the silent ids of s5.

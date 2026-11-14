# Organ Engine Specification (`DSPO`)

## 1. Scope

Specifies the 2-layer (`Organ A`, `Organ B`) physical modeling Organ Engine and Model 122 Vintage Rotary Speaker simulation of the Nord Stage 4.

Implemented in [`crates/nord-core/src/organ/`](../../crates/nord-core/src/organ/):

| Module | Content |
|---|---|
| `mod.rs` | `OrganLayer` state, `note_on`/`note_off`, `render_frame` signal chain |
| `types.rs` | enums (`OrganModel`, `ScannerMode`, `RotarySpeed`, `ToneWheelSource`), gear table, rotary constants |
| `firmware_bank.rs` | `ToneBank`: PNO Core1 oscillator-bank arithmetic (92 wheels, mono bus output and the two firmware lanes), x table, `fracmul`, `phase_step`, `cycles_per_frame` |
| `tonewheel.rs` | gear frequencies, pickup calibration, 91-wheel generator (firmware or legacy source) |
| `drawbar.rs` | drawbar offsets, firmware level curve (`drawbar_gain`, `DrawbarLaw`, TD-10), foldback |
| `vibrato_bank.rs` | 12-TET ladder (`wheel_hz`, Fine Tune), 184-entry detune table, group gains, copy weights |
| `keyclick.rs`, `scanner.rs`, `rotary.rs` | key click, scanner vibrato/chorus, Model 122 rotary |
| `bank_golden.rs` | test-only golden vectors from `tools/organ_bank_ref.py golden` |

---

## 2. Supported Organ Models

| Model Enum | Panel Name | Synthesis Architecture |
|---|---|---|
| `B3` | **B3** | 91 phase-locked spinning tonewheels, harmonic foldback, key click, 2nd/3rd percussion, C1–C3/V1–V3 scanner vibrato |
| `Vox` | **Vox** | Vox Continental 12-oscillator top-octave divider (`16'`, `8'`, `4'`, `II`, `III`, `IV`, Sine + Reed/Triangle filter waves); opt-in firmware source `ToneWheelSource::Vox` (84 note oscillators, section 3.1.7) |
| `Farf` | **Farf** | Farfisa Compact transistor divider with multi-tab bandpass registers and high-boost brilliance |
| `Pipe1` | **Pipe 1** | Pure liturgical Principal/Flute pipe voicing with subtle wind chiff transient |
| `Pipe2` | **Pipe 2** | Full swell/great tracker organ registration with reed harmonics |

---

## 3. Hammond B3 91-Tonewheel Mathematical Model

A classical Hammond B3 tonewheel generator contains a synchronous AC motor driving 12 main shaft gears paired with 91 rotating iron tonewheels (`tw_index = 1..=91`).

### 3.1 Tonewheel Frequencies & Gear Ratios

Rather than ideal equal temperament ($2^{n/12}$), the 12 drive gears use integer tooth ratios approximating $2^{n/12}$ driven at 1200 RPM (20 Hz shaft reference):

| Semitone (Mod 12) | Note | Drive Gear Teeth | Driven Gear Teeth | Frequency Ratio |
|---|---|---|---|---|
| 0 | C | 85 | 104 | `0.81730769` |
| 1 | C# | 71 | 82 | `0.86585366` |
| 2 | D | 67 | 73 | `0.91780822` |
| 3 | D# | 105 | 108 | `0.97222222` |
| 4 | E | 103 | 100 | `1.03000000` |
| 5 | F | 84 | 77 | `1.09090909` |
| 6 | F# | 74 | 64 | `1.15625000` |
| 7 | G | 98 | 80 | `1.22500000` |
| 8 | G# | 96 | 74 | `1.29729730` |
| 9 | A | 88 | 64 | `1.37500000` ($A_4 = 440.0\text{ Hz}$) |
| 10 | A# | 67 | 46 | `1.45652174` |
| 11 | B | 108 | 70 | `1.54285714` |

Each octave doubles the number of teeth on the rotating wheel (`2, 4, 8, 16, 32, 64, 128`, with top-octave tonewheels `tw >= 84` using physical `192`-tooth wheels paired with a $4/3$ step-up gear). All 91 tonewheel phase accumulators advance continuously on every audio sample (`phase[i] = (phase[i] + 2*pi * f[i] / fs) mod 2*pi`), ensuring authentic phase interaction when multiple keys share a tonewheel.

### 3.1.1 Pipe organ bank parity (`ToneWheelSource::Firmware`, default for the models Pipe1 / Pipe2)

**Correction (model attribution, VERIFIED):** the bank was first assigned to the `B3` model. [organ-model-paths](../research/organ-model-paths.md) disproves that: the firmware installs, per `Edt_Org_kModel` (B3, Vox, Farfisa, Pipe1, Pipe2, B3 Bass = 0..5; chain step `0xC022F060`), the bank `0x122620` and its mix stage `0x12268F` for **Pipe1 and Pipe2 only** (they differ in one word, `DM(0x9594E)`: mix-stage input array `0xC24E5` or `0xC2655`, not modelled). B3, Farfisa and B3 Bass install no generator; Vox installs `0x12272D` + `0x1229B3` ([organ-post-stage](../research/organ-post-stage.md), not in Rust yet).

**Status:** the bank ALGORITHM, its pitch programming, the weights and the vibrato detune table are VERIFIED (SHARC oracle, ARM run of `0xC022D9F0`); the model to routine map is VERIFIED (run). ASSUMED: the model value order matches the UI order (the string pool order is the only evidence) and that the bank is the audible tone source of the pipe models. `ToneWheelSource::ModelDefault` (the default) resolves per model (`OrganModel::default_tone_source`):

| Model | Default source | Explicit `Firmware` | Explicit `Legacy` |
|---|---|---|---|
| Pipe1, Pipe2 | `Firmware` (bank; no B3 pickup calibration or crosstalk) | bank | gear-ratio sine |
| B3 (and B3 Bass mode) | `Legacy` (tone source not located) | bank, experiment only (attribution disproved) | gear-ratio sine |
| Vox, Farfisa | `Legacy` | falls back to `Legacy` (no 92-wheel bank) | gear-ratio sine |

The tone source of B3, Farfisa and B3 Bass is open ([OPEN-QUESTIONS](../OPEN-QUESTIONS.md)); `WeightMode` stays `Legacy` by default.

What would settle it: (1) ~~the ARM writer of the bank's weights~~ found ([organ-weights](../research/organ-weights.md): key-down / update of the four bank engines send `a`, `b`, `wB`; `wA` is DSP state; footage offsets, foldback and the drawbar curve VERIFIED), still open whether `Edt_Sys_kToneWheelMode` / `Edt_Sys_kStringResonanceLevel` drive them and what writes the engine's drawbar indices; (2) the piano-versus-organ program selector (which program runs the PNO Core1 state machine when an organ layer is active); (3) a running boot capture of the DSP data memory with an organ patch sounding.

Source: [organ-tonewheel-bank](../research/organ-tonewheel-bank.md) (oracle run, 0 mismatches over 29,440 words). For `OrganModel::B3` each wheel runs

| Step | Equation | Status |
|---|---|---|
| Step | `step = round(f / fs * 2^32)` (firmware: `fracmul(inc, tune)`, `f = fs * step / 2^32`) | VERIFIED relation; `f` is the 12-TET ladder of section 3.1.2 |
| Accumulate | `phase = (phase + step) mod 2^32` (`u32`), before the table read | VERIFIED |
| Index | `idx = phase >> 20`, 4096 entries, no interpolation | VERIFIED |
| Waveform | `x = sin t - sum_h 10^(dB_h/20) cos h t`, h = 2..5 at -19, -36, -44, -48 dB (coefficients 0.112202, 0.0158489, 0.00630957, 0.00398107), `T[i] = x(2 pi i / 4096)` built once in `new` | VERIFIED: least-squares fit of the firmware table, max error 7.8e-8, sine terms and h >= 6 below 1e-8 (so the phase origin is VERIFIED too); [organ-oracle-audio](../research/organ-oracle-audio.md) s4 |
| Role | the x wave on every B3 wheel, replacing the sine and the extra low-wheel harmonics | ASSUMED (about 70 % organ tone source) |

Every other model keeps the float-phase sine path; `Legacy` on `B3` is selected by setting `OrganLayer::tonewheel_source`. The pickup calibration and crosstalk stay `ASSUMED`. Tests: `firmware_bank::tests` (closed form at chosen steps, harmonic levels, `fracmul` rounding, detune table steps), `bank_golden::tests` (Rust bank vs the Python model, vectors from `python tools/organ_bank_ref.py golden`, synthetic table, no firmware data).

### 3.1.2 Pitch ladder, Fine Tune and vibrato detune (`vibrato_bank.rs`)

The ARM routine `0xC022D9F0(this, level)` programs the 184 increments from a table chosen by the organ Vibrato-Enable level ([arm-organ-messages](../research/arm-organ-messages.md) section 7, `tools/arm_bank.py`, `docs/research/data/arm-bank-increments.csv`). Element `j = 2 w + c`: wheel `w`, copy `c`.

| Item | Value | Status |
|---|---|---|
| Vibrato level not 1 | exact 12-TET table `bank0_CODE@0x444188`: copies identical, spread 0.00 cent, step 100.00 cent (max 0.055 cent from `2^(w/12)`, word quantisation); gains (setter idx 5..8) `0x7FFFFF, 0, 0x7FFFFF, 0` (flag 0) | VERIFIED |
| Vibrato level 1 | detuned table `@0x443EA8`: `DETUNE_CENTS[j]` (mean +0.11, rms 1.41, range -2.49..+2.46 cent; copies of a wheel differ 1.90 cent rms); gains `0x600000, 0x600000, 0x300000, 0x300000` (flag 0); flag-1 variants in the research doc | VERIFIED |
| Reading of the detune | vibrato / chorus ensemble effect, replacing the earlier constant `-0.945 / +0.945` cent pair (`COPY_SPREAD_CENTS`, removed) | ASSUMED |
| Pitch anchor | wheel 0 = MIDI 24 (C1), A4 = 440 Hz; the organ is equal tempered, not Hammond gear-ratio tuned (the gear table above applies to `Legacy` only) | ASSUMED |
| Fine Tune | `OrganLayer::fine_tune_cents`, multiplies every frequency by `2^(cents/1200)`; clamped to the firmware tune table range -400..+381 cent (the Fine Tune handler writes the bank rate `DM 0xC27C5`) | range VERIFIED, mapping ASSUMED |
| Copy mix | `OrganLayer::detuned_copies` selects the table. The mix stage applies the lane matrix `out_x = g5 tx + g8 ty`, `out_y = g6 ty + g7 tx` (`vibrato_bank::lane_matrix`, [3.1.3](#313-mix-stage-of-the-bank-oracle-chain)); the mono bus uses its mean: exact `[1, 0]`, detuned `[0.5625, 0.5625]` (+1.02 dB against exact for equal copies). `ToneBank::advance_lanes` gives both lanes (not routed to the stereo path) | matrix VERIFIED, mono mean is a modelling choice, int-to-float `raw / 2^23` ASSUMED |

Tests: `vibrato_bank::tests` (12-TET step 100 cent, A4 anchor, Fine Tune, table statistics and spot values from the CSV rows, weights, lane matrix, smoother), `firmware_bank::tests::detuned_copies_follow_the_cent_table`, `tonewheel::tests::fine_tune_retunes_the_firmware_bank`.

**Render change** (`render_hash`, 2,240,000 bytes): before this change (default `Legacy`) `25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a`; after (bank as the default of every `B3` layer, 12-TET pitches, firmware drawbar curve) `cb8d83b758cc3596ba52f1444b0e1f5d40726d27f36e53205464b47b414aacd0`; after the exact x-table levels of the oracle-audio change `d98808bd1597f99ffee7b247cb9785ab62a441941fb6b92312575ddbf73b7b75` (max sample difference 2.0e-6). Only the engine timeline (its organ layers) and the standalone organ segment change; synth, sample, piano and FX segments are bit-identical. **Model attribution fix:** with the bank moved from `B3` to Pipe1 / Pipe2 (`ToneWheelSource::ModelDefault`) the default `render_hash` returns to the pre-bank value `25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a` (bit-identical: the render dump uses the `B3` model and no Pipe layer).

- **Pickup Calibration (`B3` vs `B3 Bass`):** Standard `B3` tapers the bottom 12 complex wheels (`0.74..0.94`) and softens upper wheels above `tw 60` (`1.0..0.84`). `B3 Bass` boosts low wheels `0..24` (`1.32..1.05`) for prominent pedal/bass weight.

### 3.1.3 Mix stage of the bank (oracle chain)

The bank output (184 elements) goes through the PNO Core1 mix stage `0x12268F` (one record per set, four sets; [organ-oracle-audio](../research/organ-oracle-audio.md) s3, run by `sharc-organ`, `tools/organ_oracle_audio.py`). All items VERIFIED by poking the real routine in the SHARC oracle unless marked.

| Item | Value |
|---|---|
| Weights | `weight[i] = a[i] wB[i] + b[i] wA[i]`, `i` = 0..91 (92 wheels), applied to elements `2i` (copy 0) and `2i + 1` (copy 1); `a = DM 0x9594F + i`, `b = 0x95AE2 + i`, `wB = 0x957DE + i`, `wA = 0x9566E + i` (set 0; strides `0x65`, `0x64`, `0x5C` per set) |
| Lanes | `tx = sum weight[i] x[2i]`, `ty = sum weight[i] x[2i+1]`; `out_x = L (g5 tx + g8 ty)`, `out_y = L (g6 ty + g7 tx)`; `g5 = DM 0x959AF`, `g6 = 0x959B0`, `g8 = 0x95B3E`, `g7 = 0x95B3F`, `L = 0x95B42` |
| Smoothing | gains and level states move `s = 0.01 t + 0.99 s` per frame (steady level 0.999983 of the target) |
| Group gains | `(g5, g6, g7, g8)` = `(0x7FFFFF, 0, 0x7FFFFF, 0)` exact (both lanes = copy 0), `(0x600000, 0x600000, 0x300000, 0x300000)` detuned (`/ 2^23`: ASSUMED conversion) |
| Python model agreement | 1.6e-5 (exact) / 1.8e-5 (detuned) relative rms over 16,384 frames of a chord |
| Writer of `a`, `b`, `wB`, `wA` | ARM code not found; footage offsets, foldback and the drawbar curve as final gain stay ASSUMED |

Engine status: the 92 wheels exist in `ToneBank` (`BANK_WHEELS`); the layer feeds 91 to the drawbar buses, with the smoothed two-lane stereo path of section 3.1.4. Rust against oracle on a C4 E4 G4 chord with drawbars 888000000: partials within -0.19..+0.11 dB, SNR 37.7 dB (exact) / 37.6 dB (detuned) after one gain; residue = ASSUMED pickup calibration; overall level 0.0792x (ASSUMED `0.08` bus scale). Tests: `bank_mix_golden::tests`, `firmware_bank::tests`.

### 3.1.4 Vibrato-enable link, smoothed gains and stereo lanes (`vibrato_link.rs`, `lane_mix.rs`)

`OrganLayer::vibrato_link` selects between two behaviours (only layers whose tone source resolves to `Firmware` are affected: the Pipe models by default):

| `VibratoLink` | Table / gains | Mixing |
|---|---|---|
| `Firmware` (default) | vibrato-enable level 1 = detuned table + stereo-blend gains, any other level = exact table + copy-0-on-both-lanes gains (VERIFIED, `0xC022D9F0`). The level follows `scanner_mode` (`Off` = 0, any `V1..C3` = 1: ASSUMED, the ARM passes the layer vibrato-enable byte `(value == 1)`); `detuned_copies = true` forces level 1 | gains `g5..g8` and the level ramp with `s = 0.01 t + 0.99 s` (VERIFIED) per firmware frame; the two lanes drive the stereo path |
| `Manual` | `detuned_copies` only | previous behaviour: instant mono mean `copy_weights`, organ mono until the rotary |

- **Smoothing at the engine rate:** the VERIFIED coefficient is per firmware frame. The per-sample keep factor is `0.99^(F / fs)` with `F = FIRMWARE_FRAME_RATE_HZ` = 43,402.34 Hz (VERIFIED: the ARM's sample-rate literal, [dsp-frame-rate](../research/dsp-frame-rate.md)), so the time constant is 100 frames = 2.30 ms at any sample rate (`keep_for_rate`; 0.9909 at 48 kHz). Differences below `5e-5` snap to the target (the f32 fixed point sits about 7e-6 off).
- **Lanes:** `x = m00 a + m01 b`, `y = m10 a + m11 b` with the smoothed matrix `[[g5, g8], [g7, g6]] * level`; detuned steady state `x = 0.75 a + 0.375 b`, `y = 0.375 a + 0.75 b`. The lane split is done before the pickup/crosstalk stages (linear, so equivalent). While the table is exact and the ramp has settled only one lane exists and the old code path runs bit-exactly (render hash unchanged). Leaving level 1 keeps both copies running until the ramp settles, then retunes to the exact table.
- **Stereo routing:** `mid = (x + y) / 2` goes through key click, scanner and rotary exactly as the mono signal did; `side = (x - y) / 2` is added to L and subtracted from R after the rotary. `(L + R) / 2` therefore equals the `Manual` mono mean (test `mono_sum_equals_the_manual_mono_mean`); without rotary `L = x`, `R = y`.
- **Against the oracle** (`organ_ref --detuned` vs `organ_oracle_audio.py render --detuned`, `compare --lanes`, C4 E4 G4, 888000000, 43,156 Hz, the oracle-matching `organ_ref` default rate; not re-run with `F` = 43,402.34 Hz, which changes the smoother time by 0.6 %, first 2,048 frames dropped): lane x 37.7 dB, lane y 37.6 dB SNR after one gain; the old mono path against the same lanes gave 22.6 / 23.3 dB. Exact table: 37.7 dB on both lanes.
- **Per-wheel weights:** the mix stage also smooths the per-wheel weights (`wA <- a wB + b wA`, `a` = 0.0096 on key-down, 0.0043 on a full update, [organ-weights](../research/organ-weights.md)); implemented as the opt-in `WeightMode::Firmware`, section 3.1.5.
- Tests: `lane_mix::tests` (time constant, step response, both gain sets, level selection), `vibrato_link::tests`, `stereo_lanes::tests` (level/table selection, 0.75 / 0.375 blend, ramp, mono-sum equivalence, dual-mono when exact), `drawbar_sum::tests`.

### 3.1.5 Firmware weights (`WeightMode::Firmware`, `weights.rs`, `weight_state.rs`)

`OrganLayer::weight_mode` selects how the drawbar sum is formed (only `B3`-style drawbar buses; percussion and the other models keep their paths):

| `WeightMode` | Per-wheel gain | Scale | Pickup tables |
|---|---|---|---|
| `Legacy` (default) | instant `drawbar_gain * key gain` per key and drawbar | ASSUMED `0.08` bus scale | ASSUMED calibration on every wheel |
| `Firmware` | smoothed `wA[w]` of the ARM weights, below | VERIFIED `2^-23` per integer unit, level `L = weight_level` (ASSUMED 1.0) | none (the bank and mix stage have none); `b3_bass_mode` still applies its table |

Targets (VERIFIED, [organ-weights](../research/organ-weights.md) s3, s5; 128 notes x 9 drawbars bit exact):

```text
wheel(note, j) = fold(note + octave_shift - 24 + OFFSET[j])          OFFSET = -12, 7, 0, 12, 19, 24, 28, 31, 36
wB[w]          = 2^-23 * sum over held keys k and drawbars j with wheel(k, j) = w of  d7[level_j] * ((vel_k * 127) >> 4)
d7             = 0, 3, 6, 13, 22, 39, 62, 87, 127           (DrawbarLaw is ignored in this mode)
```

Chords add on shared wheels; there is no tapering (all taper bytes are 127). One key at velocity 127 on a drawbar at 8 gives `UNIT_WEIGHT = 0.0152607`. The velocity is `round(gain * 127)` clamped to 1..127 (`note_on_with_gain`; ASSUMED that the engine gain is `velocity / 127`, the organ velocity source of the firmware is open). Targets are rebuilt only when a key, the drawbars or the octave shift change.

Smoothing (VERIFIED coefficients and recurrence, [organ-weights](../research/organ-weights.md) s2, s4): per wheel and per firmware frame `wA <- a wB + (1 - a) wA` with `a = 0.0096406` (`0x3C1DF380`) on the wheels a key-down writes and `a = 0.0042715` (`0x3B8BF800`) on every wheel after a full update. At the engine rate the per-sample keep factor is `(1 - a)^(F / fs)` with `F` = 43,402.34 Hz (VERIFIED, as in 3.1.4), so the ramp lasts the same time at any rate (`weights::keep_for`). Differences below `1e-6` (-84 dB re one key) snap onto the target so a release ends in exact silence; the render early-out waits for the tail.

| Event | Targets | `a` |
|---|---|---|
| key-down | new targets at once | `0.0096` on the 9 wheels of that key until the next full update (ASSUMED persistence: in the firmware the update that follows the key-down resets it, the delay between the two is not known) |
| key-up | new targets | `0.0043` on all wheels (ASSUMED: the key-up handler was not read) |
| drawbar or octave change | new targets | `0.0043` on all wheels |

Output: `out_lane = L * sum_w wA[w] * bus_lane[w]` through the 91 wheel buses of 3.1.4 (the lane matrix and its smoothing are unchanged), times the existing B3 Normal-percussion `0.708` dip. Percussion keeps its ASSUMED gains scaled by `UNIT_WEIGHT * L / 0.08` so the drawbar / percussion balance is as in `Legacy`; the key click is not scaled (`key_click_enabled` switches it off for oracle comparisons).

Against the oracle (`python tools/organ_weight_compare.py`, ARM key-down / key-off messages poked into `sharc-organ` with the mix smoothers started on their targets, `organ_ref --weights firmware`, C4 E4 G4 at 888000000, 43,156 Hz): level 1.0000x on both lanes, attack and release envelope within 0.1 % of peak, steady SNR 44.5 dB (57 dB in the first 2,000 frames; the residue grows with time: bank phase drift of the 12-TET ladder against the exact increment words, not the weights). `Legacy` was 0.0792x and 37.7 dB against the same chord with unit weights. The absolute loudness of `Firmware` is `0.0153` per key and drawbar at `L = 1`, 14.4 dB below `Legacy` (`0.0153 / 0.08`): `L` (`0x95B42`) and anything downstream of the mix stage are unknown, so the default stays `Legacy` and `weight_level` is the free parameter. Tests: `weights::tests` (formula, curve, velocities, chords, step responses of both `a`, rate independence), `weight_state::tests`, `weight_tests` (all 128 x 9 entries of `organ-key-wheel-map.csv`, oracle partials, velocity ratio 508 / 1008, release, attack ramp, mode switch), `tools/tests/test_organ_weight_compare.py`.

### 3.1.6 B3 generator wheels (`ToneWheelSource::B3Wheels`, `b3_wheels.rs`)

Opt-in tone source of the `B3` model, from the PNO core 1 generator `0x121BBE` ([b3-generator](../research/b3-generator.md), [b3-tone-source](../research/b3-tone-source.md)). One wheel per MIDI note (wheel `w` = note `24 + w`), `u32` phase, per-note table with linear interpolation.

| Item | Law | Tag |
|---|---|---|
| Phase step per DSP call | `((PHASE_INC[n % 12] * 0x3F000000 + 2^30) >> 31) << (n / 12)`, `PHASE_INC` = `DM 0x95172..0x9517D` | VERIFIED (81 oracle-fitted ring frequencies, max relative error 2.5e-11; `tools/organ_b3_gen.py scan`, test `steps_match_the_oracle_fits`) |
| Ladder | 12-TET to 0.94 cent (the fifth is 1.4983, not 1.5) | VERIFIED |
| Hz anchor | every C exact 12-TET, `A4 = 440` Hz (`note_hz`); the firmware's own pitch (`firmware_note_hz`: ARM fine tune word `0x3FA02A00`, 2 frames per call, `F` = 43,402.34 Hz) is note 60 = 261.652 Hz (+0.18 cent) and within 1.3 cent of 12-TET over 24..115. The old 43,829 vs 43,156.4 Hz split was the fine tune word, [dsp-frame-rate](../research/dsp-frame-rate.md) | VERIFIED |
| Wheel tables | notes < 40: `DM 0xB314B`, 40..47: `0xB31CC`, 48 and up: `0xB324D`, as 12-harmonic fits (reconstruction error 4.2e-3, 6e-7, 7e-7) | VERIFIED (oracle series equal `A * table`, residual 3e-8) |
| Per-wheel amplitude `A` (`B3_WHEEL_GAIN`, `b3_keys.rs`) | ARM key-down float at `DM 0x95122 + note - 36`, wheels 12..88 (`fold(key - 24 + footage offset)` into 12..88 by octaves); first write per wheel is constant over keys | VERIFIED (44 wheels, [keydown-b3-wheels.csv](../research/data/keydown-b3-wheels.csv), test `gain_table_matches_the_captured_first_writes`), EMPIRICAL elsewhere |
| Footage weight `W(key, footage)` (`KEY_W`, `b3_keys.rs`) | `DM 0x951F1 + partial note - 36` per footage item, `d7 * taper`, columns 16' / 5 1/3' / 8' / 4' for 24 captured keys ([b3-key-weights.csv](../research/data/b3-key-weights.csv), `tools/organ_b3_keys.py`); linear in the note between keys; no velocity | VERIFIED (24 keys), EMPIRICAL between, ASSUMED for footages 2 2/3' and up (the 4' column) |
| Partial of a held key | `A[wheel] * W(key, footage) * drawbar_gain(level) * table(phase)`, summed over keys (`b3_weights.rs`, 2 ms weight ramp ASSUMED), times `DRY_GAIN` = 1.0 (`DM 0xB44C0`, no ARM writer found); percussion keeps the `0.12 / 0.08` legacy gain on the 4' weight; active on `B3` when not in B3 Bass mode | factors VERIFIED, dry gain and percussion ASSUMED |

Processor tail (`b3_tail.rs`, [organ-b3-tail](../research/organ-b3-tail.md)): the chain `0x121E2B` after the ring words is linear and time invariant; its amplitude response `G(f)` (+7.7 dB at 65 Hz, +8.2 dB at 131 Hz, +6.9 dB at 262 Hz, +4.4 dB at 440 Hz, -2.2 dB at 1 kHz, -10.8 dB at 5 kHz, -15.7 dB at 10 kHz) is a 34-point dB table measured with the oracle on the **full ARM boot log** ([b3-tail-gain.csv](../research/data/b3-tail-gain.csv), `tools/organ_b3_proc.py`, 6 key triples; [b3-tail-bootstate](../research/b3-tail-bootstate.md); the earlier 83-point table was measured without 21,000 boot words and was 7 to 26 dB lower); lines of different notes at one frequency agree within 0.1 dB; halving `W` leaves `G` unchanged), applied by scaling every harmonic of every wheel table by `G(h f0)` (EMPIRICAL; phase not modelled). Valid for `DM(0xB44C0) = 1.0`.

Oracle chords on the corrected state ([b3-tail-bootstate](../research/b3-tail-bootstate.md) s2): C4 E4 G4 at 8888 and C4 E4 A5 at 8888 every line within 0.54 dB (fundamentals within 0.13 dB); C3 C#4 E6 every line within 0.9 dB except wheel 36 h2 on wheel 48 (-5.55 dB) and wheel 49 (-1.72 dB), the start-phase lines. Superseded earlier state (kept for the record, [organ-b3-tail](../research/organ-b3-tail.md) section 4): C4 E4 G4 at 8888 10 of 13 line frequencies within 1 dB (rms 0.75 dB, max 1.5 dB; before the tail +12.7 to +31.8 dB, of which 8.2 dB was a replay artefact); C4 E4 A5 8 of 13; C3 C#4 E6 6 of 11 (wheel 36 h2 on wheel 48 is a start-phase line, +6.3 dB); a single-footage chord 2 of 2 (rms 0.53 dB). Remaining errors are multi-write `A` words of the 5 1/3' and 4' items, the start phase of coincident lines and the interpolated `W` of key 67. Hardware clip (`organ_b3`): with the weights only RMS gap +14.9 to +3.1 dB, clipping 10.3 % to 0.022 %, score 4.76; with the tail of the earlier state the mid band was 11.9 dB low and the score 8.07; with the corrected tail the mid band is -1.1 dB but RMS is +8.0 dB and the score 5.54 against 4.83 for the default, so the source stays opt-in (D22, [b3-tail-bootstate](../research/b3-tail-bootstate.md) s3).

Not modelled (why it stays opt-in): the phase and transients of the tail, the multi-write `A` words, the taper bytes of the footages 2 2/3' and up, the percussion on the shared wheels (the key-down's fourth write is not a drawbar item in the oracle chord), the scanner line box. The drawbar bus of the legacy path is replaced on this source by the weight sum; key click, scanner and rotary run unchanged on it with a flat pickup (1.0, no `pickup_calibration`). The default render hash is unchanged (`render_hash`: 25baa387b0219c08 before and after).

### 3.1.7 Vox note bank (`ToneWheelSource::Vox`, `vox_bank.rs`, `vox_shaper.rs`, `vox_output.rs`, `vox_source.rs`, `vox_layer.rs`)

Opt-in tone source of the `Vox` model (firmware `Edt_Org_kModel = 1`), the PNO core 1 post stage `0x12272D` and output stage `0x1229B3` ([organ-post-stage](../research/organ-post-stage.md), [organ-model-paths](../research/organ-model-paths.md), [pno-full-pass](../research/pno-full-pass.md)). It is not a tonewheel bank: 84 note oscillators with a built-in pitch LFO, a waveshaper and a per-note low-pass, summed with per-key weights through three 2-tap sections. `ToneWheelSource::Vox` applies to `OrganModel::Vox` only (other models fall back to their default); `ModelDefault` on `Vox` stays `Legacy`. Everything is allocated in `OrganLayer::new()`; `render_frame` runs 84 oscillators, one table lookup and one one-pole each per sample, with no allocation or lock.

Per firmware frame (`F = 43,402.34 Hz`, VERIFIED, `lane_mix.rs`; the engine converts with `scale = F / fs`: phase steps and the LFO scale, the one-poles and sections are redesigned at the engine rate with the cutoff in Hz unchanged):

```text
lfo      phase += 0x905A3;  tri = clip(4 (|s| - 0.5), 1);  tri' = clip(tri - 0.08, 0.91) + 0.08
m        = 0.5 (fine tune factor) + 0.5 depth tri'                 (depth = DM(0xE20E3), pitch fraction)
note k   ph += m rate_k,   rate_k = 0x62C2E7 2^(k/12)              (k = 0..83, oscillator k = MIDI k + 24)
         x = clip((|s| - 0.5) coef_k, 1),  coef_k = 128.8 2^(-k/12),   A = T[fix(x 512)]
         k in 12..47: A *= (A > 0 ? 1 : 0.9 + 0.2 s)               (s = signed phase -1..1)
         Y = c (A + A') + (1 - 2c) Y'                              (bilinear one-pole, c per group of 4 notes)
output   T = sum_k w_k Y_k;  3 sections (DC blocker, two low-passes);  y = out_3 g1 0.090075
```

| Item | Value | Tag |
|---|---|---|
| Structure (LFO, ladder, fold, clip, table, asymmetry, one-pole, 3 sections, `0.090075`) | bit-exact SHARC model `tools/organ_post_ref.py`; the Rust code matches it on closed-form tables (`vox_golden.rs`, max error 2.1e-5 (rms 1.6e-6) plain and 2.6e-4 (rms 2.2e-5) with vibrato over 48 records of 5 and 3 notes, test `golden_note_signals_match_the_python_model`) | VERIFIED (structure) |
| `rate_k`, `coef_k` | image values, closed forms within 1.8e-6 relative | VERIFIED |
| Waveshaper `T` | band-limited step, Blackman windowed sinc integral, `a = 170.58`, shift `-0.4638` (`vox_shaper.rs`, regenerated, no firmware table; rms 1.7e-4 against the image, `tools/organ_shaper.py`) | EMPIRICAL |
| One-pole cutoff per group of 4 notes | 21 ratios `fc / f0` (1.9119 ... 1.7108, 4 digits) rebuilt from the image coefficients, within 4.1e-5 relative; the pattern repeats per octave with three steps and is not a closed form | EMPIRICAL |
| LFO | `0x905A3` per frame = 5.94 Hz at `F` (the note in organ-post-stage quotes 5.96), per engine rate in Hz unchanged; both SIMD lanes share one LFO, start phase `0x80000000` (the image word) | VERIFIED |
| Start phases | the 84 note phases start at the image words `DM(0xE2B14 + k)` (`vox_image.rs`, `tools/organ_vox_image.py`); nothing resets them at key-down (no ARM message writes them), the oscillators free-run | VERIFIED (image, scan of the key-down, key-up and install scripts), hardware phase at a key press ASSUMED |
| Vibrato depth | `V1/C1 = 0xE464 << 8`, `V2/C2 = 0x1782D << 8`, `V3/C3 = 0x240B7 << 8` of `2^31` = 0.70 %, 1.15 %, 1.76 % peak pitch (the ARM words); the mapping from `scanner_mode` is ASSUMED (the Hammond scanner is bypassed on this source) | VERIFIED (words), mapping ASSUMED |
| Fine Tune | `m` scaled by `2^(cents / 1200)` (`DM(0xE20E0)` offset) | VERIFIED (law), cents mapping ASSUMED |
| Output sections `(a0, a1, b0, b1)` | `(0.9891, -0.9944, 0.9944, 0)` high-pass, `(0.9514, 0.0305, 0.0161, 0)`, `(0.8348, 0.1035, 0.0546, 0)` low-passes (corners about 340 Hz and 1.1 kHz at `F`); redesigned at the engine rate by the bilinear transform of their analog equivalent | VERIFIED (image values), rate conversion mine |
| Gains | `g1 -> 1.0`, `g2 -> 0` (0.001 per frame), gain word 1.0; start state settled | targets VERIFIED, word and start ASSUMED |
| Key to row map, weights (`vox_keys.rs`) | a held key `n` drives the rows `fold(n - 36 + I[slot])` (`fold` by octaves into 0..95, `I = [0, 7, 12, 19, 19, 24, 24, 28, 31, 31, 36, 36, 36, 40, 40, 43, 48, 48, 52, 60]`) of the four default level slots 0, 2, 5, 10 (16', 8', 4', 2') with weight `2076180 / 2^23` = 0.2475 each, summed where rows fold together, times `weight_level`, no velocity, applied as the firmware sends them: instant steps on the key-down timeline of `vox_timeline.rs` (16' at tick 3, 8' and 4' at 17, 2' at 47 after the key-down; key-up 2' at 1, 8' and 4' at 7, 16' at 37; tick 1 ms ASSUMED; [organ-vox-click](../research/organ-vox-click.md)), no legacy key click on this source; the 8' slot lands on oscillator `n - 24` | VERIFIED (all 20 flags of 24 keys, [keydown-vox-flags.csv](../research/data/keydown-vox-flags.csv), test `flag_table_matches_the_capture`; weights `0x3E7D70A0` in [keydown-organ-keys.csv](../research/data/keydown-organ-keys.csv)); drawbar index 0..6 -> slots `{0}, {2}, {5}, {10}, {3, 13}, {8, 11, 16}, {15, 17, 18, 19}` with the level `[0, 629145, 838860, 1048576, 1258291, 1468006, 1677721, 1887436, 2076180][v] / 2^23` (`vox_keys::levels_from_drawbars`, the layer's drawbars drive it; drawbars 8 and 9 write no level) | VERIFIED (listing and image table, [organ-vox-click](../research/organ-vox-click.md)); the ARM call path of the edit was not run |
| Output gain word | `DM(0x95FD1) = 0x3F1EB100` (0.61989), written by the ARM boot, multiplies the output (`vox_source::OUTPUT_LEVEL`); found by bisecting the boot words (the install alone leaves 1.0) | VERIFIED (default program); it is the swell word of `Fnc_Swell = 100` (`520 v^2 2^-23`, [organ-output-law](../research/organ-output-law.md)), not the layer level |
| Sine bank (36 sines, 12 buckets, rows 84..95, `vox_sines.rs`) | `ph_j += B (1 + mod) L_j`, `B = 0x7572967D`, `L_j = 0x1AE89F99 2^(j/12)` (folded 0.75 for j 24..31, 0.63 for 32..35), gains `1 / 0.1718 / 0.1660 / 0.0841` per octave group, polynomial sine `u g p(u^2)`, bucket `j`, `j - 12`, `j - 24`, `j - 32`; weighted by the key rows 84..95. Golden max 1e-3 (`vox_golden.rs` `B_PLAIN`); chord with key 90: waveform SNR 39.9 dB against the oracle. At the image base the sines run at 4.2 to 19.5 kHz and are attenuated 50 dB or more by the output sections (not pitch carriers) | VERIFIED (golden, run) |
| Absolute pitch | oscillator 0 is 32.7032 Hz at `F` = 43,402.34 Hz (C1 + 0.0001 cent, the word is `round(C1 / F * 2^33)`); the "9.9 cent flat of the Pipe bank" of `pno-full-pass` s4 came from reading the Pipe bank at unity tune (it is on C1 at the ARM's Fine Tune word, [dsp-frame-rate](../research/dsp-frame-rate.md) s3.6) | VERIFIED (frame rate); key map ASSUMED |

Not applied on this source: drawbars (the slot map is decoded, the value law is not), percussion, the Vox "Tone" morph of `render_frame` step 3, crosstalk and pickup tables. The key click and the rotary speaker are unchanged.

Comparison with the oracle chord (C4 E4 G4, weight 1 on oscillators 36, 40, 43, `pno-pass-install-vox.script` + `pno-pass-vox-weights-c4e4g4.script`, 20,000 passes; Rust `organ_ref --mode vox --vibrato N --rate 43402.34375` (`--rate 43156.4` was used before the frame rate was settled; the table below was not re-run); `python tools/organ_vox_compare.py ORACLE.npy RUST.csv`, partial amplitudes with a Hann window at the exact 12-TET frequencies, skip 4,000 frames, 32,768 analysed; table of the last run in [organ-post-stage](../research/organ-post-stage.md) section 8):

| Quantity | Result (Rust against oracle) |
|---|---|
| Fundamentals C4 / E4 / G4, absolute level | -0.001 / -0.002 / -0.002 dB (no vibrato); -0.002 dB at V1 and V3 |
| Harmonics 2..9 relative to the fundamental, no vibrato, above -60 dB (20 partials) | max 1.0 dB, rms 0.23 dB; the largest differences are in partials below -60 dB (6th of C4 -3.9 dB at -64 dB) |
| V3 (depth 1.76 %), above -60 dB (10 partials) | max 0.65 dB, rms 0.23 dB |
| V1 (0.70 %), above -60 dB (13 partials) | max 4.0 dB (4th of E4 at -54 dB), rms 1.1 dB |
| First vibrato sidebands, V3, oracle against Rust, relative to the carrier | C4 0.458 / 0.432 against 0.457 / 0.433; E4 0.611 / 0.576 against 0.610 / 0.577; G4 0.781 / 0.732 against 0.780 / 0.733 |
| First vibrato sidebands, V1 | C4 0.168 / 0.160 against 0.167 / 0.161; E4 0.213 / 0.203 against 0.212 / 0.204; G4 0.256 / 0.244 against 0.255 / 0.244 |

With the image start phases three oracle chords (C4 E4 G4; A2 E3 A3 C#4; C3 G3 C4 F4, 12,000 passes, `tools/organ_vox_chord.py`, `tools/organ_partials.py`) are within 0.34 dB on every line above 1e-4 (fundamentals 0.04 dB), including the rows 43 and 55 that were 3.4 / 2.1 dB off with zero start phases; a fourth chord with key 90 (sine bucket row) has a steady-state waveform SNR of 39.9 dB ([organ-vox-start-state](../research/organ-vox-start-state.md) section 2). Video check (`emu_vs_hw.py`, Twin amp default, key-down timeline and drawbars of the program, no key click): crest 16.1 dB against 16.1 dB (solved: it was the legacy click), but mid +7.8 dB, presence -7.3 dB, brilliance -9.5 dB re RMS and RMS -13.7 dB (an organ output level stage of about +14 dB is missing between the DSP organ and the FX chain), so `ModelDefault` on `Vox` stays `Legacy` (D22: oracle criterion met, video criterion not).

Comparison with the key-down weights (`keydown-vox-c4e4g4.script`: 12 rows 24, 28, 31, 36, 40, 43, 48, 52, 55, 60, 64, 67 at 0.2475; `tools/organ_partials.py vox`, absolute dB, no normalisation, [organ-keydown-weights](../research/organ-keydown-weights.md) section 3): fundamentals of rows 24, 28, 31 +0.02 dB; 31 of 39 lines above 1e-4 within 1 dB; the other 8 lines are two coincident frequencies (rows 43 and 55, where C4 harmonics land on G4 fundamentals) at -3.4 and -2.1 dB; G4 alone matches within 0.14 dB on every line, so the difference is the relative start phase of free-running oscillators (Rust 0, oracle the settled image state, ASSUMED), not the weights. Hardware clip (`organ_vox`): score 7.66 to 3.44 (Legacy default 5.68), centroid -921 to -308 Hz, presence and brilliance -8 to -2.4 dB. Not yet verified end to end (phase-dependent lines, sine bank, footage edits), so `ToneWheelSource::Vox` stays opt-in (D22).

The odd harmonics are far below the unfiltered single-note values of organ-post-stage section 5 (3rd -22.7 dB against -16.4 dB for C4) because the output sections low-pass the sum (corners about 340 Hz and 1.1 kHz). The Rust chord is mono; the oracle `L` and `R` lanes of the full pass carry the same partials.

### 3.1.8 Farfisa divider (`ToneWheelSource::FarfisaWheels`, `farfisa_wheels.rs`)

Status: the measured per-sum line gain of the processor is in `farfisa_proc_gain.rs` (research s8, not wired); the generator is ported; the processor `0x122B85` is not (its key-down inputs and the oracle chords are in [organ-farfisa-bass](../research/organ-farfisa-bass.md) s7). The source stays opt-in: the oracle criterion (<= 1 dB per line) cannot hold without the processor, and no video check was run.

Opt-in tone source of the `Farfisa` model, the PNO core 1 generator `0x122A71` ([organ-farfisa-bass](../research/organ-farfisa-bass.md)). It is a free-running top-octave divider, not a tonewheel bank, but its 98 outputs are one signal per note, so it plugs into the 91-wheel bus: wheel `w` = note `24 + w`, oscillator `n` (semitone 0..11), row `r` = note `12 + n + 12 r` (8 rows, 9 for `n < 2`). Row 0 (notes 12..23) is below the bus and dropped; wheels 86..90 (notes 110..114) have no source and stay silent. `ToneWheelSource::FarfisaWheels` applies to `OrganModel::Farfisa` only (other models fall back to their default); `ModelDefault` on `Farfisa` stays `Legacy`. All state is in `OrganLayer::new()` (12 oscillators x 9 rows of `f32`); `advance` has no allocation or lock.

```text
phase_n += step_n                        u32, 1 cycle = 2^32, step_n = (inc_n >> 1) rho fine,  inc_n = round(3236192 2^(n/12))
phase_(r+1) = 2 phase_r + 0xAAAAAC11     wrapping
u = (|phase_r / 2^31| - 0.66666645) slope_(n,r)         slope_(n,r) = -1327.1669 2^-r 2^(-n/12) / rho
y = max(b clip(u) + a y_prev, clip(u))   a = exp(-0.0047343 2^r 2^(n/12) rho)  (r <= 6),  2^11 for r = 7,  0 for r = 8;  b = 1 - a
```

`rho = 43,402.6 Hz / sample rate` keeps the pitch (12-TET at `A = 440` Hz) and the edge and release widths in samples; Fine Tune scales `step`.

| Item | Law | Tag |
|---|---|---|
| Structure (the LFO is excluded: the image depth is 0), ladder, offsets, edge, release | bit-exact SHARC model `tools/organ_farfisa_ref.py` (4 random scenarios and 40 consecutive image-state calls, 0 differing words); the Rust code matches the oracle ring over 60,000 frames to max 4.0e-5, SNR 104.5 dB, spectral error 0.00 dB (`python tools/organ_farfisa_oracle.py compare`) and the Python golden vectors (`farfisa_golden.rs`) to 2e-4 | VERIFIED (structure) |
| `inc_n`, `S`, `off`, `slope` | closed forms of the image tables (1 unit, exact, exact, 1e-7) | VERIFIED (fit) |
| `a_(n,r)` | `exp(-0.0047343 ...)`, rows 7 and 8 as above; error 1e-6 | EMPIRICAL (fit) |
| Frame rate 43,402.6 Hz | puts oscillator 0 row 0 on C0 = 16.3516 Hz; the Vox offset (-9.9 cent against the Pipe bank rate) agrees | ASSUMED |
| LFO (rate `DM 0xC27D6`, depth `0xC27D7`, the Farfisa vibrato) | modelled in Python, not in Rust (the depth is an ARM word with unknown mapping) | VERIFIED in Python, not ported |

Not modelled (why it stays opt-in): the processor `0x122B85` (four weighted sums of the ring, filter sections, lattice, gains; every coefficient is an ARM word that no captured burst writes, so the oracle pass is silent), the Farfisa vibrato, the per-key weights. The existing drawbar bus, percussion and rotary run unchanged on the wheel outputs. Default render hash unchanged (`25baa387b0219c08`). Tests: `farfisa_wheels::tests` (steps, wheel map, pitch, golden rows, release, sample rate, Fine Tune), `farfisa_source_tests` (resolution, host-rate pitch, Fine Tune, determinism, silent top, other models unchanged), `tools/tests/test_organ_farfisa_ref.py`, `test_organ_farfisa_oracle.py`, `test_organ_fb_pass.py`.

### 3.1.9 Output level law (`output_level.rs`, `OrganLayer::output_level`)

One gain stage for every firmware organ path (`Firmware` weights, `B3Wheels`, Vox); research [organ-output-law](../research/organ-output-law.md).

```text
out = K * swell_word(pedal) * layer_level^2 * bank_output          (bank_output at swell 1, level 1)
swell_word(v)   = 520 v^2 * 2^-23   (v = 127: 0x7FFFFF * 2^-23)    VERIFIED  sender 0xC0223B90, boot word 0x3F1EB100 at v = 100
layer_level(v)  = (v / 127)^2       (0 mutes)                      VERIFIED  handler 0xC011AB8C (disassembly)
K               = 2.8 (+8.9 dB)                                    EMPIRICAL three clips, firmware location open
```

| `OutputLevel` | Behaviour |
|---|---|
| `ModelDefault` (default) | `Firmware` for Pipe1 / Pipe2 on the firmware bank (`WeightMode::Firmware` weights, `K`, squared layer level in the engine), `Legacy` for every other source |
| `Legacy` | the built-in levels: Legacy drawbar bus 0.08 (`LEGACY_BUS_OVER_UNIT` = +14.39 dB over the ARM weight unit), the Vox boot swell word 0.6199, layer level linear |
| `Firmware` | `K` on the weight mix / `B3Wheels` sum, the Vox word replaced by `K` (the engine pedal is the swell), layer level squared (`Stage4Engine::render_frame`) |

`ns4 render-patch --organ-level default|legacy|firmware`, `FirmwareOptions::organ_level`. The program rotary drive enters the engine as `preset / 10` (the firmware drive is `v / 127`, VERIFIED, [fx-type-map](../research/fx-type-map.md) s9); `export` multiplies by 10.

### 3.2 9 Drawbar Harmonic Bus Matrix & Foldback

Each key routes 9 contact switches to the 9 drawbar buses (`0..=8` level steps). The level law is the firmware curve (`drawbar::drawbar_gain`, `DrawbarLaw::Firmware`): the edit value `0..=8` maps through the integer table `bank0_CODE@0xC02B0EC0` to the 7-bit values `0, 3, 6, 13, 22, 39, 62, 87, 127`, i.e. `0` (off), -32.5, -26.5, -19.8, -15.2, -10.3, -6.2, -3.3, 0 dB re full scale (VERIFIED table, [arm-organ-messages](../research/arm-organ-messages.md) section 7; that `value / 127` is the final DSP gain is ASSUMED, TD-10). The old `level / 8` stays selectable as `DrawbarLaw::Linear`. `0` = $-\infty\text{ dB}$:

| Drawbar | Footage | Semitone Offset ($\Delta k$) | Foldback Behavior |
|---|---|---:|---|
| 1 | `16'` | `-12` | Keys `C1..B1` fold up $+12$ semitones on standard manuals |
| 2 | `5 1/3'` | `+7` | Normal |
| 3 | `8'` | `0` | Fundamental |
| 4 | `4'` | `+12` | Normal |
| 5 | `2 2/3'` | `+19` | Top notes > TW #91 fold down $-12$ semitones |
| 6 | `2'` | `+24` | Top notes > TW #91 fold down $-12$ semitones |
| 7 | `1 3/5'` | `+28` | Top notes > TW #91 fold down $-12$ semitones |
| 8 | `1 1/3'` | `+31` | Top notes > TW #91 fold down $-12$ semitones |
| 9 | `1'` | `+36` | Top notes > TW #91 fold down $-12$ semitones |

### 3.3 Harmonic Percussion (Single-Triggered)

- **Harmonic Switch:** `2nd` (4' bus, $+12$ semitones) or `3rd` (2 2/3' bus, $+19$ semitones).
- **Decay Switch:** `Fast` ($\tau \approx 180\text{ ms}$) or `Slow` ($\tau \approx 550\text{ ms}$).
- **Volume Switch:** `Normal` ($0\text{ dB}$ percussion boost, $-3\text{ dB}$ main drawbar attenuation) or `Soft` ($-6\text{ dB}$ percussion level, $0\text{ dB}$ drawbar attenuation).
- **Single-Trigger Logic:** Percussion fires only when transitioning from 0 keys held on the layer to $\ge 1$ key held; legato playing does not re-trigger the percussion envelope until all keys in the layer are released.

### 3.3.1 POLY percussion, Preset / Drawbar Live, shared vibrato (manual p.20-21, OS v1.6x)

All three are opt-in; defaults reproduce the previous render bit for bit (`render_hash` and the golden tests are unchanged).

| Feature | Engine field / method | Behaviour | Status |
|---|---|---|---|
| POLY percussion (Shift+Volume) | `OrganLayer::percussion_poly` (`percussion_poly.rs`) | Every new key stroke starts its own percussion envelope (per key, 128 floats, no allocation); the percussion partial of each held key is scaled by its own envelope (`perc_env_for`). Single mode (default) keeps the layer-wide envelope. Decay times and gains are the existing ones. Test `only_poly_mode_gives_the_second_note_a_fresh_percussion_attack` | manual behaviour VERIFIED (p.20), decay/gain ASSUMED as in 3.3 |
| Preset vs Drawbar Live | `OrganLayer::drawbar_live`, `live_drawbars: [u8; 9]`, `active_drawbars()`, `set_live_drawbar`, `sync_preset_to_live` (`preset_live.rs`); `Stage4Engine::sync_organ_preset(LayerId)` | Preset on (default): the stored `drawbars` drive the sound. Live: `live_drawbars` (the physical bars) drive it, `drawbar_morph_applicable()` is `false`. SYNC copies live into the stored preset. Global drawbar MIDI CCs always move `live_drawbars` and edit the stored `drawbars` only outside Live mode | manual behaviour VERIFIED (p.21); "morph does not apply in Live" ASSUMED |
| Shared Vox / Farf vibrato | `Stage4Engine::organ_shared_vibrato`, `apply_organ_sharing()` (`engine/organ_modes.rs`, run by `process_block`) | When on and both layers are Vox (or both Farfisa), Organ B takes Organ A's `scanner_mode`. B3 and Pipe keep per-layer vibrato on/off | sharing rule VERIFIED (p.20-21); layer A as source ASSUMED (hardware check: load a program with different stored values per layer) |

Not modelled: the live Percussion and Vibrato On/Off buttons in Drawbar Live mode (one stored value per layer), and the manual's "percussion only for the B3 model" (the engine applies percussion on every model since before this change; gating it would alter existing renders, see `docs/OPEN-QUESTIONS.md` candidates). B3 Bass mode (`b3_bass_mode`) uses the legacy drawbar sum; poly percussion and Live drawbars act on it and are covered by tests (`poly_with_b3_bass_and_every_model_stays_finite`, `live_mode_works_for_b3_bass_and_firmware_weights`). Sources: manual p.20-21 only; no web sources used.

### 3.4 Key Click & Crosstalk

- **Key Click:** On note-on and note-off, a short $2.5\text{ ms}$ bandpass-filtered impulse burst ($f_c \approx 1.8\text{ kHz}$) simulates physical palladium contact bounce.
- **Tonewheel Crosstalk:** Adjacent pickup coils bleed at $-42\text{ dB}$ (`vintage_level` scalable), adding subtle warm sub-harmonics.

---

## 4. Model 122 Vintage Rotary Speaker Simulation

Firmware strings confirm the dedicated `LeslieHorn` and `Rotary Mic Distance` parameters inside `DSPO`.

### Signal Flow

1. **Tube Overdrive Preamp:** Asymmetric soft-clipping waveshaper (`tanh(drive * x) + 0.1 * x^2`).
2. **800 Hz Linkwitz-Riley Crossover (`ROTARY_CROSSOVER_HZ`):** Verified against `ARM0` (`0x0db130` / `0x0e0098`), splits the signal into the **Treble Horn** ($\ge 800\text{ Hz}$) and **Bass Rotor Drum** ($< 800\text{ Hz}$), with `ROTARY_BASS_HORN_BALANCES` (`70/30 .. 30/70` from OS v1.66 `0x2c20d8`).
3. **Independent Rotational Ballistics:**
   - **Slow (Chorale):** Horn = $0.80\text{ Hz}$ ($48\text{ RPM}$), Bass Drum = $0.67\text{ Hz}$ ($40\text{ RPM}$).
   - **Fast (Tremolo):** Horn = $6.85\text{ Hz}$ ($411\text{ RPM}$), Bass Drum = $5.70\text{ Hz}$ ($342\text{ RPM}$).
   - **Acceleration / Deceleration Time Constants:** Horn ramps in $\tau_{\text{horn}} \approx 0.7\text{ s}$; heavy wooden Bass Drum ramps in $\tau_{\text{drum}} \approx 4.2\text{ s}$.
4. **Doppler Pitch, Mic Placement & Cabinet Reflections:**
   - **Mic Distance (`rotary_close_mic`):** `Close` yields deeper horn Doppler excursion ($\pm 0.55\text{ ms}$) and narrower beaming with subtle baffle reflections (`0.05`); `Normal` uses $\pm 0.35\text{ ms}$ excursion and stronger stereo cabinet early reflections (`0.18` cross-taps at $3.4\text{ ms}$ / $4.6\text{ ms}$).
   - **Mic Angle (`rotary_mic_angle_deg`):** Virtual stereo microphone angular separation ($0^\circ..180^\circ$, default $120^\circ$).
   - **Stop Mode (`rotary_stop_mode` / `RotarySpeed::Stop`):** Smoothly brakes both horn and drum rotors to zero rotational speed and parks the horn facing the cabinet baffle.

---

## 5. Hammond Scanner Vibrato & Chorus (V1–V3, C1–C3)

The Hammond mechanical scanner vibrato consists of a multi-tap LC delay line (9 phase-shift stages) sampled by a 16-segment rotary capacitive scanner driven by the synchronous motor shaft at ~6.8 Hz.

| Mode Enum | Mode | Description |
|---|---|---|
| `Off` | Off | Pure unmodulated tonewheel bus |
| `V1` | Vibrato 1 | Subtle pitch vibrato (~0.8% frequency excursion, 100% wet phase-shifted signal) |
| `V2` | Vibrato 2 | Standard medium depth vibrato (~1.5% excursion) |
| `V3` | Vibrato 3 | Full deep pitch vibrato (~2.5% excursion) |
| `C1` | Chorus 1 | Subtle chorus (50% dry + 50% V1 wet signal) |
| `C2` | Chorus 2 | Classic Hammond organ shimmer (50% dry + 50% V2 wet signal) |
| `C3` | Chorus 3 | Deep rich chorus (50% dry + 50% V3 wet signal) |

---

## 6. Transistor & Pipe Organ Filter Banks

### 6.1 Vox Continental Model

The firmware tone source of this model (opt-in `ToneWheelSource::Vox`, 84 note oscillators with a built-in pitch vibrato) is specified in section 3.1.7; the description below is the default `Legacy` path.

12 master top-octave dividers generating unfiltered square/reed and filtered sine/flute waves:
- Drawbar 1..4 (`16'`, `8'`, `4'`, `IV` Mixture): Control relative balance between fundamental registers.
- Drawbar 9: Acts as the classic Vox **Tone / Wave** control (morphing between pure Flute/Sine and rich Reed/Square wave).

### 6.2 Farfisa Compact Model
The firmware tone source of this model (opt-in `ToneWheelSource::FarfisaWheels`, the 12 top-octave dividers of the generator `0x122A71`) is specified in section 3.1.8; the description below is the default `Legacy` path.

Multi-contact keying feeding multi-tab resonant bandpass filter banks:
- **Flute 8'**, **Oboe 8'**, **Clarinet 16'**, **Strings 4'**: Emulated via parallel resonant 2-pole bandpass filters with formant peaks at 750 Hz, 1.2 kHz, 2.5 kHz, and 4.2 kHz.

### 6.3 Liturgical Pipe Models
- **Pipe 1:** Pure principal flutes with characteristic chiff wind transient on note-on.
- **Pipe 2:** Rich swell/mixture plenum with subtle pipe chorus detune ($\pm 2\text{ cents}$) across adjacent ranks.

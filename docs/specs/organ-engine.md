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
| `Vox` | **Vox** | Vox Continental 12-oscillator top-octave divider (`16'`, `8'`, `4'`, `II`, `III`, `IV`, Sine + Reed/Triangle filter waves) |
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

### 3.1.1 Firmware oscillator-bank parity (`ToneWheelSource::Firmware`, default for `B3`, role ASSUMED about 70 %)

**Status:** the bank ALGORITHM and its pitch programming are VERIFIED (SHARC oracle, ARM run of `0xC022D9F0`); its ROLE as the organ tone source is ASSUMED at about 70 % ([organ-tone-source](../research/organ-tone-source.md): the PNO Core1 184-element bank `0x122620` plus post stage `0x12272D`; the ORG image is channel-strip post-processing). The bank is installed by the PNO Core1 state machine and by no ORG/Core2 module id ([pno-module-chain](../research/pno-module-chain.md)); the ARM drives it from the organ edit ids. `ToneWheelSource::Firmware` is therefore the default for `B3`; `ToneWheelSource::Legacy` (gear ratios, sine) stays selectable.

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

**Render change** (`render_hash`, 2,240,000 bytes): before this change (default `Legacy`) `25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a`; after (default `Firmware`, 12-TET pitches, firmware drawbar curve) `cb8d83b758cc3596ba52f1444b0e1f5d40726d27f36e53205464b47b414aacd0`; after the exact x-table levels of the oracle-audio change `d98808bd1597f99ffee7b247cb9785ab62a441941fb6b92312575ddbf73b7b75` (max sample difference 2.0e-6). Only the engine timeline (its organ layers) and the standalone organ segment change; synth, sample, piano and FX segments are bit-identical.

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

`OrganLayer::vibrato_link` selects between two behaviours (only `B3` with `ToneWheelSource::Firmware` is affected):

| `VibratoLink` | Table / gains | Mixing |
|---|---|---|
| `Firmware` (default) | vibrato-enable level 1 = detuned table + stereo-blend gains, any other level = exact table + copy-0-on-both-lanes gains (VERIFIED, `0xC022D9F0`). The level follows `scanner_mode` (`Off` = 0, any `V1..C3` = 1: ASSUMED, the ARM passes the layer vibrato-enable byte `(value == 1)`); `detuned_copies = true` forces level 1 | gains `g5..g8` and the level ramp with `s = 0.01 t + 0.99 s` (VERIFIED) per firmware frame; the two lanes drive the stereo path |
| `Manual` | `detuned_copies` only | previous behaviour: instant mono mean `copy_weights`, organ mono until the rotary |

- **Smoothing at the engine rate:** the VERIFIED coefficient is per firmware frame. The per-sample keep factor is `0.99^(F / fs)` with `F = FIRMWARE_FRAME_RATE_HZ` = 43,156.4 Hz (ASSUMED: the rate at which wheel 0 equals the engine's C1), so the time constant is 100 frames = 2.32 ms at any sample rate (`keep_for_rate`; 0.9910 at 48 kHz). Differences below `5e-5` snap to the target (the f32 fixed point sits about 7e-6 off).
- **Lanes:** `x = m00 a + m01 b`, `y = m10 a + m11 b` with the smoothed matrix `[[g5, g8], [g7, g6]] * level`; detuned steady state `x = 0.75 a + 0.375 b`, `y = 0.375 a + 0.75 b`. The lane split is done before the pickup/crosstalk stages (linear, so equivalent). While the table is exact and the ramp has settled only one lane exists and the old code path runs bit-exactly (render hash unchanged). Leaving level 1 keeps both copies running until the ramp settles, then retunes to the exact table.
- **Stereo routing:** `mid = (x + y) / 2` goes through key click, scanner and rotary exactly as the mono signal did; `side = (x - y) / 2` is added to L and subtracted from R after the rotary. `(L + R) / 2` therefore equals the `Manual` mono mean (test `mono_sum_equals_the_manual_mono_mean`); without rotary `L = x`, `R = y`.
- **Against the oracle** (`organ_ref --detuned` vs `organ_oracle_audio.py render --detuned`, `compare --lanes`, C4 E4 G4, 888000000, 43,156 Hz, first 2,048 frames dropped): lane x 37.7 dB, lane y 37.6 dB SNR after one gain; the old mono path against the same lanes gave 22.6 / 23.3 dB. Exact table: 37.7 dB on both lanes.
- **Not implemented (VERIFIED in PR #111, `organ-weights.md`):** the mix stage also smooths the per-wheel weights (`wA <- a wB + b wA`, `a` = 0.0096 on key-down, 0.0043 on a full update). The engine still applies key gains instantly; per-wheel weight smoothing is TD-04 (d).
- Tests: `lane_mix::tests` (time constant, step response, both gain sets, level selection), `vibrato_link::tests`, `stereo_lanes::tests` (level/table selection, 0.75 / 0.375 blend, ramp, mono-sum equivalence, dual-mono when exact), `drawbar_sum::tests`.

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
12 master top-octave dividers generating unfiltered square/reed and filtered sine/flute waves:
- Drawbar 1..4 (`16'`, `8'`, `4'`, `IV` Mixture): Control relative balance between fundamental registers.
- Drawbar 9: Acts as the classic Vox **Tone / Wave** control (morphing between pure Flute/Sine and rich Reed/Square wave).

### 6.2 Farfisa Compact Model
Multi-contact keying feeding multi-tab resonant bandpass filter banks:
- **Flute 8'**, **Oboe 8'**, **Clarinet 16'**, **Strings 4'**: Emulated via parallel resonant 2-pole bandpass filters with formant peaks at 750 Hz, 1.2 kHz, 2.5 kHz, and 4.2 kHz.

### 6.3 Liturgical Pipe Models
- **Pipe 1:** Pure principal flutes with characteristic chiff wind transient on note-on.
- **Pipe 2:** Rich swell/mixture plenum with subtle pipe chorus detune ($\pm 2\text{ cents}$) across adjacent ranks.

# Synth hardware sweep plan (OQ7: confirm the remaining scales)

**Checked:** 2026-11-11
**Sources:** `docs/research/synth-0x2f-ns4decode-crosscheck.md` (firmware cannot give these scales), `crates/nord-core/src/synth_params.rs`, `crates/nord-core/src/synth.rs`, NS4 User Manual v1.6X pp. 33-36.
**Question:** What are the real glide, vibrato, unison, oscillator-envelope pitch and amp-velocity scales of the Synth section, so that the `ASSUMED` constants can be replaced by `EMPIRICAL` ones?

## What is still ASSUMED and what a capture decides

| # | Constant (file) | Current value | Capture | Measured quantity | Tool |
|---|---|---|---|---|---|
| 1 | `GLIDE_MAX_SEC_PER_OCTAVE` and the `(k/10)^2` law (`synth_params.rs`) | `2 s` per octave at knob 10, quadratic | Glide sweep | seconds per octave vs Glide knob; linearity of the slew | `sweep_analyze.py glide` |
| 2 | Legato glide only on overlap (`voice_alloc.rs`) | Mono glides always, Legato only when overlapping | Glide sweep (gap vs overlap) | glide present or absent | `glide` |
| 3 | `VIBRATO_MAX_CENTS` and `2 + 6*raw/127` Hz rate (`synth_params.rs`, `synth_preset.rs`) | `100 cents` at amount 10; `2..8 Hz` | Vibrato sweep | rate (Hz) and peak depth (cents) vs Rate / Amount | `vibrato` |
| 4 | Vibrato delay fade-in (`synth.rs`) | starts after delay, 0.5 s ramp | Vibrato `DLY` | onset time and ramp length | `vibrato` (windowed) |
| 5 | Unison cents `3.5 / 7.0 / 11.5` (`synth.rs`) | copied from Piano | Unison sweep | outer-voice detune in cents per setting | `unison` |
| 6 | Osc-envelope pitch depth `+/-12 semitones` at full Env Amt (`synth.rs`) | assumed | Env-to-pitch sweep | peak pitch excursion vs Env Amt | `excursion` |
| 7 | Amp velocity steps `n/3` (`synth_params.rs`) | linear | Velocity sweep | level (dBFS) at velocity 20/64/127 for settings 0..3 | `level` |
| 8 | Filter envelope depth `+/-3 octaves` (`synth.rs`) | assumed | Filter env sweep (needs a saw) | cutoff excursion | manual / future |

Already firmware-VERIFIED and needing no capture: LFO to pitch depth curve (`synth_tables.rs`), field order and widths.

## Capture setup (all sweeps)

1. Stereo line out to an audio interface, **48 kHz, 24-bit**, no processing; keep input gain constant for the whole session. Save mono or stereo WAV (`sweep_analyze.py` mixes to mono).
2. Start from `Init` on **Synth A only** (Piano/Organ/Synth B/C off). `OSC` = **Analog > PURE > Sine** so the pitch trackers see a clean sine. **Filter Off**, all **FX off** (Mod 1/2, Amp, Delay, Reverb, Comp), Unison 0 unless the sweep says otherwise, Vibrato none, LFO destination none (no LED), Arp off, Octave 0.
3. Amp env: attack `0.5 ms`, decay `Hold` (max), release `3 ms`; amp velocity `0` unless it is the swept parameter. A held note then has a constant level.
4. Play with the on-board keys or MIDI at a fixed velocity of `100` (use `20/64/127` only for sweep 7). Hold each note for the stated duration and release.
5. Name files `<sweep>_<setting>.wav` and keep them in `.reference/hardware/` (git-ignored). Record the OS version (v1.66) and the knob value shown on the display.

## Sweep procedures

### 1. Glide (`glide_<mode>_<knob>.wav`)

- Voice mode `MONO`, Glide knob at `0.5, 1, 2, 3, 5, 7, 10` (read the value from the display).
- Play **C3**, hold 1.5 s, then (without releasing) play **C4**, hold 6 s (overlap). Repeat with a one-octave **C3 to C5** jump to test constant-rate scaling.
- Analyse `python tools/sweep_analyze.py glide glide_mono_5.wav` and tabulate `sec_per_octave` vs knob. A constant-rate glide gives `linearity > 0.99`; two octaves take twice as long as one.
- Repeat in `LEGATO` with a gap (release C3, then press C4) and with overlap to check the Legato rule.
- Result decides: the knob-to-rate law (fit `sec_per_octave = a * knob^b`) and `GLIDE_MAX_SEC_PER_OCTAVE`.

### 2. Vibrato (`vib_<mode>_<rate>_<amt>.wav`)

- Vibrato mode `ON`; sweep Rate `2, 5, 8 Hz` (display) at Amount `2, 5, 10`; hold a sustained **A3** for 4 s.
- `python tools/sweep_analyze.py vibrato vib_on_5_5.wav` gives `rate_hz` and `depth_cents_peak`.
- For `DLY`: delay `0.1, 1.0, 3.0 s`; record 8 s and read the onset time and fade-in length from the pitch track (plot or `vibrato_stats` on windows).
- Result decides: `VIBRATO_MAX_CENTS`, the Hz and cents scales, and the delay ramp (currently `0.5 s`).

### 3. Unison (`unison_<n>.wav`)

- Unison `0, 1, 2, 3`; hold **C6 (1046.5 Hz)** for 5 s. Record stereo and also test each channel separately (export L and R).
- `python tools/sweep_analyze.py unison unison_2.wav --f0 1046.5` returns `spread_cents` and `peak_cents`. The default FFT needs a steady 2.7 s; the first 0.5 s of each take is discarded by cutting the file.
- Result decides: the cents per setting and how many voices exist (count of peaks). Update `uni_cents` in `synth.rs`.

### 4. Osc-envelope pitch depth (`oscenv_pitch_<amt>.wav`)

- `ENV TO PITCH` on, Osc env attack `0.5 ms`, decay `0.5 s`, Env Amt `-10, -5, 5, 10`; play **A3**, hold 3 s.
- `python tools/sweep_analyze.py excursion oscenv_pitch_5.wav` gives `max_semitones` and `min_semitones` relative to the settled pitch.
- Result decides: semitones per unit Env Amt (replace the assumed `+/-12`).

### 5. Amp velocity (`ampvel_<setting>_<vel>.wav`)

- Amp velocity `0, 1, 2, 3`; play **A3** at velocities `20, 64, 127` for 2 s each.
- `python tools/sweep_analyze.py level ampvel_2_020.wav --from 0.5 --to 1.8` and compare dBFS across velocities per setting.
- Result decides: the sensitivity per step (currently `n/3` linear) and the velocity curve.

### 6. LFO pitch depth sanity check (optional)

- LFO destination `PITCH`, triangle, rate `2 Hz`, Mod Amt raw `27` (about 1 semitone) and max; `vibrato` should report `depth_cents_peak` of roughly `100` and `4800`. Confirms the VERIFIED table end to end.

## Recording the result

For each sweep add a row to `docs/research/synth-hardware-measurements.md` (date, OS version, setting, measured value, tool output) and then change the matching constant, its tag (`ASSUMED` to `EMPIRICAL`: name the recording, what was compared, and the fit error) and its test in the same commit. Move any unresolved item to `docs/OPEN-QUESTIONS.md`.

## Implications

- Each ASSUMED constant has a concrete, reproducible capture and a tool that turns the WAV into the number to compare.
- All estimators are pure Python and tested on synthetic signals (`tools/tests/test_sweep_analyze.py`); real recordings with filters or FX on will not track cleanly, hence the Pure Sine / filter-off setup.

## Follow-ups

- Add a filter-envelope depth tool (spectral centroid or cutoff tracking on a saw) for sweep 8.
- Add an `ns4 render-patch` comparison command that renders the same program in the emulator and reports the same metrics side by side.

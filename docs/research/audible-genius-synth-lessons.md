# Audible Genius lesson clips: measured synth and modulation behaviour

- **Checked:** 2026-11-13
- **Sources (local, never committed):** six Audible Genius "Nord Stage 4 Lesson Pack" clips in `.reference/audible-genius-nord-4/` (hardware Stage 4, panel and OLED visible, 48 kHz Opus audio, vertical 1080x1920 except Unison 4K): Delayed vibrato and aftertouch (`3zlr...nIQ`, 60 s), Unison (`qde8OVkL6RI`, 5:47), Flanger (`AdtsvmaqaVY`, 51 s), Laser zap (`0NMvKWDc-Lo`, 61 s), Pitch blip (`s0OhmMpbscU`, 62 s), Super Saw detune (`MjkV3qz8Gqk`, 59 s). Tools: `tools/ag_frames.py` (OLED change sheets), `tools/ag_audio.py` (STFT, centroid / peak tracks, spectrogram PNG), `tools/ag_pitch.py` (partial tracking, vibrato stats, exponential fit), example `ag_synth` (`cargo run --release -p nord-core --example ag_synth -- out.wav key=value ...`). Earlier readings of the same clips: [synth-0x2f-ns4decode-crosscheck](synth-0x2f-ns4decode-crosscheck.md) (unison, vibrato, laser zap, detune, blip).
- **Question:** Which quantifiable laws of the Synth (filter-envelope depth and decay, pitch-envelope depth and decay, Super Saw detune, unison, vibrato rate/depth/delay/aftertouch) and of the Mod 2 flanger do the clips pin down, and how far is the emulator from the hardware on the same settings?

Tags follow `.agents/rules/provenance-in-code.md`. The audio is lossy YouTube audio with speech over most of each clip, so every number is shape or timing evidence. Emulator errors use the **same analysis pipeline on both sides** (hardware slice and emulator render), which cancels the analysis bias; that matters for vibrato (-5 %) and for the pitch-blip fit (+20 %). Nothing from the clips (video, audio, frames) is committed.

## 1. Short answer

| Feature | Hardware (measured) | Emulator default | Emulator `measured_laws` | Tag |
|---|---|---|---|---|
| Super Saw detune | 16 equal-level voices, equally spaced in cents, no centre voice; outer voice +-6.4 / 11.0 / 27.9 / 93.75 cents at Detune 1.4 / 3.0 / 6.0 / 10.0 | 7 voices, outer +-2 / 4.3 / 9.2 / 15.1 cents (error -4 / -7 / -19 / -79 cents) | +-6.5 / 10.3 / 28.0 / 93.75 (error <= 0.7 cent) | EMPIRICAL |
| Filter envelope depth | resonance peak 4.8 octaves above the displayed cutoff at Env Amt 7.7 (4.6 at 7.4 in the earlier read): 0.62 octave per unit, 6.2 at 10 | 8 octaves at 10 (6.2 at 7.7: peak leaves the band for 0.16 s) | 6.2 | EMPIRICAL |
| Filter / osc envelope decay | tau = 0.217 x displayed Decay (pitch blip: 21.6 and 22.7 ms for 100 ms; laser zap: 0.40 to 0.47 s for 2.07 s); "displayed time = 40 dB fall" fits both | 0.18 | 0.2171 (= 1/ln 100) | EMPIRICAL |
| Pitch blip depth (Env To Pitch) | fit extrapolates to 14.9 and 13.1 semitones at Osc Env Amt +5.5 against 17.6 for 2.21 octave and 8.5 for the default; true onset about 11.6 semitones | 1 octave at 10 (6.6 st at 5.5; same-pipeline fit 8.5 st, error -5.5 st) | 1.76 octave at 10 (fit 14.1 st, error +0.1 st) | EMPIRICAL (one amount) |
| Vibrato depth | 46.5..47.5 cents read at Amount 5.0, 92.5 at 10.0, 46.6 / 46.5 with aftertouch at Amount 5.0; the pipeline reads an emulator render of 46 / 92 cents as 43.6 / 88.1, so the hardware peaks at about 49.0 / 97.5 | 92 at 10 (error -5.5 cents at 10, -3 at 5) | x1.06 (98 at 10) | EMPIRICAL (two amounts) |
| Vibrato rate | displayed 5.4 / 4.5 / 5.0 Hz read 5.44 / 4.56 / 5.05 Hz | real Hz | same | VERIFIED (1.5 %) |
| Vibrato delay | onset 0.74 to 0.85 s for 0.7 s displayed, ramp 0.40 to 0.45 s to full depth | onset = delay, ramp 0.45 s | same | EMPIRICAL (resolution 0.1 s) |
| Aftertouch vibrato | follows the pressure: 0 cents with no pressure, full Amount depth at full pressure, drops to 1 cent within 0.25 s of release | `vibrato_ctrl` linear | same | EMPIRICAL |
| Unison | 2 / 4 / 4 voices at +-2 / +-4 +-1.33 / +-6 +-2 cents (firmware table) | implemented | same | VERIFIED (earlier PR) |
| Flanger (Mod 2 FLANG) | delay sweep 1.15 to 2.3 ms, linear segments, LFO half periods 2.5 s then 1.5 s while the Rate knob was moved; Rate and Amount values not readable | base 2.8 ms +- 2.2 ms x amount, sine LFO | not changed | EMPIRICAL (shape only) |

Defaults and `render_hash` `25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a` are unchanged: the laws sit behind `SynthLayer::measured_laws` (default `false`), `crates/nord-core/src/synth/measured.rs`.

## 2. Panel values read from the OLED (confidence in brackets)

OLED crops come from `tools/ag_frames.py` (settled frames after each knob move; boxes in native pixels, e.g. `--box 205,585,310,170` for the zap clip). Values below were read by eye from the crops; none is guessed. A parameter overlay is only visible while a knob moves, so a value is trusted only when it was the last overlay before the demonstrated notes.

| Clip | Time | Panel value | Confidence |
|---|---|---|---|
| Zap | 12.8 s / 20 s / 23 s | Filter type LPM, Freq overlay 7.9, 4.8, 7.5 kHz while sweeping, Res 6.2 | high (sweeps, not settings) |
| Zap | 29 to 44 s | Filter Env ADR: attack 0.5 ms, decay 1.82 -> 1.30 -> 2.07 s, release 3.0 ms; Env Amt 7.7 (38.3 s); Freq 415 Hz (42.5 s); Res 7.3 (45.3 s); no overlay after 45.3 s | high for the last values; the earlier PR read 468 Hz and Amt 7.4 for the same clip |
| Blip | 11 / 12.75 / 15.25 s | Osc Env decay 545, 244, 100 ms; attack 0.5 ms; release 3.64 s | high |
| Blip | 16.5 / 17.5 s | Oscillator Env Amount +4.1, then +5.5; Pure Saw; notes 19.5 to 21.5 s use +5.5 and 100 ms | high |
| Blip | 25.7 s on | waveform Pure Square (a second lesson part, not used) | high |
| Detune | 0 / 14.6 / 17.4 / 22.2 / 34 / 39.6 / 47.8 s | Super Saw (1 of 5), Detune 0.0 / 1.4 / 2.9 / 3.0 / 5.8 / 6.0 / 10.0 | high (settled), medium at 1.4 and 5.8 (knob still moving) |
| Vibrato | 3.5 to 58 s | Rate 5.4 -> 4.5 Hz (7 s) -> 5.0 Hz (38 s); Amount 5.0, 0.0 (11 s), 10.0 (13.75 s), 5.0 (18 s on); Delay time 0.7 s, 1.8 s from 38 s | high |
| Flanger | whole clip | no OLED page; Mod 2 type FLANG lit, Rate / Amount knob pointers only | **not read** (no number, a knob angle would be a guess) |
| Unison | not re-read here | settings 1, 2, 3 from the earlier PR | n/a |

## 3. Per-video tables

### 3.1 Laser zap (resonance + filter envelope)

Method: STFT 2048 / hop 240 (`ag_audio`), strongest bin 500 Hz to 12 kHz, 11-frame median, crossing times of 8 / 5 / 3 / 2 / 1.2 kHz after the trigger (55.77 s, G4, LPM, 415 Hz, Res 7.3, Amt 7.7, decay 2.07 s). The emulator renders use the same notes and `res = 1.0` (the only value that gives a comparable peak, see below).

| Run | 8 kHz | 5 kHz | 3 kHz | 2 kHz | 1.2 kHz | error vs hardware (ms) |
|---|---|---|---|---|---|---|
| Hardware | 51 | 126 | 221 | 306 | 451 | |
| Emulator default (ZDF, 8 octaves) | 131 | 206 | 296 | 361 | 466 | +80, +80, +75, +55, +15 |
| `--filter-engine firmware` | 141 | 201 | 296 | 376 | 521 | +90, +75, +75, +70, +70 |
| `--synth-chain strip` | 121 | 176 | 256 | 326 | 451 | +70, +50, +35, +20, 0 |
| `measured_laws` (6.2 octaves, tau 0.217) | 36 | 136 | 241 | 316 | 446 | -15, +10, +20, +10, -5 |

Decay law: log of the octaves above 415 Hz falls by 0.095 per 40 ms on the hardware (tau 0.40 s; the 49.5 s zap gives 0.47 s), `1/ln(100)` of 2.07 s is 0.45 s. Resonance: the hardware peak stands +6 to +15 dB above the fundamental over the sweep at panel Res 7.3; the emulator reaches only -8 to +4 dB at `res = 0.73` and +14 dB at 1.0, so the emulator resonance scale is too low by about one third of the knob (single point, not applied). The peak snaps to harmonics of 392 Hz (+-1 harmonic).

### 3.2 Pitch blip

Method: `ag_pitch.partial_cents`, harmonic 3, window 1024 / hop 48, notes C5, A#4, G4 (523.3 / 466.3 / 392.1 Hz measured), onset from the 1 ms RMS rise, exponential fit of the cents after 25 ms.

| Quantity | Hardware (Env +5.5, 100 ms, to pitch) | Emulator default | `measured_laws` |
|---|---|---|---|
| tau of the pitch fall | 21.6 and 22.7 ms (A#4 note not fittable) | 17.6 ms (error -4.5) | 21.4 ms (error -0.9) |
| fitted onset | 14.9 and 13.1 st | 8.5 st | 14.1 st |
| true onset (pipeline calibrated) | about 11.6 st | 6.6 st | 11.6 st |

At 545 and 244 ms displayed Decay the Amount was not read, so those notes give no second point.

### 3.3 Super Saw detune

Method: 65536-point FFT (0.73 Hz bins) on steady G4 windows, peaks at harmonic 3 and 5 within +-200 cents, only windows whose OLED value had been stable (0.0 at 0 to 1.2 / 7 to 8 / 11.5 to 13 s; 3.0 at 21 s; 6.0 at 39.7 s on a C5; 10.0 at 46.3 s).

| Detune | Hardware voices (peaks) | Hardware outer (cents) | Emulator default outer | `measured_laws` outer |
|---|---|---|---|---|
| 0.0 | 1 (resolution +-2) | 0 | 0 | 0 |
| 1.4 | about 6 resolved | +-6.4 | -2.8 / -0.7 | -6.1 / +6.9 |
| 3.0 | 14 resolved, spacing 1.3 to 2 | +-10.4 / 11.7 | -3.9 / +4.7 | -10.4 / +10.1 |
| 6.0 | 16, spacing 3.7 | -27.9 / +27.9 | -9.3 / +9.0 | -27.8 / +28.2 |
| 10.0 | 16, spacing 12.5 | -93.7 / +93.8 | -15.8 / +14.4 | -93.7 / +93.8 |

All 16 peaks sit within 1 dB of each other at Detune 10.0. The spacing is the same in cents at harmonics 3, 5 and 8, so it is a pitch ratio. The law is close to exponential, `outer = 7.5 x 0.55 exp(0.31 d)` (within 6 % of the points); the four points are interpolated in `ln(outer)`. The earlier note (3.0 -> 25 to 28 cents) came from a moving knob. Not measured: the stereo placement of the voices (the emulator sums them to mono) and the overall level against Detune.

### 3.4 Delayed vibrato and aftertouch

Method: `ag_pitch`, harmonic 4 of G4, 2048 / hop 480, 0.2 s windows. Hardware and emulator renders go through the same code.

| Setting | Hardware | Emulator (92 cents at 10) | `measured_laws` |
|---|---|---|---|
| Amount 5.0, rate 5.4 / 4.5 / 5.0 Hz | depth 47.3 / 47.5 / 46.7, rate 5.44 / 4.56 / 5.05 | depth 43.6, rate 5.00 (rate set 5) | 46.3 expected (x1.06 x 0.949) |
| Amount 10.0, rate 4.5 Hz | depth 92.5, rate 4.57 | 88.1 | 92.5 expected |
| Aftertouch, Amount 5.0, two presses | 46.6 and 46.5 cents, 0 to 1 cent between presses | linear in `vibrato_ctrl` | same |
| Delay 0.7 s | onset 0.74 to 0.85 s after the key, full depth about 0.4 s later | 0.7 s, ramp 0.45 s | same |
| Delay 1.8 s | the only note (1.1 s) is shorter than the delay and stays flat; no onset measured | | |

### 3.5 Flanger (airplane swoosh)

Method: a C2 saw (65.4 Hz harmonics) under the Mod 2 flanger. The ripple of the harmonic levels along the harmonic index is detrended (order 4) and transformed; the dominant ripple frequency divided by f0 is the flanger delay (scratch script, 8192-point frames, hop 50 ms, harmonics 2 to 100). Valid from 29.5 to 36 s (ripple depth 6 dB std); later the ripple drops to 2 to 3 dB.

| Time | Delay | Event |
|---|---|---|
| 29.6 s | 2.0 ms | falling |
| 31.4 s | 1.15 ms | minimum (half period 1.8 s so far) |
| 33.9 s | 2.28 ms | maximum (half period 2.5 s) |
| 35.4 s | 1.48 ms | next minimum (half period 1.5 s, the Rate knob was being raised) |

Slope 0.45 ms/s over 31.5 to 33 s (a triangle sweep); mean 1.7 ms, swing +-0.57 ms at that Amount. The emulator flanger (`fx/modulation.rs`) uses 2.8 ms +- 2.2 ms x amount with a sine LFO: centre +1.1 ms against the hardware, and at Amount 1.0 the swing is four times larger than the hardware swing. No Rate or Amount value was readable, so no rate or depth law is pinned and the FX code is unchanged.

### 3.6 Unison

Not re-measured. The earlier PR holds the firmware table (+-2 / +-4 +-1.33 / +-6 +-2 cents, no centre voice, weak channel 0.43 to 0.45). Level against unison amount (the emulator uses constant power over the partner voices) is still open.

## 4. Implications

- The Super Saw law is the largest single error of the Synth (up to 79 cents at Detune 10); a 16-voice equal-level comb fixes it to under 1 cent at four settings.
- The two envelope depths of the earlier PR (8 octaves) and the 0.18 decay fraction were a little off; one pair of laws (6.2 octaves, 0.217) brings the zap timing error from 80 ms to 20 ms and the blip fall to 1 ms.
- The pitch-envelope scale rests on one Amount (+5.5) and the vibrato scale on two; the filter-envelope depth on two close amounts. All are behind the switch until a second setting confirms linearity.
- The vibrato depth read of the earlier PR (46 cents at 5.0) was not corrected for the analysis bias; the corrected hardware value is about 49 cents at 5.0 and 97.5 at 10.0.
- The strip chain (`--synth-chain strip`) is 70 to 0 ms late on the zap, better than the default (80 to 15 ms) at every threshold but still worse than `measured_laws` (20 ms); it does not use the filter-envelope octaves of `measured_laws` (its own envelope records), so the two switches are independent.

## 5. Follow-ups

1. A second Osc Env Amount for the blip (the 545 and 244 ms notes at 11 to 15 s used an unread Amount) and a second Filter Env Amt for the zap, to confirm linearity of 1.76 and 6.2 octaves.
2. Read the Flanger Rate and Amount: pan the camera to the OLED page of Mod 2, or compare knob pointer angles of the clip frames with the panel scale; then fit the rate range and the delay law against the sweep table above.
3. Measure the Super Saw stereo spread and level against Detune, and Unison level against amount.
4. Resonance scale: Res 7.3 matches an emulator `res = 1.0`; repeat at two other values before changing `resonance` mapping.
5. Vibrato delay 1.8 s onset with a note longer than 2 s.
6. If the opt-in laws hold, flip them with a D22-style decision (re-baseline `render_hash`).

## Reproduce

```bash
python tools/ag_frames.py <clip.webm> --box 205,585,310,170 --fps 4 --out sheet.png     # OLED values
python tools/ag_audio.py <clip.wav> --png s.png --start 30 --end 60 --fmax 6000           # spectrogram
python tools/ag_pitch.py <clip.wav> --start 12.2 --end 13.0 --fref 391.995 --harmonic 4  # vibrato stats
cargo run --release -p nord-core --example ag_synth -- out.wav note=67 wf=supersaw detune=10 measured=1
```

The 48 kHz WAVs came from `ffmpeg -i <clip.webm> -vn -ac 2 -ar 48000 out.wav`. Raw per-window numbers were produced by throw-away scripts on those WAVs (peak lists per window, crossing times); the committed tools hold the reusable parts (tracking, fitting, tiling).

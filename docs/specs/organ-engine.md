# Organ Engine Specification (`DSPO`)

## 1. Scope

Specifies the 2-layer (`Organ A`, `Organ B`) physical modeling Organ Engine and Model 122 Vintage Rotary Speaker simulation of the Nord Stage 4.

Implemented in [`crates/nord-core/src/organ.rs`](../../crates/nord-core/src/organ.rs).

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

| Semitone (Mod 12) | Note | Drive Gear Teeth | Driven Gear Teeth | Frequency Ratio to C |
|---|---|---|---|---|
| 0 | C | 85 | 104 | `0.81730769` |
| 1 | C# | 71 | 82 | `0.86585366` |
| 2 | D | 67 | 73 | `0.91780822` |
| 3 | D# | 35 | 36 | `0.97222222` |
| 4 | E | 69 | 67 | `1.02985075` |
| 5 | F | 100 | 92 | `1.08695652` |
| 6 | F# | 37 | 32 | `1.15625000` |
| 7 | G | 49 | 40 | `1.22500000` |
| 8 | G# | 48 | 37 | `1.29729730` |
| 9 | A | 110 | 80 | `1.37500000` ($A_4 = 440.0\text{ Hz}$) |
| 10 | A# | 73 | 50 | `1.46000000` |
| 11 | B | 84 | 54 | `1.55555556` |

Each octave doubles the number of teeth on the rotating wheel ($2, 4, 8, 16, 32, 64, 128, 192$). All 91 tonewheel phase accumulators advance continuously on every audio sample (`phase[i] = (phase[i] + 2*pi * f[i] / fs) mod 2*pi`), ensuring authentic phase interaction when multiple keys share a tonewheel.

### 3.2 9 Drawbar Harmonic Bus Matrix & Foldback

Each key routes 9 contact switches to the 9 drawbar buses (`0..=8` level steps, mapped in -3 dB steps per notch below `8`, with `0` = $-\infty\text{ dB}$):

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
2. **800 Hz Linkwitz-Riley Crossover:** Splits the signal into the **Treble Horn** ($\ge 800\text{ Hz}$) and **Bass Rotor Drum** ($< 800\text{ Hz}$).
3. **Independent Rotational Ballistics:**
   - **Slow (Chorale):** Horn = $0.80\text{ Hz}$ ($48\text{ RPM}$), Bass Drum = $0.67\text{ Hz}$ ($40\text{ RPM}$).
   - **Fast (Tremolo):** Horn = $6.85\text{ Hz}$ ($411\text{ RPM}$), Bass Drum = $5.70\text{ Hz}$ ($342\text{ RPM}$).
   - **Acceleration / Deceleration Time Constants:** Horn ramps in $\tau_{\text{horn}} \approx 0.7\text{ s}$; heavy wooden Bass Drum ramps in $\tau_{\text{drum}} \approx 4.2\text{ s}$.
4. **Doppler Pitch & Amplitude Modulation:** Stereo left/right virtual microphones separated by $90^\circ..180^\circ$ modulate delay lines ($\pm 0.45\text{ ms}$ excursion) and directional horn beaming.

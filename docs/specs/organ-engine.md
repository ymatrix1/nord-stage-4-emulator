# Organ Engine Specification (`DSPO`)

## 1. Scope

Specifies the 2-layer (`Organ A`, `Organ B`) physical modeling Organ Engine and Model 122 Vintage Rotary Speaker simulation of the Nord Stage 4.

Implemented in [`crates/nord-core/src/organ/`](../../crates/nord-core/src/organ/):

| Module | Content |
|---|---|
| `mod.rs` | `OrganLayer` state, `note_on`/`note_off`, `render_frame` signal chain |
| `types.rs` | enums (`OrganModel`, `ScannerMode`, `RotarySpeed`, `ToneWheelSource`), gear table, rotary constants |
| `firmware_bank.rs` | `ToneBank`: PNO Core1 oscillator-bank arithmetic, x table, `fracmul`, `phase_step` |
| `tonewheel.rs` | gear frequencies, pickup calibration, 91-wheel generator (firmware or legacy source) |
| `drawbar.rs` | drawbar offsets, level law (`drawbar_gain`, TD-10), foldback |
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

### 3.1.1 Firmware oscillator-bank parity (`ToneWheelSource::Firmware`, opt-in, role disputed)

**Status:** the bank ALGORITHM is VERIFIED against the SHARC oracle; its ROLE as the organ tonewheel generator is ASSUMED and disputed. The bank is installed by the PNO Core1 piano state machine (state 1 generators: `0x122620` oscillator bank, `0x12272D` post stage, `0x122A71` LFO delay) and by no ORG/Core2 module id ([pno-module-chain](../research/pno-module-chain.md), branch `research/pno-module-chain`). Current lean: about 65 % piano string resonance (unison-style detuned pair; the ARM has `Edt_Sys_kStringResonanceLevel`), about 35 % B3 tonewheels. The default is therefore `ToneWheelSource::Legacy`; `Firmware` stays available as an option with the same code. The organ tone source remains unlocated.

What would settle it: (1) the ARM writer of the bank's weights (`0x9566E`, `0x95726`, `0x957DE`) and whether `Edt_Sys_kToneWheelMode` / `Buffer_kOrgDrawbar` or `Edt_Sys_kStringResonanceLevel` drives them; (2) the piano-versus-organ program selector (which program runs the PNO Core1 state machine when an organ layer is active); (3) a running boot capture of the DSP data memory with an organ patch sounding.

Source: [organ-tonewheel-bank](../research/organ-tonewheel-bank.md) (oracle run, 0 mismatches over 29,440 words). For `OrganModel::B3` each wheel runs

| Step | Equation | Status |
|---|---|---|
| Step | `step = round(f / fs * 2^32)` (firmware: `fracmul(inc, tune)`, `f = fs * step / 2^32`) | VERIFIED relation, the frequencies `f` stay the Hammond gear ratios (ASSUMED; the firmware ladder is 12-TET) |
| Accumulate | `phase = (phase + step) mod 2^32` (`u32`), before the table read | VERIFIED |
| Index | `idx = phase >> 20`, 4096 entries, no interpolation | VERIFIED |
| Waveform | `x = sin t - 0.1122 cos 2t - 0.01585 cos 3t - 0.00631 cos 4t - 0.00398 cos 5t` (harmonics 0, -19.0, -36.0, -44.0, -48.0 dB), `T[i] = x(2 pi i / 4096)` built once in `new` | VERIFIED harmonics, table phase origin ASSUMED |
| Role | the x wave on every B3 wheel, replacing the sine and the extra low-wheel harmonics (opt-in only) | ASSUMED/disputed (about 35 % tonewheels, about 65 % piano string resonance) |

The default (`ToneWheelSource::Legacy`) and every other model keep the float-phase sine path; the `Firmware` variant is selected by setting `OrganLayer::tonewheel_source`. The pickup calibration, crosstalk and drawbar mapping are unchanged and `ASSUMED`. Tests: `firmware_bank::tests` (closed form at chosen steps, harmonic levels, `fracmul` rounding), `bank_golden::tests` (Rust bank vs the Python model, vectors from `python tools/organ_bank_ref.py golden`, synthetic table, no firmware data).

**Detuned copies** (`OrganLayer::detuned_copies`, default off): each wheel runs two accumulators at `-0.945` and `+0.945` cent (`COPY_SPREAD_CENTS = 1.89`, EMPIRICAL: rms of `1200 log2(v[2k+1] / v[2k])` over the ARM default table; the firmware signs are random, the constant pair is ASSUMED) and the outputs are averaged. Off by default, and only effective with `ToneWheelSource::Firmware`; the role of the copies is unproven.

**Render change** (`render_hash`, 2,240,000 bytes): PR #83 made `Firmware` the default and moved the SHA-256 from `25baa387...7b59a` to `181c4fcb...3029`; restoring `Legacy` as the default returns it to `25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a` (re-run after this change). The `181c4fcb` figure now applies only to `Firmware` selected explicitly. In that mode only the engine timeline and the standalone organ segment change; synth, piano and FX segments are bit-identical. Organ level changes by +0.08 dB RMS; the 4th and higher partials of each wheel appear.

- **Pickup Calibration (`B3` vs `B3 Bass`):** Standard `B3` tapers the bottom 12 complex wheels (`0.74..0.94`) and softens upper wheels above `tw 60` (`1.0..0.84`). `B3 Bass` boosts low wheels `0..24` (`1.32..1.05`) for prominent pedal/bass weight.

### 3.2 9 Drawbar Harmonic Bus Matrix & Foldback

Each key routes 9 contact switches to the 9 drawbar buses (`0..=8` level steps). The level law is `ASSUMED`: the code uses linear `level / 8` (`drawbar::drawbar_gain`); this section used to claim -3 dB per notch, neither is verified (TD-10; the preset field is 4 bits `0..=8`, [preset-dsp-map](../research/preset-dsp-map.md)). `0` = $-\infty\text{ dB}$:

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

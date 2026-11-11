# Piano Engine & Sympathetic String Resonance Specification (`DSPF`)

## 1. Scope

Specifies the 2-layer (`Piano A`, `Piano B`), 120-voice polyphonic Piano Engine, Timbre shaping, Dynamic Pedal Noise, and real-time Modal Sympathetic String Resonance of the Nord Stage 4.

Implemented in [`crates/nord-core/src/piano.rs`](../../crates/nord-core/src/piano.rs).

---

## 2. Piano Categories & Voice Architecture

The Stage 4 organizes piano banks into 6 acoustic and electro-mechanical categories:
1. **`Grand`** — Concert & studio grand pianos (full string resonance + pedal noise).
2. **`Upright`** — Vertical acoustic pianos with wider stereo soundboard bloom.
3. **`Electric`** — Tine (Rhodes Mk I/II/V) and Reed (Wurlitzer 200A) electric pianos with Dyno preamps.
4. **`Clav`** — Hohner Clavinet D6 with pickup & mute filter tabs.
5. **`Digital`** — FM DX7 / MKS-20 / CP-80 layer pianos.
6. **`Misc`** — Harpsichord, Marimba, Vibraphone, Cimbalom.

### Velocity Response Curves (`KBD TOUCH`)

Maps incoming MIDI velocity $v \in [1, 127]$ to normalized gain/timbre excitation $v_{\text{norm}} \in (0, 1]$:
- **`Light`:** $\left(\frac{v}{127}\right)^{0.65}$ (easier to reach fortissimo layers)
- **`Medium` (Default):** $\left(\frac{v}{127}\right)^{1.00}$ (linear response)
- **`Heavy`:** $\left(\frac{v}{127}\right)^{1.55}$ (wider pianissimo dynamic control)

---

## 3. Timbre EQ Presets (`PIANO TIMBRE`)

Each Piano layer offers selectable spectral contours modeled via 2nd-order biquad shelving/peaking filters:
- **`Off`:** Flat response ($0\text{ dB}$).
- **`Soft`:** $-6\text{ dB}$ high-shelf above $1.8\text{ kHz}$ for warm ballad voicings.
- **`Mid`:** $+3.5\text{ dB}$ presence peak at $1.2\text{ kHz}$ ($Q = 0.9$) to cut through a live band.
- **`Bright`:** $+4.5\text{ dB}$ high-shelf above $2.5\text{ kHz}$.
- **`Dyno1` / `Dyno2`** (Electric Pianos): Classic Dyno-My-Piano bell boost ($+6\text{ dB}$ @ $3.8\text{ kHz}$, $+3\text{ dB}$ @ $120\text{ Hz}$, $-4\text{ dB}$ notch @ $800\text{ Hz}$).

---

## 4. Advanced String Resonance (Modal Sympathetic DSP)

When `String Resonance` is active on an Acoustic `Grand` or `Upright` model:
1. **Damper Pedal Down (`CC64 >= 64`):** A bank of 12 tuned modal harmonic resonators (covering the fundamental and low overtones of the soundboard harp) is excited by the audio output of struck notes ($g_{\text{res}} \approx -21\text{ dB}$, decay $T_{60} \approx 4.5\text{ s}$).
2. **Undamped Held Notes (Pedal Up):** Any key held silently (or after note decay) keeps its individual string resonator undamped; striking a lower note whose 2nd, 3rd, or 4th harmonic matches the held key's fundamental excites sympathetic ringing on that specific pitch.

Each resonator is implemented as a 2-pole high-$Q$ bandpass/comb resonator with frequency-dependent damping:
$$
y[n] = (1 - r^2) \sin(\omega_0) \cdot x[n] + 2r\cos(\omega_0) \cdot y[n-1] - r^2 \cdot y[n-2], \qquad r = e^{-\frac{3 \ln 10}{T_{60} f_s}}
$$

---

## 5. Dynamic Pedal Noise (`Piano Pedal` in Firmware)

When `Pedal Noise` is enabled and `CC64` changes rapidly:
- **Damper Lift (Note-On of Pedal):** Velocity of pedal depression ($\Delta\text{CC64} / \Delta t$) scales a short low-passed mechanical thump ($f_c = 140\text{ Hz}$) plus an airy felt-release brush transient, exciting the sympathetic string resonator bank.
- **Damper Drop (Pedal Release):** Generates a damped felt thud ($f_c = 95\text{ Hz}$, $\tau = 65\text{ ms}$) as all dampers return to the strings.

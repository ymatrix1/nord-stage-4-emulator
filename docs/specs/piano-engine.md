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
- **`Light`:** $\left(\dfrac{v}{127}\right)^{0.65}$ (easier to reach fortissimo layers)
- **`Medium` (Default):** $\left(\dfrac{v}{127}\right)^{1.00}$ (linear response)
- **`Heavy`:** $\left(\dfrac{v}{127}\right)^{1.55}$ (wider pianissimo dynamic control)

---

## 3. Key Release

Releasing a key starts a 120 ms damping envelope on its voice. With the damper pedal down, the note remains sustained until pedal release, which starts the same damping phase. Note Off does not deactivate a voice immediately.

## 4. Timbre EQ Presets (`PIANO TIMBRE`)

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
y[n] = (1 - r^2) \sin(\omega_0) \cdot x[n] + 2r\cos(\omega_0) \cdot y[n-1] - r^2 \cdot y[n-2], \qquad r = \exp\left(-\frac{3 \ln 10}{T_{60} f_s}\right)
$$

---

## 5. Dynamic Pedal Noise (`Piano Pedal` in Firmware) & Unison / Dyn Comp

When `Pedal Noise` is enabled and `CC64`, `CC66`, or `CC67` transitions:
- **Damper Lift (`CC64 >= 64`):** Velocity of pedal depression scales a low-passed mechanical thump ($f_c = 140\text{ Hz}$) plus an airy felt-release brush transient (`pedal_brush_env`) exciting the sympathetic string resonator bank.
- **Damper Drop (`CC64 < 64`):** Generates a deeper damped felt thud ($f_c = 95\text{ Hz}$).
- **Sostenuto (`CC66`):** Captures notes currently active at pedal-down (`sostenuto_held`) until pedal release and triggers a lighter mechanical rod transient ($f_c = 120\text{ Hz}$).
- **Una Corda / Soft (`CC67`):** Softens hammer strike excitation ($0.76\times$ gain, $0.90\times$ post-shaping) and triggers rail-shift noise ($f_c = 160\text{ Hz}$).
- **Piano `UNISON` (`1, 2, 3`) & `DYN COMP` (`1, 2, 3`):** `unison` adds symmetric stereo hammer/string detune (`3.5`, `7.0`, `11.5` cents) across active voices; `dyn_comp` compresses strike velocity curves and applies fast output dynamic leveling.

---

## 6. Damper Release Envelopes & Soft Release (`SOFT REL`)

Upon receiving MIDI `NoteOff` or Damper Pedal Release (`CC64 < 64`), the vibrating string is not instantly silenced to zero; instead, an authentic felt damper contact decay is applied:

1. **Standard Key Release:**
   Exponential envelope decay with time constant $\tau = 80\text{ ms}$ ($v_{\text{env}}[n] = v_{\text{env}}[n-1] \cdot \exp(-1 / (0.080 \cdot f_s))$). This completely eliminates high-frequency step discontinuities and audio clicks.
2. **Soft Release (`SOFT REL`):**
   When `soft_release` is active on the front panel, the damper descends with a gentler time constant $\tau = 180\text{ ms}$, modeling lighter felt contact for expressive legato phrasing and ambient tail preservation.
3. **Voice Deactivation Invariant:**
   A voice remains in `releasing` state and active in the polyphonic voice matrix until $v_{\text{env}} < 10^{-4}$ ($-80\text{ dB}$), at which point it is safely marked inactive (`v.active = false`) without memory allocations or locking.

---

## 7. Multisample Stroke Playback & Pitch Transposition

Implemented in [`crates/nord-core/src/piano.rs`](../../crates/nord-core/src/piano.rs) (`NpnoSound`, `LoadedPianoStroke`).

When a pre-decoded `.npno` or open SFZ/WAV (`NpnoSound::from_sfz_str` / `from_sfz_path`, `parse_wav_mono` 8/16/24/32-bit mono/stereo) instrument is loaded via `PianoLayer::set_sound`:
1. **Zone, Velocity, and Release Lookup (`NpnoSound::select_stroke`):**
   Multi-velocity SFZ `<region>` zones (`lovel..=hivel`, `lokey..=hikey`, `trigger=release`) are deduplicated in RAM (`Arc<[f32]>`). Key-off events spawn matching `trigger=release` samples without heap allocation. For `.npno` files, `key_map[n]` and `tuning_cents[n]` resolve the stroke directly.
2. **Pitch Transposition Ratio:**
   The fractional playback speed increment per engine sample frame is calculated as:
   $$
   r = 2^{\frac{(n - n_{\text{root}}) + \frac{c}{100}}{12}}, \qquad \text{sample\_step} = r \cdot \frac{f_{\text{s, stroke}}}{f_{\text{s, engine}}}
   $$
3. **Linear Fractional Sample Interpolation:**
   To guarantee clean, anti-aliased transposition without DSP buffer clicks or heap allocations during real-time rendering:
   $$
   x[t] = x[k] + \mathrm{frac} \cdot (x[k+1] - x[k]), \qquad k = \lfloor \text{sample\_pos} \rfloor, \quad \mathrm{frac} = \text{sample\_pos} - k
   $$
4. **Resilient Fallback:**
   If no sample is loaded, or if the note is outside the mapped multisample zones, the engine smoothly falls back to the mathematical harmonic oscillator bank.

---

## 8. Polyphonic Voice Stealing Priority

When note-on events exceed the pre-allocated voice capacity (`MAX_PIANO_VOICES = 24` per layer):
1. **Note Match:** If an existing voice is already sounding the exact same MIDI note, it is re-triggered with updated velocity and envelope.
2. **Free Voice:** The first inactive voice (`!v.active`) is selected.
3. **Priority Stealing:** If all voices are occupied, the allocator computes a stealing score:
   $$
   \text{score} = \begin{cases} v_{\text{env}} \cdot 0.1 & \text{if } v_{\text{releasing}} \\ v_{\text{env}} & \text{otherwise} \end{cases}
   $$
   The voice with the lowest score (decaying released notes first, then quietest sustaining notes) is recycled, preserving loud bass roots and active top melody notes without audible voice drops.

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
1. **Damper Pedal Down (`CC64 >= 64`):** A bank of 12 tuned modal harmonic resonators (covering the fundamental and low overtones of the soundboard harp) is excited by the audio output of struck notes ($g_{\text{res}} \approx -21\text{ dB}$, decay $T_{60} \approx 4.5\text{ s}$) and summed with balanced alternating stereo panning (`0.72 / 0.28`, avoiding anti-phase cancellation).
2. **Undamped Held Notes (Pedal Up):** Any key held silently (or after note decay) keeps its pitch-class (`midi_note % 12`) resonator undamped across octaves; striking a note excites sympathetic ringing in held strings without artificial high-frequency ringing.

Each resonator is implemented as a 2-pole high-$Q$ bandpass/comb resonator with frequency-dependent damping:

$$
y[n] = (1 - r^2) \sin(\omega_0) \cdot x[n] + 2r\cos(\omega_0) \cdot y[n-1] - r^2 \cdot y[n-2], \qquad r = \exp\left(-\frac{3 \ln 10}{T_{60} f_s}\right)
$$

---

## 5. Dynamic Pedal Noise (`Piano Pedal` in Firmware) & Unison / Dyn Comp

When `Pedal Noise` is enabled and `CC64`, `CC66`, or `CC67` transitions:
- **Damper Lift (`CC64 >= 64`):** Velocity of pedal depression scales a low-passed mechanical felt thump ($f_c = 140\text{ Hz}$) plus an airy felt-release brush transient (`pedal_brush_env`) exciting the sympathetic string resonator bank (using filtered impulse transients without synthetic sine chirps).
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
1. **Zone, Velocity, Damper & Release Lookup (`NpnoSound::select_stroke_with_damper`):**
   Multi-velocity `.npno` strokes and SFZ `<region>` zones (`lovel..=hivel`, `lokey..=hikey`, `trigger=release`) are deduplicated in RAM (`Arc<[f32]>`). In `.npno` banks, byte `+0x0D` (`vel_low`) encodes descending attenuation (`0 = fff` loudest/brightest down to `max_att = ppp` softest/warmest); zones are sorted descending and partitioned across `0..=127`. When `damper_pedal_down` is active, `.npno` `kind = 1` (`NPNO_STROKE_DAMPER`) sympathetic harp strokes are selected ahead of dry `kind = 0` strokes. Held sampled voices preserve their natural recorded decay envelope (`v.env = 1.0`) until note-off or sample end, while key-off events spawn matching release (`kind = 2` / `trigger=release`) samples without heap allocation. For `.npno` files, `key_map[n]` and `tuning_cents[n]` resolve root mapping and stretch tuning directly.
2. **Pitch Transposition Ratio:**
   The fractional playback speed increment per engine sample frame is calculated as:
   $$
   r = 2^{\frac{(n - n_{\text{root}}) + \frac{c}{100}}{12}}, \qquad \text{sample\_step} = r \cdot \frac{f_{\text{s, stroke}}}{f_{\text{s, engine}}}
   $$
3. **SMP 6-Tap Q31 Sample Interpolation (`SampleInterp::SmpQ31`, default):**
   All fractional sample lookups (including `35 kHz -> 48 kHz` `.npno` hardware sample-rate conversion and semitone transposition) use the kernel the SMP DSP image applies (`crates/nord-core/src/piano/q31_kernel.rs`; see "SMP sample interpolation" below). `SampleInterp::Cubic` selects the earlier zero-allocation 4-point Catmull-Rom spline over `[p_{k-1}, p_k, p_{k+1}, p_{k+2}]` (`PianoLayer::sample_interp`).
4. **Resilient Fallback:**
   If no sample is loaded, or if the note is outside the mapped multisample zones, the engine smoothly falls back to the mathematical harmonic oscillator bank.

---

## 8. Polyphonic Voice Stealing Priority

When note-on events exceed the pre-allocated voice capacity (`MAX_PIANO_VOICES = 64` per layer):
1. **Same-Note Retrigger:** If sustaining voices are already sounding the exact same MIDI note, they are transitioned into rapid felt damping while a free slot is allocated via `allocate_voice_slot(None)` so the previous strike decays cleanly without zero-sample hard clicks.
2. **Free Voice:** The first inactive voice (`!v.active`) is selected.
3. **Priority Stealing:** If all voices are occupied, the allocator computes a stealing score:
   $$
   \text{score} = \begin{cases} v_{\text{env}} \cdot 0.1 & \text{if } v_{\text{releasing}} \\ v_{\text{env}} & \text{otherwise} \end{cases}
   $$
   The voice with the lowest score (decaying released notes first, then quietest sustaining notes) is recycled, preserving loud bass roots and active top melody notes without audible voice drops.

---

## 9. SMP sample interpolation (hardware parity)

Source: [smp-kernel-fit](../research/smp-kernel-fit.md) (VERIFIED, `tools/ns4_smp_kernel_fit.py`, 0 mismatches over the 1,536 words at SMP `DM(0xE3400)`). Code: `piano/q31_kernel.rs` (kernel and interpolator), `piano/smp_unpack.rs` (unpacker and predictor model).

| Item | Definition |
|---|---|
| Position | `pos = n + frac`; `phase p = floor(frac * 256)` (hardware: `frac >> 24`, truncation) |
| Taps | tap `t = 0..5` weighs sample `n - 2 + t`; samples past either end repeat the edge sample (ASSUMED) |
| Offset | `d = (2 - t) + p / 256` |
| Kernel | `u(d) = sinc(d) * (0.54 + 0.46 cos(pi d / 3))`; `h[t][p] = u + (1 - sum_t u) / 6` |
| Word | `min(round(h * 2^23), 2^23 - 1) << 8` (Q31, low byte zero; the peak saturates to `0x7FFFFF00`) |
| Output | `y = sum_t h[t][p] * x[n - 2 + t]`; the integer model `mac_q31` rounds the upper 32 bits of `2 * sum a w` to nearest (ties up, ASSUMED) |
| Properties | `h[t][p] = h[5-t][256-p]` exactly; phase 0 is a pass-through; per-phase tap sum 1 (+- 2 LSB of 23 bits) |

The table is built once (`SmpKernel::shared()`, called in `PianoLayer::new`); `render_frame` only reads a `&'static` table (6 MACs per channel, no allocation, no lock).

`PianoLayer::sample_interp` selects the path: `SmpQ31` (default, hardware parity) or `Cubic` (Catmull-Rom). Measured worst-case error of interpolating a sine at fractional positions (kernel vs Catmull-Rom, Python model in the research note): 0.05 fs -71 / -66 dB, 0.10 fs -55 / -47 dB, 0.15 fs -50 / -35 dB, 0.20 fs -55 / -27 dB, 0.25 fs -36 / -19 dB, 0.30 fs -22 / -14 dB. The unit test `test_sine_tone_error_against_ideal_and_cubic` pins the kernel below the cubic error at 0.2 fs. Like the hardware, the kernel is a fixed interpolator with no anti-alias filtering for upward transposition.

`piano/smp_unpack.rs` models the voice-loop residual unpacker (LSB-first, 16 payload bits per 32-bit word, `FEXT`/`FDEP (SE)`, widths 1..16) and the 4-tap integer predictor; tests prove it equals `cbin::sample` decoding of the same synthetic residuals for widths 1..16 and orders 0..4.

`piano/smp_voice.rs` and `piano/smp_page.rs` model the whole voice call (stream fetch, predictor, history ring, kernel, shift by 17, gain, bus add, position update) and play a `.npno` page; they equal the SMP image on the oracle bit for bit ([smp-voice](../research/smp-voice.md): counter, phase, history, interpolated value and bus, 17 runs on real piano strokes and two committed synthetic goldens). Two differences to the f32 engine path are VERIFIED there: the DSP rounds the six-tap sum to a whole decoded sample (`R2 = MRF (SSFR)`) before scaling, so its output is a multiple of 1/8192 (`SmpKernel::interpolate_hw` reproduces it; the f32 path differs by at most 0.5/8192, 76 dB SNR typical); and the DSP starts from a zero history while the engine repeats the first sample (ASSUMED rule, the ARM preload is unknown). The voice decodes one sample per call, so the per-voice step must stay at or below 1 (ASSUMED consequence for the ARM's multisample choice).

## 10. Candidate: firmware detuned oscillator bank as string resonance (not implemented)

Status: **candidate, ASSUMED, not implemented.** The PNO Core1 routine `0x122620` is a 184-element phase-accumulator wavetable bank: 92 fixed chromatic pitches, each in two copies detuned by about 1.89 cent rms, no per-note state, notes act only through ARM-written weights ([organ-tonewheel-bank](../research/organ-tonewheel-bank.md), algorithm VERIFIED by oracle; the mix stage `0x12268F` is not reproduced). The module chain installs it from the PNO piano state machine and from no ORG/Core2 module id ([pno-module-chain](../research/pno-module-chain.md), branch `research/pno-module-chain`).

| Observation | Fits string resonance |
|---|---|
| Fixed pitch ladder of 92 semitones, always running | sympathetic strings exist for every key regardless of notes held |
| Two copies per pitch, spread about 2 cent | unison-style detuned string pair |
| ARM has `Edt_Sys_kStringResonanceLevel` (`0xC02D57D0`) next to `Edt_Sys_kToneWheelMode` (`0xC02D57F0`) | a level control for such a bank exists |
| Role judgement | superseded by `docs/research/organ-tone-source.md`: about 70 % organ tonewheels, about 20 % string resonance only (ASSUMED) |

The modal resonators of section 4 are the shipped model; this bank would replace them only after the ARM weight writer or a running capture confirms the role. Open items stay in `docs/OPEN-QUESTIONS.md` and TD-04.

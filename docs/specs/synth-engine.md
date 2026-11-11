# Synth Engine Specification (`DSPS` — Nord Wave 2 Architecture)

## 1. Scope

Specifies the 3-layer (`Synth A`, `Synth B`, `Synth C`) 46-voice polyphonic Synth Engine derived from the Nord Wave 2 (`DSPSyn` SHARC partition).

Implemented in [`crates/nord-core/src/synth.rs`](../../crates/nord-core/src/synth.rs).

---

## 2. Oscillator Categories (`OSC MODE`)

Each Synth layer selects one of four synthesis paradigms:

| Mode | Description | Control Parameters |
|---|---|---|
| **Analog (VA)** | Anti-aliased Virtual Analog waveforms (`Saw`, `Pulse/Square`, `Triangle`, `Sine`, `SuperSaw`) | Shape/PWM (`0..1`), Osc Config (`Pitch`, `Detune`, `Sync`, `Sub`, `Noise`) |
| **Wavetable** | 64 single-cycle harmonic wavetables (`NUM_WAVETABLES = 64`, 8 spectral families) with 8-band octave mipmap anti-aliasing (`MIP_MAX_HARMONICS`) and Lanczos-smoothed Fourier synthesis | Table index (`wavetable_index: 0..63`), Spectral formant/scan (`wavetable_position: 0..1`) |
| **FM (2/3-Op)** | Phase-modulation synthesis with 2/3-operator cascade (`Op3 -> Op2 -> Op1`), 16 harmonic & inharmonic carrier:modulator ratios (`FM_RATIOS`), anti-hunting modulator self-feedback, and transient decay mod envelope | Harmonic ratio (`fm_ratio`), FM Amount (`fm_amount`), Feedback (`fm_feedback`), Op3 Sub Ratio/Amount, Mod Env Amount |
| **Sample** | Lock-free zero-allocation multisample playback (`SynthSampleSound` / `LoadedSynthSample`) with root-key pitch tracking, linear loop crossfade, and 1-pole timbre filter (`Off`/`Soft`/`Bright`) | Sample zone bank, Loop crossfade (`loop_crossfade`), Timbre filter (`SampleTimbre`) |

---

## 3. PolyBLEP Anti-Aliased Virtual Analog Oscillator

To eliminate aliasing around discontinuous waveform edges (Sawtooth wrap at $\phi = 1.0$ and Pulse transitions at $\phi = 0, \text{pw}$) without costly oversampling, we apply the 2-point **Polynomial Band-Limited Step (`PolyBLEP`)** residual:

Let normalized phase increment be $\Delta t = f_0 / f_s \in (0, 0.5)$ and phase be $t \in [0, 1)$:

$$
\operatorname{poly\_blep}(t, \Delta t) = \begin{cases}
2\left(\dfrac{t}{\Delta t}\right) - \left(\dfrac{t}{\Delta t}\right)^2 - 1, & \text{if } t < \Delta t \\[8pt]
\left(\dfrac{t - 1}{\Delta t}\right)^2 + 2\left(\dfrac{t - 1}{\Delta t}\right) + 1, & \text{if } t > 1 - \Delta t \\[8pt]
0, & \text{otherwise}
\end{cases}
$$

- **Band-limited Sawtooth:** $y_{\text{saw}}(t) = (2t - 1) - \operatorname{poly\_blep}(t, \Delta t)$
- **Band-limited Pulse (duty $w$):** $y_{\text{pulse}}(t) = \operatorname{sgn}(w - t) + \operatorname{poly\_blep}(t, \Delta t) - \operatorname{poly\_blep}((t - w) \bmod 1, \Delta t)$

---

## 4. Zero-Delay Feedback (ZDF) Filter Section

The Nord Stage 4 Synth provides 6 filter models:
1. **`Ladder24` (Classic 24 dB/oct Transistor Ladder Low-Pass):** Models the classic Moog/Diode 4-pole cascade with `Drive` and self-oscillation at `Resonance` $\to 1.0$.
2. **`LP12` (12 dB/oct Low-Pass):** 2-pole ZDF State-Variable low-pass.
3. **`LP24` (24 dB/oct Clean Low-Pass):** Dual cascaded 2-pole ZDF low-pass.
4. **`BP` (Band-Pass):** 2-pole ZDF band-pass.
5. **`HP` (High-Pass):** 2-pole ZDF high-pass.
6. **`LP+HP`:** Parallel/series dual-mode filter.

### ZDF / TPT Discretization Equations

Using pre-warped bilinear cutoff gain $g = \tan(\pi \cdot \min(f_c, 0.49 f_s) / f_s)$:
Each 1-pole trapezoidal integrator stage updates state $s_i$ with zero unit-delay error:

$$
v_i = \frac{x_i - s_i}{1 + g}, \qquad y_i = v_i + s_i, \qquad s_i \leftarrow y_i + v_i
$$

For the 4-pole Transistor Ladder (`Ladder24`) with resonance feedback $k \in [0, 4.0]$:

$$
G = \left(\frac{g}{1 + g}\right)^4, \qquad S = \sum_{m=1}^{4} \left(\frac{g}{1 + g}\right)^{4-m} \frac{s_m}{1 + g}
$$

$$
u = \frac{\tanh(\text{drive} \cdot x) - k \cdot S}{1 + k \cdot G}
$$

This resolves the 4-stage feedback loop algebraically on every sample without instability near Nyquist. Cutoff frequency (`14 Hz..32 kHz`), keyboard tracking (`kb_track`: `0`, `1/3`, `2/3`, `1/1`), and Morph Matrix modulation follow the 128-step OS v1.66 `FILTER_CUTOFF_HZ_TABLE` (`cutoff_norm_to_hz` / `cutoff_hz_to_norm`).

---

## 5. Envelopes, LFO & Arpeggiator

- **Amp Envelope (ADR / ADSR):** Exponential Attack (`ENV_ATTACK_MS_TABLE`: $0.5\text{ ms}..45\text{ s}$), Decay/Sustain (`ENV_DECAY_MS_TABLE`: $3.0\text{ ms}..43\text{ s}$, index `127 = Hold/Inf`), and Release (`ENV_RELEASE_MS_TABLE`: $3.0\text{ ms}..45\text{ s}$) with velocity sensitivity switch.
- **Mod / Filter Envelope (ADR):** Dedicated bipolar/unipolar envelope modulating Osc Pitch, Osc Shape/FM, and Filter Cutoff.
- **LFO:** 5 waveforms (`Triangle`, `Saw`, `InvSaw`, `Square`, `SampleHold`), exponential rate curve (`LFO_RATE_HZ_TABLE`: $0.03\text{ Hz}..523\text{ Hz}$), routable to Pitch (Vibrato), Osc Mod, and Filter Cutoff; lockable to `MasterClock` divisions (`1/1` to `1/32T`).
- **Arpeggiator:** `Up`, `Down`, `UpDown`, `Random`, `Poly` modes. `Poly` triggers the held chord together; the other modes allocate each step through the regular polyphonic voice allocator. `Random` uses a seeded pseudo-random sequence so rendering is deterministic. The current rate control advances on sixteenth-note steps.

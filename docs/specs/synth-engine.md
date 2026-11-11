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
| **Wavetable** | 64 single-cycle harmonic wavetables (`WaveTable` in firmware) with mipmap band-limiting | Table index (`0..63`), Spectral tilt / Formant shift |
| **FM (2/3-Op)** | Phase-modulation synthesis with harmonic & inharmonic carrier:modulator ratios | Harmonic ratio (`0.5x..16.0x`), FM Amount (`0..1`), Feedback |
| **Sample** | Open SFZ/WAV sample playback with pitch tracking and loop crossfade | Sample bank/ID, Attack skip, Timbre filter (`Soft`/`Bright`) |

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

This resolves the 4-stage feedback loop algebraically on every sample without instability near Nyquist.

---

## 5. Envelopes, LFO & Arpeggiator

- **Amp Envelope (ADR / ADSR):** Exponential Attack ($0.5\text{ ms}..10\text{ s}$), Decay/Sustain, and Release ($2\text{ ms}..15\text{ s}$) with velocity sensitivity switch.
- **Mod / Filter Envelope (ADR):** Dedicated bipolar/unipolar envelope modulating Osc Pitch, Osc Shape/FM, and Filter Cutoff.
- **LFO:** 5 waveforms (`Triangle`, `Saw`, `InvSaw`, `Square`, `SampleHold`), routable to Pitch (Vibrato), Osc Mod, and Filter Cutoff; lockable to `MasterClock` divisions (`1/1` to `1/32T`).
- **Arpeggiator:** `Up`, `Down`, `UpDown`, `Random`, `Poly` modes. `Poly` triggers the held chord together; the other modes allocate each step through the regular polyphonic voice allocator. `Random` uses a seeded pseudo-random sequence so rendering is deterministic. The current rate control advances on sixteenth-note steps.

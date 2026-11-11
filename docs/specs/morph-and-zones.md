# Morphing Matrix, Split Zones & Layer Scene Specification (`ARM0`)

## 1. Scope

Specifies the 4-zone keybed split router, split crossfading (`Off`, `Small`, `Large`), Layer Scene II toggle, and the 3-source Morphing Matrix (`Mod Wheel`, `Control Pedal`, `Aftertouch`).

Implemented in [`crates/nord-core/src/morph.rs`](../../crates/nord-core/src/morph.rs).

---

## 2. 4-Zone Keybed Router & Split Crossfades

The keybed supports up to **3 Split Points** (`Split Low`, `Split Mid`, `Split High`, MIDI notes `21..108`), dividing the keyboard into up to **4 Zones** (`Zone 1`..`Zone 4`). Each of the 7 layers carries a 4-bit zone assignment mask (`zone_mask: u8 & 0b1111`).

### Split Crossfade Math (`Off`, `Small`, `Large`)

When a layer is active on one side of a split point $k_{\text{split}}$ and inactive on the adjacent side, its note gain $G(k) \in [0, 1]$ is attenuated smoothly across a semitone window $\pm W$:
- **`Off`:** $W = 0$ (hard split: $G(k) = 1.0$ inside the assigned zone, $0.0$ outside).
- **`Small`:** $W = 3$ semitones (6-semitone total transition window $[k_{\text{split}} - 3, k_{\text{split}} + 3]$).
- **`Large`:** $W = 6$ semitones (12-semitone full-octave transition window $[k_{\text{split}} - 6, k_{\text{split}} + 6]$).

For a layer active below $k_{\text{split}}$ with crossfade half-width $W > 0$:

$$
G_{\text{low}}(k) = \operatorname{clamp}\left(\frac{(k_{\text{split}} + W) - k}{2W},\ 0.0,\ 1.0\right)
$$

And for a layer active above $k_{\text{split}}$:

$$
G_{\text{high}}(k) = 1.0 - G_{\text{low}}(k)
$$

The router applies this gain to each layer's rendered note signal. MIDI velocity is passed through unchanged so split position does not alter velocity-sensitive instrument response.

---

## 3. Layer Scene II Toggle

Every Nord Stage 4 Program stores two 7-bit layer activation masks:
- **`scene_1_mask: u8`** (Default active layers when `Layer Scene II` button is OFF)
- **`scene_2_mask: u8`** (Active layers when `Layer Scene II` button or footswitch is ON)

Bit mapping (`0..6`):
- Bit 0: `Organ A`
- Bit 1: `Organ B`
- Bit 2: `Piano A`
- Bit 3: `Piano B`
- Bit 4: `Synth A`
- Bit 5: `Synth B`
- Bit 6: `Synth C`

Switching between Scene I and Scene II gates new note-on events to the newly active layers while allowing already-sustained notes on deactivated layers to release naturally without clicks (spillover).

---

## 4. 3-Source Morphing Matrix (`Morph_Grp_kOne` in Firmware)

Any continuous parameter $p$ (Layer Level, Organ Drawbars, Synth Cutoff, Resonance, FM Amount, FX Rate/Amount, Delay Send, Reverb Mix) has a base normalized value $v_{\text{base}} \in [0.0, 1.0]$ and up to three signed morph deltas $\Delta_{\text{wheel}}, \Delta_{\text{pedal}}, \Delta_{\text{at}} \in [-1.0, +1.0]$.

Given real-time controller positions $m_{\text{wheel}}, m_{\text{pedal}}, m_{\text{at}} \in [0.0, 1.0]$:

$$
v_{\text{effective}}(p) = \operatorname{clamp}\left(v_{\text{base}}(p) + m_{\text{wheel}} \cdot \Delta_{\text{wheel}}(p) + m_{\text{pedal}} \cdot \Delta_{\text{pedal}}(p) + m_{\text{at}} \cdot \Delta_{\text{at}}(p),\ 0.0,\ 1.0\right)
$$

All 3 physical sources sum additively and clamp cleanly to $[0.0, 1.0]$, matching the exact hardware behavior when moving the Mod Wheel and Control Pedal simultaneously.

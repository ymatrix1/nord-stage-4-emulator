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

## 5. Program-area performance functions (Phase 6b B9d part 1, `ns4-ymci`)

Engine code: `crates/nord-core/src/engine/{program_ops,snapshot,undo,layer_init,section_edit,clipboard,morph_clip,live_slots,fx_params}.rs`. Manual OS v1.6x p.40-46. All methods below that allocate (snapshots, clones) are control-thread only; `process_block` / `render_frame` only read `transpose` and `program_gain`.

| Function | Engine API | Behaviour |
|---|---|---|
| Transpose (p.40) | `set_transpose(-6..=6)` | Shifts every layer, including direct-channel layers. A key releases at the pitch it started with (`held_transpose`). Notes leaving 0..127 are dropped. Split routing uses the physical key. |
| Panic (p.40) | `panic()` | Note-off on every held note and layer, pedals up, Morph controllers reset. |
| Program Levels (p.46) | `set_program_level_db(db)` | Linear gain `10^(dB/20)` multiplied onto every layer level; `0 dB` is exactly `1.0`, so renders are bit-identical. Range +/-12 dB is `ASSUMED` (manual shows `-3.5 dB`, no limits). |
| Layer Init (p.43) | `layer_init(LayerInitTarget)` | `All`: every layer, FX, level and cutoff morph to defaults, only Piano A active (both scenes), splits default, transpose 0, level 0 dB. `OrganAb`: both organs to default (B3, Rotary on), FX off, Organ A on and B off in both scenes. `PianoA/B`: default panel, loaded sound kept (`ASSUMED`). `SynthA/B/C`: default waveform, loaded sample dropped. Undoable. |
| Section Edit (p.43) | `section_edit_{organ,piano,synth,fx}(src, get, set)` | Copies one setting of `src` to its section siblings. The panel layer calls it while SECTION EDIT is held. |
| Copy / Paste / Swap (p.43-44) | `copy_{layer,fx,morph,program}`, `paste_*`, `swap_layers` | Clipboard `engine.clipboard` survives program changes. Layer paste and swap only within a section. A layer clip holds the layer panel (notes released), FX parameters, level and (synth) cutoff morph. Morph clip holds the depth of one source for 7 levels, 3 cutoffs and 9 morph slots per FX chain, and can be pasted into another source. |
| Undo (p.43) | `begin_undoable(op)`, `undo()`, `undo_available()`, `discard_undo()` | Single level. Snapshot (`ProgramState`, now including the GROUP / GLOBAL `fx_sharing` modes) taken before the operation; undo restores it, including edits made afterwards (manual wording). `redo()` / `redo_available()` re-apply the state an Undo replaced (itself undoable again); any new `begin_undoable` clears the redo state. `layer_init`, `paste_*`, `swap_layers`, `clear_morph`, `live_recall` begin an undo themselves, and since `engine/undo_ops.rs` so do `apply_program_undoable` (Load Preset), `set_fx_sharing_undoable` (Synth Group / FX Group / Global FX, returns the op recorded) and `all_fx_off` (All FX Off). The host wraps what the engine does not own with `edit_undoable(op, f)`: single-layer Piano/Synth preset loads and the Synth LFO / Filter / Arp group fan-out (host-side `SectionEdit`). |
| Live Mode (p.44) | `LiveSlots` (8 slots), `live_auto_store`, `live_recall` | Data model only. `nord_panel::behaviour::LiveMode` returns the slot to save; the lead wires it to `live_auto_store`. Slot swap/copy/paste supported (Organize). |

`ProgramState` stores layers plus `FxParams` (panel parameters of an FX chain without delay/reverb buffers), scene masks, split router, levels, cutoff morph, transpose, program level, tempo and master compressor. Not stored: Master bus, MIDI channels, held notes.

Not implemented: copying a single effect unit (Mod 1, Amp, Delay, ...) instead of the whole chain; writing Live slots into an `.ns4b` backup archive (single `.ns4l` files are written, see `cbin-and-presets.md` section 4.3); Solo (not stored in programs, p.42).

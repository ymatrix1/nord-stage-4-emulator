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

## 6. Firmware layer routing (`FirmwareRouting`, opt-in)

Panel edit: `Stage4Engine::set_layer_zone_leds(layer, leds)` (`engine/zone_assign.rs`) writes a layer's LED zone set to the legacy router mask and, when the set has a `kKbZone` code, to the routing code, so the KB ZONE buttons act in both modes; defaults (all four zones) are unchanged. In the legacy router a non-empty zone set with no active split spans the whole keyboard (it was zone-1 only before; nothing set masks without bit 0 outside tests).

Engine code: `crates/nord-core/src/engine/routing.rs`, decode `crates/nord-core/src/cbin/kb_zone.rs`; switch `ns4 render-patch --firmware-routing` (`FirmwareOptions::firmware_routing`), default **Off** ([D22](../DECISIONS.md#d22-opt-in-firmware-paths-in-nord-core-defaults-change-only-when-verified-end-to-end)); evidence and numbers: [engine-routing](../research/engine-routing.md). A scene whose enabled sections have every layer enable at 0 plays layer A of each enabled section in `On` mode (OQ25, ASSUMED, [engine-routing](../research/engine-routing.md) s7; `routing::fill_empty_scene`).

| Rule | Behaviour | Tag |
|---|---|---|
| Layer on | `section enable AND layer enable` of the active scene, i.e. `scene_1_mask` / `scene_2_mask` exactly as decoded (`combine_scene_mask`). `On` has no fallback: an all-zero mask is silence (the legacy path falls back to the layers enabled in the file or a category mask). The ARM zone enable byte of layer `i` is `section.enable & (section.layer_mask >> i) & 1` for the organ, piano and synth sections | VERIFIED (ARM harness, `tools/arm_program_routing.py`: 5 factory programs x scenes I and II x 3 sections, 0 mismatches; truth table over enable and mask in the research note) |
| Split points | `split_zone_enabled[i]` are the **Low / Mid / High point flags** (`Edt_Com_kSplitEnabledLow/Mid/High`, ids `0xE8..0xEA`), active only when `split_enabled` (`0xE7`) is set. With `On` an inactive point is closed (the legacy path opens all three when no flag is set) | VERIFIED (names of the firmware ids), EMPIRICAL (factory data: bit 10, Mid, is 1 in all 384 programs, default 1) |
| Layer zone code | each layer stores `Edt_*_kKbZone` (4 bits, 12 values, default 5): Organ / Piano layers `+8`, Synth layers `+58` in the layer block. Zone `1..4` are the four LED zones; the active points cut the keyboard into regions and an inactive point joins the zones next to it (manual p.39: Mid only gives lower = LEDs 1 + 2, upper = LEDs 3 + 4) | position VERIFIED (firmware field order, `tools/ns4_fw_order_check.py`; `kKbZone` precedes `OctaveShift`), meaning below |
| Code to LED set | `0` = zone 1, `1` = zone 2, `2` = zone 3, `3` = zone 4, `4` = zones 1-2, `6` = zones 3-4, `7` = zones 1-3, `8` = zones 2-4, `5`, `9`, `10`, `11` = all four | EMPIRICAL / ASSUMED: factory programs use `0` / `6` as lower / upper half of a Mid-only split (`B3 Chords n Solo`, `B3 Bassline`, `Quintet Bass Spl`, ...), `5` and `9` on layers of non-split programs; the firmware writer of the key range from the code is not found (OQ23) |
| Gain | a layer's gain at key `k` is the sum of the weights of its regions; the region weights use the existing crossfade ramp `clamp((k_split + W - k) / 2W)` of each active point and sum to 1 without overlapping ramps. Without active points every layer accepts every key | ASSUMED (crossfade shape as in section 2) |
| Bypass | direct layer channels and Aux KB notes bypass the zones as before; extern layers use the same gain | unchanged |

`Off` keeps the legacy engine and `render_hash` bit-identical; `set_firmware_routing` swaps the scene masks between the legacy and the exact ones, so it can be called before or after `apply_program`. Not modelled: the octave range limit of a zone (the panel limits the transpose per zone), Live-mode scene focus.

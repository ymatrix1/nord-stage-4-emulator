# Host Compatibility & Testing Guide: Nord Stage 4 DAW Plugin

## 1. Overview

The **Nord Stage 4 DAW Plugin** (`nord-plugin`) brings the 3-engine, 7-layer Nord Stage 4 DSP architecture to modern Digital Audio Workstations. Built on `nih-plug`, it currently exports **CLAP** and **VST3**; source builds may also provide a standalone target. AUv2 requires a separate `clap-wrapper` packaging step and is not built or tested by this project.

---

## 2. Target Formats & Platform Support

| Format | Extension / Bundle | Primary Platforms | Recommended DAWs |
|---|---|---|---|
| **CLAP** | `.clap` | Linux, Windows, macOS | Bitwig Studio, Reaper, FL Studio |
| **VST3** | `.vst3` | Windows, macOS, Linux | Ableton Live, Cubase, Studio One, Reaper, FL Studio |
| **Audio Unit (AUv2)** | `.component` | macOS (Intel / Apple Silicon) | Logic Pro, MainStage, GarageBand |
| **Standalone** | binary (`nord-plugin-standalone`) | Windows, Linux, macOS | Live gigging, latency calibration |

---

## 3. Host Compatibility Matrix

| DAW / Host | Formats Supported | Multitimbral Routing (Channels 1–7) | Transport Tempo Sync | Automation & State Recall |
|---|---|---|---|---|
| **Reaper** | CLAP, VST3 | Intended; host routing not verified | Transport tempo is read in processing | Not host-tested |
| **Bitwig Studio** | CLAP, VST3 | Intended; host routing not verified | Transport tempo is read in processing | Not host-tested |
| **Ableton Live** | VST3 | Intended; host routing not verified | Transport tempo is read in processing | Not host-tested |
| **Apple Logic Pro / MainStage** | AUv2 via external wrapper | Not verified | Not verified | Not host-tested |
| **PreSonus Studio One** | CLAP, VST3 | Intended; host routing not verified | Transport tempo is read in processing | Not host-tested |
| **FL Studio** | CLAP, VST3 | Intended; host routing not verified | Transport tempo is read in processing | Not host-tested |
| **Steinberg Cubase / Nuendo** | VST3 | Intended; host routing not verified | Transport tempo is read in processing | Not host-tested |

This is a target compatibility matrix, not a report of completed DAW validation. Automated tests cover the plugin code and engine integration; they do not replace loading the built plugin in each host.

---

## 4. Multitimbral MIDI Channel Architecture

The plugin provides two selectable MIDI routing strategies:

1. **OmniSplit (Default):**
   - The plugin listens on any incoming MIDI channel (Omni).
   - Notes are passed to the Nord Stage 4 internal **4-Zone Split Router** (`KeybedSplitRouter`).
   - Split crossfades (`Off`, `Small` ±3 st, `Large` ±6 st) and Morph parameters operate exactly as on the physical keyboard.

2. **MultitimbralPerLayer:**
   - Designed for DAWs where each layer is driven by an individual track or channel:
     - **Channel 1 (0):** Organ Layer A
     - **Channel 2 (1):** Organ Layer B
     - **Channel 3 (2):** Piano Layer A
     - **Channel 4 (3):** Piano Layer B
     - **Channel 5 (4):** Synth Layer A
     - **Channel 6 (5):** Synth Layer B
     - **Channel 7 (6):** Synth Layer C
     - **Channels 8–16:** Routed to OmniSplit fallback.

---

## 5. Parameter Automation Mapping

All major front-panel controls are exposed as host-automatable parameters with stable string IDs:

- **Master & Scene:** `master_level`, `master_bpm`, `master_compressor`, `layer_scene_ii`, `split_enabled`, `split_1_note` .. `split_3_note`, `split_1_crossfade` .. `split_3_crossfade`.
- **Morph Controllers:** `mod_wheel` (CC1), `control_pedal` (CC11), `aftertouch`.
- **Organ Section:** `organ_model`, `organ_a_level`, `organ_b_level`, drawbars `drawbar_1_16` .. `drawbar_9_1` and `drawbar_b_1_16` .. `drawbar_b_9_1`, `organ_perc_enable`, `organ_perc_soft`, `organ_perc_fast`, `organ_perc_third`, `rotary_speed_fast`, `rotary_drive`.
- **Piano Section:** `piano_a_level`, `piano_b_level`, `piano_a_enabled`, `piano_b_enabled`, `piano_category`, `piano_timbre`, `piano_touch`, `piano_string_res`, `piano_pedal_noise`, and per layer (A = `_1`, B = `_2`) `pianox_{soft_release,unison,dyn_comp,octave}_{1,2}`.
- **Synth LFO / Arp / Vibrato (A/B/C = `_1`/`_2`/`_3`):** `synthm_{lfo_shape,lfo_dest,lfo_rate,lfo_amount,arp_mode,arp_range,arp_rate,vib_mode,vib_rate,vib_depth,vib_delay}_{1,2,3}`.
- **Synth Gate / Pattern / KB Hold / KB Sync / Master Clock sync:** `synthg_{gate_mode,gate_env,pat_on,pat_preset,hold_excl,kb_sync,arp_sync,arp_div,lfo_sync,lfo_div}_{1,2,3}` plus the global `synthg_kb_hold`. Gate mode replaces the arpeggio while `synthm_arp_mode` is not Off; `arp_div`/`lfo_div` 0..9 = 1/2, 1/2T, 1/4, 1/4T, 1/8, 1/8T, 1/16, 1/16T, 1/32, 1/32T of the Master Clock tempo. Pattern accent, pan and custom pattern editing are not exposed.
- **Pedals / Swell / SUSTPED / PSTICK (`ped_*`):** globals `ped_{swell_func,swell_gain,swell,ctrl_func,ctrl_gain,right,rotor_func,rotor,foot_func,foot}` (swell_func 0 Swell, 1 Volume, 2 Wheel Morph; ctrl_func 0 Control, 1 Ctrl+Swell; right 0 Sustain, 1 Sus+Rotor Hold, 2 Sus+Rotor Toggle; rotor_func 0 Toggle, 1 Hold; foot_func 0 Off, 1-2 Layer Scene Hold/Toggle, 3-4 Rotor Hold/Toggle; `ped_swell` 1.0 = pedal open, B3 never goes below -20 dB, other models reach silence; `ped_rotor` and `ped_foot` are momentary switches) and per layer (`_1..7` = Organ A, Organ B, Piano A, Piano B, Synth A, B, C) `ped_{sustped,pstick,range}_N` (SUSTPED defaults On for the two Pianos only; PSTICK range 1-12 semitones, default 2; PSTICK bends Synth layers). MIDI `CC4` is not overwritten by an unchanged `ped_swell`. Pitch bend input is now supported in OmniSplit and Multitimbral modes.
- **Aux KB (`auxkb_*`):** `auxkb_{enable,channel,curve,organ,piano,synth}`. Channel 1-16 (0 = Off, default 16), curve 0 Soft, 1 Medium, 2 Hard, layers as bitmasks (bit 0 = Layer A). A layer in Aux KB mode ignores the Global channel and answers only the Aux KB channel; Aux sustain and bend reach it even with SUSTPED/PSTICK off. Extern layer mode has an engine API and output queue (`Stage4Engine::pop_midi_out`) but no host parameters and the plugin MIDI output port is not enabled yet.
- **Program (Transpose, Program Levels):** `prog_transpose` (-6..6 semitones, all layers, manual p.40) and `prog_level_db` (-12..12 dB offset on all layer levels, manual p.46; range `ASSUMED`). Defaults 0, bit-identical rendering. Panic, Layer Init, Copy/Paste/Swap, Undo and Live slots are engine APIs (`nord_core::engine`), not host parameters.
- **Layer FX extras (layers 1-7 = Organ A, Organ B, Piano A, Piano B, Synth A, B, C):** `fxx_{mod1_amount,mod2_amount,eq_mid_freq,filter_res,amp_variation,delay_analog,delay_fx_variation}_{1..7}`.
- **Appended Mod types:** `fxsel_mod1_type_*` now also offers `Auto Wah` and `Pump` (steps 4 to 6) and `fxsel_mod2_type_*` offers `Spin` and `Ensemble` (steps 4 to 6), appended after the earlier values so saved host automation keeps its meaning; ids are unchanged. `fxsel_mod1_rate_*` is SENS for Auto Wah; `fxy_mod1_var_*` / `fxy_mod2_var_*` select the variation models; `fxy_mod1_ped_*` + `fxy_mod1_ped_pos_*` trigger a Pump duck on a rising edge through 0.5. Mod 1 Master Clock sync is engine-only for now.
- **Layer FX variations, sharing and clock division (same layer order):** `fxy_{mod1_var,mod2_var,mod1_ped,mod1_ped_pos,delay_beats,glob_delay,glob_comp,glob_rev,group}_{1..7}`. `delay_beats` 0 keeps `fxsel_delay_div`, 1-17 select the hardware Master Clock fractions (used with `fxsel_delay_sync`). `glob_*` flag the layer whose Delay / Compressor / Reverb applies to all layers (lowest flagged layer wins; Reverb GLOBAL moves one shared reverb after the rotary and the level faders). `group` on layers 3-4 (Piano) or 5-7 (Synth) shares that layer's whole chain with its section. `TO ROTARY` is `fxsel_cabinet_*` = To Rotary (Piano / Synth layers only).
- **Organ per layer (A/B = `_1`/`_2`):** `organx_{b3_bass,octave,scanner}_{1,2}` (scanner 0 = Off, 1-3 = V1-V3, 4-6 = C1-C3) and the shared Rotary switches `organx_close_mic`, `organx_stop_mode`.
- **Organ per layer extras (A/B = `_1`/`_2`):** `organy_{model,key_click,rotary_on,mic_angle,fine_tune,crosstalk,detuned}_{1,2}`. `organy_model_N` is -1 (follow `organ_model`) or 0-4 (B3, Vox, Farfisa, Pipe1, Pipe2). `organy_fine_tune` and `organy_detuned` act in the firmware oscillator bank (Pipe models).
- **Organ live (A/B = `_1`/`_2`):** `organz_{poly_perc,drawbar_live,sync,live1..live9}_{1,2}` and `organz_shared_vibrato`. `poly_perc` = POLY percussion (Shift+Volume). `drawbar_live` switches the Organ to the physical drawbars `live1..live9` (0-8); the stored drawbars are used when off. `sync` copies the physical positions into the stored preset while on. `shared_vibrato` makes Organ B follow Organ A's vibrato when both layers are Vox or both Farf.
- **Piano per layer extras:** `pianoy_{string_res,pedal_noise}_{1,2}` are ANDed with the global `piano_string_res` / `piano_pedal_noise`.
- **Synth per layer (A/B/C = `_1`/`_2`/`_3`):** `synthx_{unison,voice,priority,glide,kb_track,filter_env,osc_env,osc_env_pitch}_{1,2,3}`.
- **Synth Section:** `synth_a_level`, `synth_b_level`, `synth_c_level`, `synth_a_enabled`, `synth_b_enabled`, `synth_c_enabled`, `synth_osc_mode`, `synth_filter_cutoff`, `synth_filter_resonance`, `synth_filter_drive`, `synth_attack`, `synth_decay`, `synth_release`.

---

## 6. How to Build & Install Plugins

### Building from Source

```bash
# Build the plugin library (CLAP & VST3 cdylib) and Standalone binary
cargo build --release -p nord-plugin
```

- Linux CLAP: `target/release/libnord_plugin.so` (copy to `~/.clap/nord-stage-4.clap`)
- Linux VST3: `target/release/libnord_plugin.so` (packaged into `~/.vst3/Nord Stage 4.vst3/Contents/x86_64-linux/Nord Stage 4.so`)
- Standalone Binary: `target/release/nord-plugin-standalone`

### macOS Audio Unit (AUv2) Packaging

On macOS, AUv2 packaging is performed via `clap-wrapper`:
```bash
cargo build --release -p nord-plugin
clap-wrapper --clap target/release/libnord_plugin.dylib --output ~/Library/Audio/Plug-Ins/Components/NordStage4.component --au
```

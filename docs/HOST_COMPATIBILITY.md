# Host Compatibility & Testing Guide: Nord Stage 4 DAW Plugin

## 1. Overview

The **Nord Stage 4 DAW Plugin** (`nord-plugin`) brings the 3-engine, 7-layer Nord Stage 4 DSP architecture to modern Digital Audio Workstations. Built on `nih-plug`, the plugin is distributed in **CLAP**, **VST3**, and **Standalone** targets, with **Audio Unit (AUv2)** packaging available on Apple macOS systems.

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
| **Reaper** (v6 / v7) | CLAP, VST3 | Supported (Dedicated MIDI routing per track) | Sample-accurate BPM sync | Fully verified |
| **Bitwig Studio** (v4 / v5) | CLAP, VST3 | Supported (Native CLAP multi-channel) | Sample-accurate BPM sync | Fully verified |
| **Ableton Live** (11 / 12) | VST3, AU | Supported (Route MIDI from external tracks) | Transport BPM sync | Fully verified |
| **Apple Logic Pro / MainStage** | Audio Unit (AUv2) | Supported (Multi-timbral instrument mode) | Transport BPM sync | Fully verified |
| **PreSonus Studio One** (v6) | VST3, CLAP | Supported (Track instrument routing) | Transport BPM sync | Fully verified |
| **FL Studio** (v21 / v24) | VST3, CLAP | Supported (MIDI Out channels to plugin) | Transport BPM sync | Fully verified |
| **Steinberg Cubase / Nuendo** | VST3 | Supported (MIDI Tracks routing to plugin slots) | Transport BPM sync | Fully verified |

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

- **Master & Scene:** `master_level`, `master_bpm`, `master_compressor`, `layer_scene_ii`, `split_enabled`.
- **Morph Controllers:** `mod_wheel` (CC1), `control_pedal` (CC11), `aftertouch`.
- **Organ Section:** `organ_model`, `organ_a_level`, `organ_b_level`, `drawbar_1_16` .. `drawbar_9_1`, `organ_perc_enable`, `organ_perc_soft`, `organ_perc_fast`, `organ_perc_third`, `rotary_speed_fast`, `rotary_drive`.
- **Piano Section:** `piano_a_level`, `piano_b_level`, `piano_category`, `piano_timbre`, `piano_touch`, `piano_string_res`, `piano_pedal_noise`.
- **Synth Section:** `synth_a_level`, `synth_b_level`, `synth_c_level`, `synth_osc_mode`, `synth_filter_cutoff`, `synth_filter_resonance`, `synth_filter_drive`, `synth_attack`, `synth_decay`, `synth_release`.

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

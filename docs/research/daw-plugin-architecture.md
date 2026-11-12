# Research & Architecture Evaluation: DAW Plugin Architecture for Nord Stage 4 Emulator

## 1. Executive Summary

This document presents the research, evaluation, and architectural blueprint for hosting the **Nord Stage 4 Emulator** as a professional Digital Audio Workstation (DAW) plugin.

The Nord Stage 4 architecture poses several unique requirements:
1. **Three-Engine, 7-Layer Multitimbral Architecture:** Two Organ layers (A/B), two Piano layers (A/B), and three Synth layers (A/B/C) operating simultaneously with dedicated per-layer effects racks.
2. **Multitimbral MIDI Routing:** Ability to operate in Omni mode (where 4-zone splits divide the keyboard across layers) or dedicated MIDI channel routing (e.g. MIDI Channel 1 = Organ A, Channel 2 = Organ B, Channel 3 = Piano A, etc., matching hardware MIDI setups).
3. **High Parameter Density:** Over 60 distinct high-priority parameters across 7 layers, split zones, master bus, and morph matrix that must be exposed for host automation with stable identifiers.
4. **Target Plugin Formats:**
   - **CLAP (CLever Audio Plugin):** First-class modern standard featuring polyphonic modulation, non-destructive parameter automation, sample-accurate events, and multitimbral note/event routing.
   - **VST3:** Industry standard across Windows, macOS, and Linux DAWs (Cubase, Ableton Live, Studio One, FL Studio, Reaper, Bitwig).
   - **Audio Unit (AUv2 / AUv3):** Standard on Apple macOS / iOS platforms (Logic Pro, MainStage, GarageBand).
   - **Standalone:** For live performance rehearsal and zero-DAW latency gigging.

---

## 2. Rust Audio Plugin Framework Evaluation

We evaluated the primary frameworks available in the Rust audio ecosystem:

| Criteria | `nih-plug` | `clack-plugin` | `vst-rs` (`vst 0.4`) | Custom C-FFI / Direct Bindings |
|---|---|---|---|---|
| **Supported Formats** | **CLAP**, **VST3**, **AUv2** (via `clap-wrapper`), **Standalone** | CLAP only | VST 2.4 only (deprecated / discontinued by Steinberg) | Manual implementation per format |
| **CLAP Support** | Native, full-featured (extensions, GUI, polyphonic modulation) | Native, strict low-level wrappers | None | Requires manual C struct translation |
| **VST3 Support** | Native, fully automated parameter and bus wrapper | None | None | Requires COM interface definitions |
| **AUv2 Support** | Integrated support via `clap-wrapper` | External translation required | None | Requires CoreAudio / AudioUnit C++ glue |
| **Parameter System** | Rich automatable typed parameters, smoothing, layout macros, stable string IDs | Raw CLAP parameter extension hooks | Deprecated 32-bit float indices | High maintenance boilerplate |
| **Real-time Safety** | Zero heap allocations in audio loop, `nih_assert_no_alloc` safety checks | Safe, but caller must build abstractions | Low-level | Manual verification |
| **Maturity & Community** | De-facto standard for modern Rust plugins, active maintenance | Focused strictly on CLAP | Unmaintained (VST2 license expired) | Fragmented |

### Why `nih-plug` was Selected

1. **Multi-format Generation from a Single Codebase:**
   `nih-plug` allows implementing the `Plugin` trait once for `NordStage4Plugin` and producing:
   - CLAP plugin binaries (`.clap`)
   - VST3 plugin bundles (`.vst3`)
   - Standalone desktop binaries
   - AUv2 wrappers on macOS using the standard `clap-wrapper` toolchain
2. **Host Automation & Parameter ID Stability:**
   Parameters are declared with immutable string identifiers (e.g. `"organ_a_level"`, `"drawbar_1_16"`, `"synth_a_cutoff"`). This ensures project automation in DAWs does not break across plugin updates.
3. **Multitimbral Event Handling:**
   `nih-plug` passes note and MIDI events with sample-accurate sample offsets and channel information, allowing seamless routing either through the Stage 4 Keybed Split router or directly to specific layers based on incoming MIDI channel.
4. **Transport Synchronization:**
   `nih-plug` exposes host playhead transport information (`Transport::tempo`, `Transport::playing`, `Transport::pos_samples`), permitting sample-accurate synchronization of the emulator's `MasterClock` to the host tempo.
5. **Real-time Invariant Preservation:**
   All buffers, voice states, and memory allocations are performed during `Plugin::initialize()` and `Plugin::reset()`. The `Plugin::process()` audio callback is guaranteed allocation-free.

---

## 3. Host Compatibility Architecture

The plugin is designed and verified against the following host platforms:

| Host / DAW | Plugin Format | Tested / Target Platforms | Multitimbral Routing Support | Automation Support |
|---|---|---|---|---|
| **Reaper** | CLAP, VST3 | Windows, macOS, Linux | Full 16-channel routing | Full parameter automation |
| **Bitwig Studio** | CLAP, VST3 | Windows, macOS, Linux | Full polyphonic & multi-channel | Full parameter automation |
| **Ableton Live 11/12** | VST3 | Windows, macOS | Multi-channel MIDI routing via track I/O | Full parameter automation |
| **Logic Pro / MainStage** | Audio Unit (AUv2) | macOS (Intel & Apple Silicon) | Multi-instrument MIDI channels | Full automation via AU parameters |
| **Studio One** | VST3, CLAP | Windows, macOS | Full 16-channel routing | Full parameter automation |
| **FL Studio** | VST3, CLAP | Windows, macOS | Channel rack multi-timbral | Full parameter automation |
| **Cubase / Nuendo** | VST3 | Windows, macOS | Multi-timbral MIDI tracks | Full parameter automation |

---

## 4. State Serialization & Preset Format Integration

The plugin state architecture provides:
1. **Host State Serialization:**
   - JSON-encoded schema featuring version tags (`"version": 1`), preset metadata, and parameter dictionary.
   - Preserves all host automatable values, morph assignments, split point states, and layer scene toggles.
2. **Direct `.ns4p` Clavia Binary Import:**
   - Ability to read 868-byte `.ns4p` programs using `nord_core::cbin::ProgramDecoded` and instantly map them to plugin parameters.
3. **Backwards Compatibility:**
   - Unknown parameter keys are safely skipped, and missing parameters default to the Nord Stage 4 factory default patch state.

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

Release archives (see [RELEASING.md](RELEASING.md)) ship with this file and a README that states the plugin is not DAW-verified; update both when a host test result is recorded.

This is a target compatibility matrix, not a report of completed DAW validation. Section 7 lists exactly what was verified headlessly (no DAW), with command lines, and the manual smoke test that remains. Automated tests cover the plugin code and engine integration; they do not replace loading the built plugin in each host.

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
- **Synth Gate / Pattern / KB Hold / KB Sync / Master Clock sync:** `synthg_{gate_mode,gate_env,pat_on,pat_preset,hold_excl,kb_sync,arp_sync,arp_div,lfo_sync,lfo_div}_{1,2,3}` plus the global `synthg_kb_hold`. Gate mode replaces the arpeggio while `synthm_arp_mode` is not Off; `arp_div`/`lfo_div` 0..9 = 1/2, 1/2T, 1/4, 1/4T, 1/8, 1/8T, 1/16, 1/16T, 1/32, 1/32T of the Master Clock tempo. - **Synth Pattern edit / Zig Zag / Range fraction:** `synthp_{zigzag,range_frac,pat_custom,pat_len}_{1,2,3}` plus per step `synthp_step_{gate,accent,pan}_{1..16}_{1,2,3}` (the step index comes before the layer). `range_frac` 0..2 thirds of an octave on top of the octave range; `pat_custom` plays the custom pattern (needs `synthg_pat_on`); `pat_len` 2..16; step gate 0..8 eighths, accent bool, pan 0 Left / 1 Center / 2 Right. Defaults equal the engine defaults. Sound Init is an engine method (`Stage4Engine::sound_init`), not a host parameter.
- **Pedals / Swell / SUSTPED / PSTICK (`ped_*`):** globals `ped_{swell_func,swell_gain,swell,ctrl_func,ctrl_gain,right,rotor_func,rotor,foot_func,foot}` (swell_func 0 Swell, 1 Volume, 2 Wheel Morph; ctrl_func 0 Control, 1 Ctrl+Swell; right 0 Sustain, 1 Sus+Rotor Hold, 2 Sus+Rotor Toggle; rotor_func 0 Toggle, 1 Hold; foot_func 0 Off, 1-2 Layer Scene Hold/Toggle, 3-4 Rotor Hold/Toggle; `ped_swell` 1.0 = pedal open, B3 never goes below -20 dB, other models reach silence; `ped_rotor` and `ped_foot` are momentary switches) and per layer (`_1..7` = Organ A, Organ B, Piano A, Piano B, Synth A, B, C) `ped_{sustped,pstick,range}_N` (SUSTPED defaults On for the two Pianos only; PSTICK range 1-12 semitones, default 2, honoured by the Synth layers only (`ped_range_5..7`); Organ and Piano bend a fixed 2 semitones). MIDI `CC4` is not overwritten by an unchanged `ped_swell`. Pitch bend input is now supported in OmniSplit and Multitimbral modes.
- **Aux KB (`auxkb_*`):** `auxkb_{enable,channel,curve,organ,piano,synth}`. Channel 1-16 (0 = Off, default 16), curve 0 Soft, 1 Medium, 2 Hard, layers as bitmasks (bit 0 = Layer A). A layer in Aux KB mode ignores the Global channel and answers only the Aux KB channel; Aux sustain and bend reach it even with SUSTPED/PSTICK off. Extern layer mode has an engine API and output queue (`Stage4Engine::pop_midi_out`) but no host parameters and the plugin MIDI output port is not enabled yet.
- **Triple Pedal (`pedx_*`):** `pedx_sustain_type` (0 SP-1, 1 SP-2 default, 2 TP-1, 3 Normally Open, 4 Normally Closed), `pedx_tp2` (TP-2 attached), `pedx_panel` (Panel Assigned target: 0 Layer Scene II, 1 Mst Clk Tap, 2 Synth Vibrato, 3 Pump), `pedx_{tp1,tp2}_{left,mid}` (function 0 Una Corda, 1 Sostenuto, 2 Panel Assigned, 3-4 Layer Scene Hold/Toggle, 5 Pump, 6 Mst Clk Tap, 7 Synth Vibrato, 8-9 Program Up/Down, 10-11 Rotor Hold/Toggle, 12 Control; defaults Left 0, Middle 1) and momentary positions `pedx_{tp1,tp2}_{left,mid}_pos` (0..1). Defaults leave sounds bit-identical. MIDI `CC67` / `CC66` are the TP-2 Left / Middle pedals. Program Up/Down and the Pump value are engine hooks (`take_program_step`, `pump_value`), not host actions yet.
- **Program (Transpose, Program Levels):** `prog_transpose` (-6..6 semitones, all layers, manual p.40) and `prog_level_db` (-12..12 dB offset on all layer levels, manual p.46; range `ASSUMED`). Defaults 0, bit-identical rendering. Panic, Layer Init, Copy/Paste/Swap, Undo and Live slots are engine APIs (`nord_core::engine`), not host parameters.
- **System / Sound / Output menu (`sys_*`):** `sys_{transpose,fine_tune,piano_noise_db,piano_res_db,tonewheel,click_high,rotary_balance,rotor_speed,rotor_acc,horn_speed,horn_acc,mono}` (manual p.57-59). `sys_transpose` -6..6 semitones added to `prog_transpose`; `sys_fine_tune` -50..50 cents for Organ, Piano and Synth; `sys_piano_noise_db` and `sys_piano_res_db` +/-6 dB; `sys_tonewheel` 0 Clean, 1 Vintage 1, 2 Vintage 2; `sys_click_high` Normal/High; `sys_rotary_balance` 0-4 = 70/30 ... 30/70; speed and acceleration 0 Low, 1 Normal, 2 High; `sys_mono` mixes both channels to `(L+R)/2`. Defaults equal the previous engine behaviour, so an untouched host renders bit-identically; the non-default gains and speed factors are `ASSUMED` (not measured). Program Levels stays `prog_level_db`. The MIDI, Extern, Output, trigger point and SW B settings are in the three entries below (`midix_*`, `outx_*`, `menx_*`).
- **MIDI and Extern menus (`midix_*`, additive, `midi_menu_params.rs`):** `midix_{sect_organ,sect_piano,sect_synth}` (Sect On, default Off), `midix_nrpn` (Type: On = CC & NRPN, default), `midix_{ctrl,dev,pc,pc_ext}` (0 Off, 1 Send, 2 Receive, 3 Send & Receive, default 3), `midix_transpose_at` (0 In, 1 Out), `midix_echo` (Echo Global To Extern Ch, default On) and per Extern layer A/B/C (`_1`/`_2`/`_3`) `midix_vel_{n}` (0 Soft, 1 Mid, 2 Hard) and `midix_{mod,aft,ctl,swl,vol}_{n}` (Device MIDI, default On). Defaults equal the engine, so an untouched host renders bit-identically. Receive halves filter the Global channel; send halves and Local Control act on the engine's keyboard entry `handle_keyboard_midi` (the plugin's on-screen keys use it when Local Control is Off). Local Control is **not** a host parameter (the manual reverts it to On at start). Ctrl defaults to Send & Receive and Echo to On (hardware Off), `ASSUMED`, to keep existing behaviour.
- **Output menu (`outx_*`, additive, `output_params.rs`):** `outx_mode` (0 Global, 1 Per Program), `outx_busout_{1..4}` (Main, Sub I-III output: 0 `1-2`, 1 `3-4`, 2 `1`, 3 `2`, 4 `3`, 5 `4`, 6 `12+34`; defaults 0, 1, 4, 5), per source `outx_global_{1..7}` (same list plus 7 = arrow / Auto) and `outx_bus_{1..7}` (0 Main .. 3 Sub III, 4 = arrow / Auto) in the order Organ, Rotary, Piano A, Piano B, Synth A, B, C, and `outx_trim_{1..4}` (+/-12 dB on outputs 1-4). The plugin has a stereo main port (outputs 1-2) and a stereo aux output port `Aux 3-4` (outputs 3-4), see the audio ports in [daw-plugin.md](specs/daw-plugin.md); a source routed to 3-4 leaves the main port, as on the hardware. A host that offers only one bus gets the stereo-only layout (main 1-2 only). Defaults send everything to 1-2, bit-identical to before.
- **Organ trigger point and Foot Switch SW B (`menx_*`, additive, `menu_extra_params.rs`):** `menx_trigger` (0 High default, 1 Low: Organ note-ons wait 8 ms, `ASSUMED`), `menx_fsb_func` (the ten Pedal-menu choices, 0 User Assigned .. 9 Rotor Toggle) and the momentary `menx_fsb`. PSTICK (`ped_pstick_{n}`) now also bends Organ and Piano layers; it defaults Off, so renders are unchanged.
- **Layer FX extras (layers 1-7 = Organ A, Organ B, Piano A, Piano B, Synth A, B, C):** `fxx_{mod1_amount,mod2_amount,eq_mid_freq,filter_res,amp_variation,delay_analog,delay_fx_variation}_{1..7}`.
- **Appended Mod types:** `fxsel_mod1_type_*` now also offers `Auto Wah` and `Pump` (steps 4 to 6) and `fxsel_mod2_type_*` offers `Spin` and `Ensemble` (steps 4 to 6), appended after the earlier values so saved host automation keeps its meaning; ids are unchanged. `fxsel_mod1_rate_*` is SENS for Auto Wah; `fxy_mod1_var_*` / `fxy_mod2_var_*` select the variation models; `fxy_mod1_ped_*` + `fxy_mod1_ped_pos_*` trigger a Pump duck on a rising edge through 0.5. Mod 1 Master Clock sync is engine-only for now.
- **Layer FX variations, sharing and clock division (same layer order):** `fxy_{mod1_var,mod2_var,mod1_ped,mod1_ped_pos,delay_beats,glob_delay,glob_comp,glob_rev,group}_{1..7}`. `delay_beats` 0 keeps `fxsel_delay_div`, 1-17 select the hardware Master Clock fractions (used with `fxsel_delay_sync`). `glob_*` flag the layer whose Delay / Compressor / Reverb applies to all layers (lowest flagged layer wins; Reverb GLOBAL moves one shared reverb after the rotary and the level faders). `group` on layers 3-4 (Piano) or 5-7 (Synth) shares that layer's whole chain with its section. `TO ROTARY` is `fxsel_cabinet_*` = To Rotary (Piano / Synth layers only).
- **Mod 1 clock division and Reverb VARIATION (`fxz_*`, additive):** `fxz_{mod1_beats,rev_var}_{1..7}`, same layer order as `fx_*`. `mod1_beats` 0 keeps the free `fxsel_mod1_rate`, 1-17 are the same Master Clock fractions as `fxy_delay_beats` (applies to A-Pan, Trem, Wah and Pump; Ring Mod and Auto Wah ignore it). `rev_var` is the Spring (longer tail) and Booth / Room (longer pre-delay) VARIATION; Stage, Hall and Cathedral ignore it (they use `fxsel_rev_color` = Chorale). Defaults (free rate, off) equal `LayerFxChain::new`.
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

---

## 7. What Was Verified (B13, 2026-11-14)

No DAW was available. Everything below ran headlessly on one Windows 11 development machine (x86-64, release builds, `rustc 1.91` for the workspace, `1.95` for `clap-validator`). The numbers come from that machine only; timing varies by 10 to 20 percent between runs, so ranges are given. **No Reaper, Bitwig or other DAW check was run**; the host rows in section 3 stay "not host-tested".

### 7.1 Audio performance (`ns4 bench`)

Workload (`crates/nord-core/src/bench.rs`): 7 layers sounding (Scene II), 88 keys held (MIDI 21..=108, velocity 100), all 7 FX racks fully on (Mod 1 A-Pan, Mod 2 Chorus, Twin amp with drive, 3-band EQ, compressor, analog ping-pong delay with LP filter and chorus feedback FX, Hall reverb), master compressor on, sources spread over all four outputs (`process_block_multi`), 128-frame blocks at 48 kHz (deadline 2.667 ms), 200 warm-up blocks, 2000 timed blocks.

```bash
cargo build --release -p nord-core
./target/release/ns4 bench                     # worst case, 4 outputs (--stereo: process_block)
./target/release/ns4 bench --scenario typical  # or idle; default worst
./target/release/ns4 bench --blocks 1000 --breakdown
```

Measured after the TD-14 optimisations (release, one Windows 11 machine with other jobs running, so several alternating runs; "before" is the source at the start of the perf task):

| Measure | Before | After | PRD budget ([PRD](PRD.md) section 5) |
|---|---|---|---|
| Worst case, mean CPU per 128-frame block | 3.9 to 4.4 ms (p50 3.6 to 3.7, p99 8 to 16) | 2.2 to 2.5 ms (p50 2.1, p99 4 to 10) | 0.8 ms: **not met** |
| Realtime factor, worst case | 0.6x to 0.7x | 1.1x to 1.2x | 1.0x minimum: met |
| Typical program (`--scenario typical`: Scene I, 3 layers, 10 keys, default FX) | 1.05 to 1.1 ms | 0.67 to 0.70 ms (about 4x realtime) | 0.8 ms: **met** |
| 7 layers and racks running, no key held (`--scenario idle`) | 1.5 to 1.9 ms | 0.8 to 0.9 ms | at the budget |
| One layer, 88 keys held, FX off (includes the idle floor of the other six layers) | Organ 0.9 to 1.2, Piano 0.63 to 0.77, Synth 1.0 to 1.4 ms | Organ 0.53 to 0.59, Piano 0.28, Synth 0.39 to 0.43 ms | |
| FX racks (7 chains, all on), worst case with and without | about 1.0 ms | about 0.65 ms | |
| Plugin `process()` incl. sync of 811 parameters (Scene I, 88 keys) | 1.6 ms per block (`PLUGIN_PROCESS_MS`, `host_lifecycle` test) | not re-measured | |

Verdict: the worst case (every key of an 88-key keybed held on all 7 layers with every FX on) now runs in real time with 10 to 20 percent headroom but still misses the 0.8 ms figure by about 2.8x; a typical program is within the budget; real playing holds 10 to 20 notes. p99 is not a render figure on this host: with a quiet machine p99 was 4 to 6 ms (2 to 2.5 times p50), with other jobs running 10 to 16 ms.

Where the time went, from a sampling profile of `ns4 bench` (a helper that suspends the bench thread every 100 us, reads the instruction pointer, resolves it through the PDB line table and, for samples inside the C runtime, through the nearest return address on the stack and the DLL exports; `samply` needs admin rights on Windows): per-voice repetition of pure functions (firmware filter coefficients with an f64 `tan` and `atan`, the strip cutoff law), the drawbar scan (division and foldback loops for 792 terms per layer), 91 sines per sample for both organ layers although the second layer's phases equal the first's, `fmodf` for every phase wrap, and the 400-tap cabinet FIR as four dependent float chains. All were removed without changing a bit; the output hash is the gate (below). Fat LTO with one codegen unit was measured again and gave no gain. What remains (TD-14): the organ drawbar sum is one 792-term dependent float chain per layer (about 0.25 ms for two layers, the summation order is part of the output), the FX racks (about 0.65 ms), the synth voice recursion and supersaw (about 0.15 ms per layer) and `sinf` (about 0.15 ms).

Output hashes (FNV-1a of every output sample bit, Windows/UCRT, 2000 blocks, 4 outputs), identical for the source before the perf task and after it: worst `0xc7ee671ec250785e`, typical `0x64043873f4f68dd0`, idle `0xf65ba453b6c42325` (all zero samples, peak 0). They are a pure function of the engine state (`bench::tests::the_hash_is_deterministic_and_the_output_finite_and_audible`), so an optimisation is checked by comparing the hash before and after; they depend on the C library's `sin`, `exp`, `tan` and `powf`, so another platform prints other values. A quick budget guard runs in `cargo test` (`bench::tests::a_block_stays_within_a_generous_budget`: 20 ms in release, 500 ms in debug); it catches order-of-magnitude regressions only.

### 7.2 Zero allocations in the audio path

Counting `#[global_allocator]` tests (alloc, free and realloc all count; armed only around the audio call; setup and warm-up unarmed):

| Test | What it runs | Result |
|---|---|---|
| `nord-core/tests/no_alloc_audio_path.rs::worst_case_polyphony_with_fx_and_multi_out_never_touches_the_heap` | The 7.1 workload for 1500 blocks with key retrigger, sustain pedal, tempo changes, `process_block_multi` and `process_block` alternating | 0 allocator calls |
| `nord-core/tests/no_alloc_audio_path.rs::process_block_with_telemetry_publishing_never_touches_the_heap` (existing) | 400 blocks with MIDI and telemetry | 0 |
| `nord-plugin/tests/host_lifecycle.rs::process_never_touches_the_heap_over_a_long_worst_case_run` | `NordStage4Plugin::process` for 1500 blocks: sync of all 811 parameters, event routing, on-screen keybed queue, 88 keys held, Scene I | 0 |

```bash
cargo test --release -p nord-core --test no_alloc_audio_path
cargo test --release -p nord-plugin --test host_lifecycle
```

Locks are not counted by these tests. The plugin `process()` path takes no `Mutex` (the only one in `crates/nord-plugin/src` is in the unit-test mock of `editor/write.rs`). The plugin test cannot reach the 7-layer scene because `layer_scene_ii` is a host parameter that an external crate cannot set; the 7-layer case is the engine test.

### 7.3 GUI frame time (`frame_cost_tests.rs`, `full_panel_frame_time_probe`, `panorama_frame_time_probe`)

CPU only: `egui::Context::run` (layout, labels, group frames, legibility frames) plus `Context::tessellate` at the given pixels per point, for the full 3120 x 840 point panel. GPU rasterisation is not measured (no GPU in headless tests), so this is the CPU part of the B13 "2x-scale frame under 4 ms" budget.

```bash
cargo test --release -p nord-gui --lib frame_cost -- --nocapture --test-threads=1
cargo test --release -p nord-gui --lib frame_time_probe -- --nocapture --test-threads=1
```

| View | `run` | `tessellate` | Total | Vertices | Before PR #306 (same test, same machine) |
|---|---|---|---|---|---|
| Full panel, 1x | 0.7 to 1.1 ms | 1.7 to 2.3 ms | 2.5 to 3.2 ms | 39,446 | 2.53 ms, 36,914 vertices |
| Full panel, 2x | 0.7 to 1.0 ms | 1.9 to 2.2 ms | 2.6 to 3.2 ms | 47,760 | 2.88 ms, 47,712 vertices |
| Panorama, 1x | 0.7 to 2.0 ms | 1.4 to 2.3 ms | 2.1 to 4.2 ms | 36,078 | 2.13 ms, 33,522 vertices |
| Panorama, 2x | 0.8 to 1.1 ms | 1.6 to 1.9 ms | 2.3 to 3.0 ms | 42,422 | 2.68 ms, 41,846 vertices |

"Before PR #306" is the first parent of merge `d735d510` (one run each). The frames and labels of #306 added about 215 clipped shapes (2,051 to 2,266) and about 2,500 vertices at 1x (7 percent) and no measurable frame time: the differences are inside the run-to-run noise. No regression, nothing to fix.

The older probes (`run` only, no tessellation) give 0.64 to 0.70 ms for the full panel and 0.57 to 0.71 ms for the panorama. The CPU side of a 2x-scale frame is under 4 ms in 11 of the 12 measured passes (3 runs of the four views; one panorama 1x pass reached 4.2 ms during a noisy run). The margin is thin and the GPU cost and the host's own repaint are not included, so the "under 4 ms" criterion is **not fully verified**. The panel repaints on a 33 ms timer plus on interaction (`panel/mod.rs`), the standalone app asks for 16 ms (`main.rs`); a "repaint only on change" audit was not done.

### 7.4 CLAP: `clap-validator` 0.4.1

Built from source outside the repository with the second installed toolchain (`cargo +1.95.0 install --git https://github.com/free-audio/clap-validator --locked --root <dir>`; the tool needs `rustc 1.95`; it is not on crates.io or PyPI). Nothing from it is committed. The validator needs a `.clap` file, which is the release DLL renamed:

```bash
cargo build --release -p nord-plugin
cp target/release/nord_plugin.dll NordStage4.clap        # Linux: libnord_plugin.so
clap-validator validate NordStage4.clap
```

Result after the fix in this change (plugin id `org.opennord.nord-stage-4`): **44 tests run, 30 passed, 4 failed, 1 warning, 9 skipped.**

| Group | Result |
|---|---|
| Factory, descriptor, features, create/destroy | passed |
| Audio: basic in/out of place, varying sample rates 1 kHz to 768 kHz, varying and random block sizes 1 to 16k, reset and re-activate, notes, inconsistent notes, sleep/process status, constant mask | passed |
| Param: set events, cookies, wrong namespace, fuzz (basic, bounds, sample accurate, modulation), conversions, default values | passed |
| Transport: null, fuzz, fuzz sample accurate | passed |
| State: `state-invalid-empty` | passed |
| `process-note-wildcard` | **crashed before the fix, passes now** (below) |
| `scan-time` | warning: 117 to 130 ms to scan, validator limit 100 ms |
| `state-invalid-random` | **crashed (abort)**: upstream, below |
| `state-reproducibility-basic`, `-binary`, `-buffered` | **failed**: upstream, below |
| Skipped | 64-bit audio (not supported), denormals (no audio input), `audio-ports-activation`, `configurable-audio-ports`, preset discovery (not implemented), `scan-rtld-now` (Unix only) |

What the failures are:

- **`process-note-wildcard` (our bug, fixed).** CLAP wildcard note events (key `-1`) arrive as note 255; `Stage4Engine::handle_midi` indexed its 128-entry hold table with it and panicked on the audio thread (a host crash). Notes, controllers and program numbers above 127 are now dropped in `crates/nord-core/src/engine/midi_dispatch.rs` and, for the multitimbral path that bypasses the engine router, in `crates/nord-plugin/src/midi.rs`. Regression tests: `out_of_range_data_bytes_are_dropped_without_panicking`, `wildcard_keys_are_dropped_in_every_routing_mode`, `hostile_note_events_do_not_panic_and_output_stays_finite`.
- **`state-reproducibility-*` (upstream nih-plug).** The validator reports that 619 parameter values changed on `state.load` without a `params.rescan(VALUES)` request (for example Delay Feedback 0.35 to 0.182). The values do change to the saved ones, which is the correct restore; the nih-plug CLAP wrapper (rev `de421011`) does not send the rescan notification. The validator stops at that check, so the rest of the test (save, load, save again, compare bytes) did not run. Not fixable in this crate. Real hosts re-read parameters after loading state; not checked in a host.
- **`state-invalid-random` (upstream nih-plug).** `ext_state_load` in the wrapper reads an 8-byte length from the stream and calls `Vec::with_capacity(length)`; random bytes give a length near 1e18 and the allocation aborts the process. Only a corrupt or hostile state triggers it. Not fixable here without patching nih-plug.
- **`scan-time`.** 117 to 130 ms against a 100 ms advisory limit; the plugin builds 811 parameters at scan time.

### 7.5 Headless host simulation (`crates/nord-plugin/tests/host_lifecycle.rs`)

Drives the `Plugin` trait the way nih-plug's wrappers do (initialize, process per block with sample-timed events, reset, re-initialize). Checks: activation at 44.1, 48, 88.2, 96 and 192 kHz with blocks of 1 to 1024 frames and finite output; sample-accurate note-on offset inside a block; reset and re-activation at another rate and in offline mode; hostile note events (keys 0, 127, 128, 200, 255, channels up to 255) never panic and the output stays finite; the parameter map has unique ids, defaults in range and a text round trip for all 811 parameters; the persisted editor-state field survives a save and load; zero allocations (7.2). Parameter automation and the wrapper's state save and load are **not** exercised here (nih-plug does not let an external crate set parameter values or call its state code); `clap-validator` is the evidence for those.

### 7.6 Not verified

| Item | Why |
|---|---|
| Any DAW (Reaper, Bitwig, Live, Cubase, Studio One, FL Studio, Logic) | No host available; section 3 stays "not host-tested" |
| VST3 | No VST3 SDK validator is installable with cargo or pip (it needs the SDK and CMake plus a C++ toolchain). The release DLL contains the `GetPluginFactory` string, which is weak evidence only; the VST3 bundle (`cargo xtask bundle nord-plugin --release`) was not built or validated |
| Standalone wrapper (`nord-plugin-standalone`) | Needs an audio device; not run |
| AUv2 | Needs macOS and `clap-wrapper` |
| Automation lanes, project save/load, editor open/close, HiDPI, window resize in a host | Need a host |
| Multitimbral routing across 7 tracks, transport sync with a moving playhead | Need a host (the tempo is read from the transport in `process`; unit test only) |
| 2x GPU frame time, repaint-only-on-change audit | Need a GPU run |
| Linux and macOS builds of the plugin | Not built in this change |

### 7.7 Manual smoke-test checklist (Reaper, Bitwig)

Run once per host and record host name, version, OS, date and the result of every line in `docs/research/` (the editor checklist in [plugin-editor.md](plugin-editor.md) covers the panel itself):

1. Build the bundles: `cargo xtask bundle nord-plugin --release`; copy `target/bundled/nord-plugin.clap` and `.vst3` to the host's plugin folders; rescan. Record the scan time and whether the plugin appears in both formats.
2. Insert on an instrument track; play MIDI notes from a keyboard or the piano roll: sound on all three engines (Scene I default), no clicks, no stuck notes after stop.
3. Set the buffer to 64, 128, 512 and 2048 frames and the sample rate to 44.1, 48 and 96 kHz: no dropouts with a 10-note chord; watch the host CPU meter with 88 keys held (the all-layers worst case is expected to overload, see 7.1).
4. Open and close the editor 10 times, resize it, move it between monitors of different DPI: no crash, the panel stays sharp.
5. Automate `master_level` and an Organ drawbar with a write lane; play back; the panel knob follows. Move a parameter from the host's generic UI: the panel follows.
6. Save the project, close the host, reopen: all parameter values, the editor size and the layer scene return. Duplicate the track: the copy keeps its own state.
7. Set the MIDI routing mode to `MultitimbralPerLayer` and send notes on channels 1 to 7: each reaches its own layer (Organ A, B, Piano A, B, Synth A, B, C).
8. Change the project tempo and play a synced delay or arpeggio: it follows the host tempo.
9. Bounce 30 seconds offline with notes and the sustain pedal: the file is finite and matches a realtime bounce within level noise.
10. Remove the plugin during playback and with the editor open: no crash.
11. Run `clap-validator validate` on the installed `.clap` and the VST3 validator on the `.vst3` if available; attach the output.
12. Route a layer to outputs 3-4 (`outx_global_*` = `3-4`) and check that the aux port (`Aux 3-4`) carries it and the main port loses it; confirm the host offers the main + aux layout and falls back to stereo only where it has one bus.

### 7.8 Aux output port 3-4 (`ns4-ux21`, 2026-11-14)

The plugin has two audio layouts (main 1-2 + aux 3-4, then stereo only; see [daw-plugin.md](specs/daw-plugin.md) Audio ports). Headless evidence, same machine as 7.1:

| Check | Result |
|---|---|
| `cargo test -p nord-plugin --test aux_outputs` | 3 passed: main pair bit-identical with and without the aux buffer (default routing and everything routed to 3-4), aux silent by default and filled when routed, 0 allocator calls in `process()` with the aux port (1500 blocks in release, 150 in debug) |
| `cargo test -p nord-plugin` unit tests `audio_out::tests` | layout order, no vendor name in port names, aux bound by the shortest slice, mono fold-down |
| `cargo test -p nord-plugin param_snapshot` | passed, `param-ids.snapshot` unchanged |
| `cargo xtask bundle nord-plugin --release` (from a copy at `C:\tmp`, because xtask refuses a nested worktree) | CLAP and VST3 bundles built |
| `clap-validator` 0.4.1 on that CLAP bundle | **44 run, 31 passed, 4 failed, 0 warnings, 9 skipped.** The 4 failures are the same upstream nih-plug ones as 7.4 (`state-invalid-random` abort, `state-reproducibility-basic` / `-binary` / `-buffered`). `layout-audio-ports-config` (tries every layout) and `process-audio-basic-*` passed with the new port. `scan-time` passed this run (92 ms; it was 117 to 130 ms before, so treat it as noise) |

Not verified: any DAW, which layout a host picks, that the aux port shows up as a second track or channel pair (Reaper, Bitwig, Live), and the VST3 bundle beyond building it.

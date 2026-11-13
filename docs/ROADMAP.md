# Roadmap & Master Implementation Plan

Don't start an unapproved phase without user confirmation. Tick checkboxes (`- [x]`) in the PR or commit that delivers them alongside unit tests.

---

## Executive Summary & Architecture Plan

```text
Phase 0: Reverse Engineering, Hardware Teardown & Full Specification Suite  [DONE]
   │
   ├──► Phase 1: Interoperability Core (`CBIN` / `.ns4b` / `.ns4p` / Firmware) [DONE]
   │
   ├──► Phase 2: Organ Engine DSP (`DSPO` — 91 Tonewheels, B3/Vox/Farf/Pipe, Rotary 122)
   │
   ├──► Phase 3: Synth Engine DSP (`DSPS` — 3x Wave 2 PolyBLEP VA, Wavetable, FM, Sample, ZDF Ladder)
   │
   ├──► Phase 4: Piano Engine DSP (`DSPF` — 2x 120-Voice SFZ/WAV, Modal String Resonance, Pedal Noise)
   │
   ├──► Phase 5: 7-Layer Routing, 4-Zone Splits, 3-Source Morph Matrix & 7 Per-Layer FX Racks
   │
   ├──► Phase 6: 1:1 Hardware Front-Panel UI, Standalone App & CLAP/VST3 DAW Plugin (`nih-plug`)
   │      │
   │      └──► Phase 6b: 1:1 Hardware Parity (every control bound) & 2.5D Panel Renderer
   │
   └──► Phase 7 (Track B): Full Firmware LZ Decompression & `CNSP` Sample Codec Reverse Engineering
```

---

## Phase 0: Reverse Engineering, Hardware Teardown & Specifications (`ns4-3387`)

- [x] Dissect `Nord Stage 4 Factory Restore RevK v1.66.ns4b` archive (`meta.xml`, `product_id=46`, build `2934`, 1,171 files census) ([research](research/ns4-backup-and-cbin-teardown.md))
- [x] Reverse-engineer the 48-byte `CBIN` v1 header and bitstream embedding across `.ns4p` (868 B), `.ns4y` (541 B), `.ns4o` (183 B), `.ns4n` (195 B), `.ns4l` (868 B), `.ns4t` (124 B) ([spec](specs/cbin-and-presets.md))
- [x] Extract and map the OS v1.66 firmware payload (`RCDATA 101`, 4,037,080 B) into `ARM0` host MCU and `DSPO` / `DSPS` / `DSPF` SHARC DSP partitions (correction: the `DSP*` names are region labels; SHARC VISA code is in `bank0_CODE@0x440000..0x52FFFF`, see [dsp-image-hunt](research/dsp-image-hunt.md)); [research](research/ns4-firmware-teardown.md)
- [x] Document Stage 4 mainboard (AM1705 host + 4 ADI DSPs, `.reference/mainboard/`), Stage 3-family reference boards (`11870-Lg.jpg`, `12916-Lg.jpg`), and `CS4392` + `LM833`/`LF353` DAC output stage ([research](research/hardware-and-dac-analysis.md))
- [x] Document 1:1 front-panel visual design from `foto.jpg`, `NS4-Panel-organ.max-2000x2000.format-png.png`, and `Screenshot20at2015.jpg` ([spec](specs/visual-style-and-panel.md))
- [x] Author complete subsystem specifications in `docs/specs/*.md` (`cbin-and-presets`, `organ-engine`, `synth-engine`, `piano-engine`, `effects-and-routing`, `morph-and-zones`, `midi-and-control`, `visual-style-and-panel`, `verification`)
- [x] Establish repository scaffolding, `AGENTS.md`, `CLAUDE.md`, `.agents/rules/`, `.agents/skills/`, `.beans/`, and GitHub Actions CI/CD (`ci.yml`, `release.yml`)

---

## Phase 1: Format Interoperability & Preset Bitstream Mapping (`ns4-cbin`)

**Goal:** Load, inspect, diff, and round-trip real Nord Stage 4 `.ns4p` programs, `.ns4y`/`.ns4o`/`.ns4n` presets, and `.ns4b` backups into `Stage4Engine` parameter states.

- [x] `crates/nord-core/src/cbin.rs`: 48-byte `CBIN` header parser, lossless serializer, schema validator (`0x0139`, `0x00D0`, `0x00CD`, `0x00CB`, `0x006C`), sub-preset slice extractor (`0x5D` Organ, `0xE4` Piano, `0x177` Synth), and `.ns4b` `meta.xml` parser
- [x] `crates/nord-core/src/firmware.rs`: OS update container header and `SRAM` / `CODE` / `ARM0` / `DSPO` / `DSPS` / `DSPF` partition table parser
- [x] `tools/ns4_inspect.py`: Python CLI and library for inspecting `.ns4b` archives and `.ns4*` presets with JSON output
- [x] `tools/check_tests.py`: Per-file unit-test enforcement ratchet across Rust and Python (`tools/test-baseline.txt` at 0 uncovered files)
- [x] Map bit offsets inside the 135-byte `.ns4o` Organ payload (Model `B3`/`Vox`/`Farf`/`Pipe1`/`Pipe2`, Layer A/B 9-drawbar nibbles, Percussion switches, Vibrato `V1..C3`, Rotary speed/drive)
- [x] Map bit offsets inside the 147-byte `.ns4n` Piano payload (Category, Model hash ID, Timbre `Soft`/`Mid`/`Bright`/`Dyno1`/`Dyno2`, `KB Touch`, `String Res`, `Ped Noise`, `Unison`, `Dyn Comp`)
- [x] Map bit offsets inside the 493-byte `.ns4y` Synth payload (`SAMPLES`/`ANALOG`/`WAVETABLE`/`FM` mode, Waveform/Table ID, Detune, Filter Type/Cutoff/Res/Drive/KB-Track, ADR Envelopes, LFO, Arpeggiator)
- [x] Synth `0x32`/`0x2F` full parameter map (LFO shape/target, Mono/Legato/priority, Glide, Unison, Octave, pitch, osc/filter/amp ADR + velocity, vibrato, osc type/category/wave) decoded, packed and wired into the DSP; cross-checked with `tools/ns4_crosscheck.py` (field order/widths firmware-VERIFIED via `tools/ns4_fw_order_check.py`, absolute offsets EMPIRICAL, scales tracked in OQ7)
- [x] Map global 45-byte `.ns4p` header (`0x30..0x5C`: Split points, Crossfade widths, Scene I/II masks, Master Clock BPM, Morph slots)
- [x] `ns4 convert <file.ns4p> --to-json` / `--from-json` human-readable patch converter and `.ns4p` → `Stage4Engine` preset loader
- [x] Hardware-exact OS v1.66 `Buffer_*` bitfields (`0x2C`, `0x11`/`0x03`, `0x0C`/`0x0B`, `0x1E`/`0x32`/`0x2F`), `0x36` per-layer FX chains, and `4W+3` Morph encoding (`zero = max_val`) wired into `Stage4Engine::apply_program`, `Stage4PanelState::apply_program`, and `tools/ns4_inspect.py` (`ns4-bnct`)

---

## Phase 2: Organ Engine DSP (`DSPO` High-Level Emulation) (`ns4-orgn`)

**Goal:** 100% mathematical physical modeling of the 2-layer Nord Stage 4 Organ section and Model 122 Vintage Rotary Speaker.

- [x] `crates/nord-core/src/organ/`: B3 91-tonewheel frequency & phase generator (firmware oscillator-bank parity), 9-drawbar (`16'` to `1'`) harmonic bus mixer with octave foldback, single-triggered 2nd/3rd harmonic percussion envelope (`Soft`/`Fast`), key-click transient, and Model 122 Rotary Speaker (800 Hz crossover + Slow/Fast ballistics + stereo Doppler)
- [x] Exact 196-tooth Hammond drive/driven integer gear ratio table and per-tonewheel pickup calibration curve (`B3` vs `B3 Bass` mode)
- [x] Tonewheel transformer crosstalk matrix and palladium contact bounce key-click bandpass filter
- [x] 18-section LC phase-shift delay-line **Scanner Vibrato / Chorus** (`V1`, `V2`, `V3`, `C1`, `C2`, `C3`)
- [x] Authentic **Vox Continental** (`16'`, `8'`, `4'`, `II`, `III`, `IV`, Sine + Reed wave) and **Farfisa Compact** (`Bass16`..`Str4` multi-tab bandpass + brilliance boost) models
- [x] **Pipe 1** (liturgical flute/principal with wind chiff) and **Pipe 2** (tracker organ swell/great registration) voicing models
- [x] Rotary Speaker `Close Mic` / `Angle` / `Stop Mode` acoustic cabinet reflection modeling

---

## Phase 3: Synth Engine DSP (`DSPS` Nord Wave 2 Derived) (`ns4-synt`)

**Goal:** Full 3-layer (`Synth A`, `Synth B`, `Synth C`) 46-voice polyphonic Nord Wave 2 synthesis engine.

- [x] `crates/nord-core/src/synth.rs`: Anti-aliased `PolyBLEP` Virtual Analog oscillator (`Saw`, `Pulse/PWM`, `Triangle`, `Sine`), `Wavetable` & `FM` modes, Zero-Delay Feedback (`ZDF`) 12/24 dB Transistor Ladder (`LP M` / `Ladder24`) & SVF (`LP12`, `BP`, `HP`) filters, and ADR envelope
- [x] Multi-oscillator `ANALOG` configurations: `Super Saw` (7 detuned PolyBLEP saws matching `DETUNE: 3.4` on the Synth OLED), `Sync`, `Bell`, `Sub`, and `Noise`
- [x] 64 band-limited mipmapped single-cycle Wavetables (`WAVETABLE` mode) with spectral formant shaping
- [x] 2/3-Operator harmonic & inharmonic `FM` router with feedback and envelope modulation
- [x] `SAMPLES` mode SFZ/WAV sample player with loop crossfade and `Soft`/`Bright` timbre damping
- [x] Polyphonic voice allocator across `Synth A/B/C` (16 pre-allocated voices per layer with dynamic envelope stealing)
- [x] Tempo-synced `LFO` (Triangle, Saw, Square, Sample & Hold -> `FILTER`, `OSC PITCH`, `OSC CTRL`) and `ARPEGGIATOR / GATE` (`Up`, `Down`, `UpDown`, `Random`, `Poly`, 1-4 octaves)

---

## Phase 4: Piano Engine DSP (`DSPF` Sample + Modal Resonance) (`ns4-pno4`)

**Goal:** 2-layer (`Piano A`, `Piano B`) 120-voice polyphonic acoustic/electric piano engine with sympathetic string resonance and pedal mechanics.

- [x] `crates/nord-core/src/piano.rs`: Multisample voice state, `KB TOUCH` velocity curves (`Light`, `Med`, `Heavy`), `TIMBRE` spectral presets (`Soft`, `Mid`, `Bright`, `Dyno1`, `Dyno2`), `Sustain` damper logic, and 12-band Modal Sympathetic String Resonance bank
- [x] Pre-decoded **NPNO / Multisample playback engine** in `crates/nord-core/src/piano.rs` (`NpnoSound`, `LoadedPianoStroke`) with root-key zone mapping, micro-cent detuning, linear fractional interpolation, and zero-allocation real-time safety
- [x] Streaming / RAM-cached open **SFZ / WAV multisample loader** supporting multi-velocity layers and key-off release samples (`SOFT REL`)
- [x] Pitch-specific **Undamped Note Sympathetic Resonance**: holding silent keys while striking lower notes excites the exact overtone resonators of the held notes
- [x] Exponential felt **Damper Release Envelopes** (80ms standard / 180ms `SOFT REL`) eliminating key-off and sustain pedal lift audio pops
- [x] Velocity-sensitive **Dynamic Pedal Noise** (`PED NOISE`) for Damper (`CC64`), Sostenuto (`CC66`), and Una Corda (`CC67`)
- [x] Piano `UNISON` (`1`, `2`, `3` subtle hammer-detune width) and `DYN COMP` (`1`, `2`, `3` dynamic range compression)
- [x] Polyphony steal priority algorithm (recycling decaying released voices and quietest sustaining voices before active sounding notes)
- [x] Hardware-parity sampler interpolation: exact SMP 6-tap x 256-phase Q31 kernel (`piano/q31_kernel.rs`, Hamming-windowed sinc, bit-exact closed form) behind `SampleInterp`, plus the SMP unpacker/predictor model (`piano/smp_unpack.rs`)
- [x] SMP compressed-voice replay on the oracle: record layout of the `0x1C07D8` loop decoded, real `.npno` pages (mono and stereo, widths 5..12, orders 1..3) played through the SMP image and equal to the Rust model bit for bit (`piano/smp_voice.rs`, `piano/smp_page.rs`, `docs/research/smp-voice.md`); `SmpKernel::interpolate_hw` is the integer-exact variant

---

## Phase 5: 7-Layer Routing, Morphing Matrix & Per-Layer FX (`ns4-fxrt`)

**Goal:** Complete 7-layer stage routing, 3-source Morphing, Layer Scene II, and 7 independent per-layer FX racks.

- [x] `crates/nord-core/src/morph.rs`: 3-source Morphing Matrix (`WHEEL`, `A.T.`, `CTRLPED`), `LAYER SCENE II` switcher, and 4-zone Keybed Split router with `Off` / `Small` (6 st) / `Large` (12 st) crossfades
- [x] `crates/nord-core/src/fx.rs`: Per-layer 5-slot FX chain (`MOD 1`, `MOD 2`, `AMP SIM/EQ`, `DELAY`, `REVERB`) + downstream `MasterCompressor` (`COMP`), `MasterClock` (`MST CLK`), and `PCM1791A` + `NJM4580M` output stage
- [x] `crates/nord-core/src/midi.rs`: Complete Nord Stage 4 MIDI CC decoder/encoder
- [x] `crates/nord-core/src/engine.rs`: Integrated 7-layer `Stage4Engine` rendering `Organ A/B`, `Piano A/B`, and `Synth A/B/C`
- [x] Full 3-band semi-parametric EQ in `AMP SIM / EQ` (`BASS` 100 Hz, sweepable `MID` 200 Hz–8 kHz + gain, `TREBLE` 4 kHz) + Cabinet impulse filters (`Small`, `JC`, `Twin`) + `TO ROTARY` routing switch
- [x] `DELAY` enhancements: smooth `ANALOG` tape pitch-slew on tempo changes, `PING PONG` stereo tap pattern, and `HP`/`LP`/`BP` feedback loop filters + `VARIATION`
- [x] `REVERB` enhancements: stereo FDN (Feedback Delay Network) for `Booth`, `Room`, `Stage`, `Spring`, `Hall`, `Cath` with `Bright`/`Dark` damping and `VAR/CHORALE` modulation

---

## Phase 6: 1:1 Front-Panel GUI, Standalone App & CLAP/VST3 Plugin (`ns4-707f`)

**Goal:** Deliver both a **CLAP + VST3 DAW Plugin** and a **Standalone desktop application** (`D11`) with a 1:1 visual recreation of the Nord Stage 4 hardware panel ([spec](specs/visual-style-and-panel.md)).

- [x] `crates/nord-plugin`: `nih-plug` workspace crate exporting **CLAP** (`nih_export_clap!`), **VST3** (`nih_export_vst3!`), and **Standalone** (`nih_export_standalone!`) targets wrapping `Stage4Engine`
- [ ] Custom 1:1 Hardware Panel Renderer (following `docs/specs/visual-style-and-panel.md`). **Reopened 2026-11-12:** `crates/nord-gui/src/panel.rs` draws a functional subset of each bay, but most hardware controls listed below have no state, host parameter or widget (see the [gap list](specs/control-inventory.md#gap-list)), and `nord-plugin` has no editor. Completion moves to [Phase 6b](#phase-6b-11-hardware-parity--25d-panel-ns4-xc5n).
  - [ ] **Bay 1 — Left Master & Rotary (`#8E1B1F`):** Wooden Pitch Stick, Stone Mod Wheel, `MASTER LEVEL` knob, `ROTARY SPEAKER` (`DRIVE` 0–10, `CLOSE MIC`, `STOP MODE`, `ANGLE`, `SLOW`/`FAST` LEDs, `MORPH`)
  - [ ] **Bay 2 — Organ Section (`#323D47`):** Dual physical faders `A`/`B` with **11-segment green LED ladders**, `ORGAN MODEL` triangle selector, `VIB/CHORUS` matrix, `B3 PERCUSSION` switches, **9 Physical Drawbars (`16'`..`1'`) with B3 cap colors and adjacent 8-segment red LED ladders (`1..8`)**, `OCTAVE SHIFT`, `KB ZONE` (4 green LEDs)
  - [ ] **Bay 3 — Piano Section (`#323D47`):** Dual physical faders `A`/`B` with **11-segment green LED ladders**, `ACOUSTICS` (`SOFT REL`, `STRING RES`, `PED NOISE`), `UNISON` (`1/2/3`), `KB TOUCH` (`LIGHT/MED/HEAVY`), `DYN COMP` (`1/2/3`), `TIMBRE` (`BRIGHT/MID/SOFT/DYNO1/DYNO2`), `PIANO SELECT` triangle matrix + `MODEL LIST` stepped encoder
  - [ ] **Bay 4 — Central Program & Morph Section (`#8E1B1F`):** `MORPH ASSIGN` (`WHEEL`, `A.T.`, `CTRLPED` hold-to-assign), `SPLIT` (3 zones + crossfade), `MST CLK`, `TRANSP`, **Central Monochrome Program OLED Display (`A:11 Nord Stage 4`)**, `LIVE MODE`, **`LAYER SCENE II`** button, `PROGRAM 1..8` buttons
  - [ ] **Bay 5 — Synth Section (`#323D47`):** Triple physical faders `A`/`B`/`C` with **11-segment green LED ladders**, **Synth Graphic OLED Display (`OSC WAVEFORM` + real-time waveform plot + 4 soft-tabs)**, `MODE`, `ARPEGGIATOR / GATE`, `VOICE` / `VIBRATO`, `LFO`, `OSCILLATORS`, `FILTER` (`LP12/24/LP M/BP/HP` + `FREQ`/`RES`), `AMP ENVELOPE`, `UNISON`
  - [ ] **Bay 6 — Layer Effects Section (`#323D47`):** Crimson `FX FOCUS` layer selector (`ORGAN A/B`, `PIANO A/B`, `SYNTH A/B/C`, `GROUP`), `MOD 1`, `MOD 2`, `AMP SIM / EQ`, `DELAY`, `COMP`, and `REVERB` boxes
  - [x] **Bottom 88-Key Interactive Keybed:** Triple-sensor velocity click/drag, 3 red Split-Point LEDs with visual crossfade shading, and live MIDI note-on highlights
- [x] Update `.github/workflows/ci.yml` and `.github/workflows/release.yml` to build and package Standalone executables + `.clap` + `.vst3` bundles for Windows, Linux, and macOS (`crates/xtask` `nih_plug_xtask`)

---

## Phase 6b: 1:1 Hardware Parity & 2.5D Panel (`ns4-xc5n`)

**Goal:** Every front-panel control in the Nord Stage 4 manual (ch. 3-9) exists in the emulator with a layout rect, a renderer and a binding (`Bound(param_id)`, `UiOnly(reason)` or `Unsupported(bean)`), drawn as one 2.5D panel (`crates/nord-panel`) shared by the standalone app and the plugin editor ([control inventory](specs/control-inventory.md), [D15](DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering), [D16](DECISIONS.md#d16-phase-6b-scope-original-art-functional-or-stub-controls-ha88-first-additive-host-ids), OQ13 in [OPEN-QUESTIONS](OPEN-QUESTIONS.md)). Host parameter IDs already shipped are a stable contract: additive changes only.

- [ ] **B1** Docs correction: reopen Phase 6 bays, control-inventory spec, visual-spec discrepancies, D15 (`ns4-7ky2`)
- [ ] **B2** Control inventory data: every control from manual ch. 3-9 with page and provenance; counts match manual and photo (`ns4-3n0d`, after B1)
- [ ] **B3** Host parameter-ID stability test: committed snapshot of ID/type/range/default; remove/rename fails, add passes (`ns4-xhbf`)
- [ ] **B4** Split oversized `nord-gui` files (`panel.rs`, `library.rs`, `audio.rs`, `widgets.rs`) and add `crates/nord-panel` skeleton; behaviour unchanged (`ns4-r7vt`)
- [ ] **B5** Host and behaviour layer: `PanelHost`, `PanelAction`, Shift / hold / double-tap / selector cycling / pot snap / Live Mode with unit tests (`ns4-q6p7`, after B4)
- [ ] **B6** Headless golden-image harness: CPU raster via `tiny-skia`, per-pixel tolerance + error budget, goldens at 0.5x; optional local-only photo comparison skipped without `.reference/` (`ns4-7cv6`, after B4)
- [ ] **B7** Asset baker (`xtask bake-panel`) and material/shadow system: Blender-rendered sprites and filmstrips from our own models, committed renders with content hashes, `tiny-skia` for flat vector layers, theme tokens; first proves one knob filmstrip and one fader cap and measures atlas size (D15, `ns4-77s9`, after B1, B4)
- [ ] **B8a-g** Section renderers with a golden image each and every section control bound: master/rotary/wheels (`ns4-0sud`), organ (`ns4-9h25`), piano (`ns4-otuf`), program + both OLEDs (`ns4-z6fi`), synth (`ns4-irgv`), effects (`ns4-hcuh`), keybed (`ns4-jpgx`) (after B2, B5, B6, B7)
- [ ] **B9a-d** Parameter and engine parity, additive IDs only: synth per layer (`ns4-4w9l`), effects (`ns4-e3zs`), organ/piano per layer (`ns4-h7he`), program/menus/Extern (`ns4-ymci`) (after B3)
- [ ] **B10** Lock-free telemetry engine to GUI; test proves `process_block` does not allocate; LED ladders show morphed level (`ns4-gjy2`, after B5)
- [ ] **B11** Plugin editor (`nih_plug_egui`): resizable fixed aspect, HiDPI, size saved; loads in Reaper and Bitwig (`ns4-hud1`, after B5, B8)
- [ ] **B12** Standalone integration: panorama view, focused rack view, library drawer (`ns4-vip1`, after B5, B8)
- [ ] **B13** Performance and host verification: 2x-scale frame < 4 ms on the reference machine; host smoke test with evidence (`ns4-4ok5`, after B11, B12)

---

## Phase 7 (Track B): Full Firmware LZ Decompression & `CNSP` Sample Codec (`ns4-trkb`)

**Goal:** Unpack the full unencrypted `CODE` segments of the OS v1.66 firmware (`ARM0`, `DSPO`, `DSPS`, `DSPF`) and reverse-engineer the `CNSP` audio sample compression codec ([D12](DECISIONS.md#d12-pursue-track-b-reverse-engineering-for-cnsp-sample-codec-and-firmware-decompression), [research](research/1to1-emulation-feasibility.md)).

- [x] Prove `CODE` segments are unencrypted and solve the Clavia **LZO1X** stream format (replacing initial 16-bit LZ guess) ([research](research/clavia-lz-reconstruction.md))
- [x] Implement full-stream LZO1X decompressor (`decompress_clavia_lz` / `compress_clavia_lz`) and bank-relative extraction (`FirmwareContainer::extract_segment`) in `crates/nord-core/src/firmware.rs`
- [x] Perform differential header analysis across the 195 `.nsmp4` and 38 `.npno` `CNSP` headers (`0x30..0x80`) to map sample rate, bit depth, key-zone split tables, and loop points ([research](research/nord-piano-npno-and-codec.md))
- [x] Parse `.nsmp4` container chunks and expose initial instrument, key-map, zone, and stroke metadata (`crates/nord-core/src/cbin/sample.rs`)
- [x] Reverse-engineer the `CNSP` 4-bit DPCM audio payload decompression routine and implement `NpnoInstrument` and `decode_4bit_dpcm` in `crates/nord-core/src/cbin/piano_sample.rs`
- [x] Complete full-stream extraction of Image 1 (`410,321` B -> `798,912` B, `0xFBB813F8`) and Image 0 (`3,509,573` B -> `5,499,504` B, `0xDE902503`) via `ns4 inspect --verify --unpack`
- [x] Reverse-engineer `ARM0` preset MurmurHash2 (`0x0a5a44`), fix `.ns4p` `0x0B` / `0x32` hash bit offsets, and wire 1-click `.npno` / `.nsmp4` auto-resolution into `nord-core` & `nord-gui`; extract and calibrate the 5×128 OS v1.66 Synth/Env/Filter/LFO tables (`0x53abe4..0x53b5e4`) and Rotary `800 Hz` / Bass-Horn constants (`ns4-dspf`, [research](research/sharc-dsp-and-preset-hash-teardown.md))
- [x] Implement a clean-room, zero-allocation `CNSP` decoder in `crates/nord-core` enabling users to load their own legally owned `.npno` and `.nsmp4` files directly from a local `.ns4b` backup (`sample.rs`, `piano_sample.rs`, `backup.rs`)
- [x] Direct A/B benchmark loop: `ns4 render-patch <entry> --backup <.ns4b> --wav out.wav [--notes 60,64,67] [--velocity V] [--hold F] [--sustain] [--cc ctrl:val@frame]` renders any factory `.ns4p` with its `.npno`/`.nsmp4` offline; compare via `tools/video_ref.py compare <hw.wav> <out.wav>` (`ns4-uenj`)

---

## Parallel Lanes & Bean Dependency Graph

| Lane | Active Beans | Blocked By | Scope |
|---|---|---|---|
| **Lane A — Format & Preset Bitmaps** | `ns4-2ih2` | `ns4-3387` (done) | Decode all bit offsets in `.ns4o`, `.ns4n`, `.ns4y`, `.ns4p` + JSON patch converter |
| **Lane B — Organ, Piano & Synth DSP** | `ns4-yaip` | `ns4-3387` (done) | Complete Vox/Farfisa/Pipe/Scanner, Wave 2 SuperSaw/Wavetable/Arp, SFZ loader, FDN Reverb |
| **Lane C — 1:1 Panel GUI, Standalone & DAW Plugin** | `ns4-707f` | `ns4-3387` (done) | `crates/nord-plugin` (`nih-plug` CLAP + VST3 + Standalone) & 1:1 front-panel UI ([spec](specs/visual-style-and-panel.md)) |
| **Lane C2 — Phase 6b Hardware Parity & 2.5D Panel** | `ns4-xc5n` | `ns4-707f` | Control inventory, `crates/nord-panel`, baked-sprite renderer, parameter parity, plugin editor ([control inventory](specs/control-inventory.md)) |
| **Lane D — Track B Firmware & `CNSP` Codec** | `ns4-trkb` | `ns4-3387` (done) | Full custom LZ firmware unpacker + `CNSP` (`.npno` / `.nsmp4`) sample codec decoder |


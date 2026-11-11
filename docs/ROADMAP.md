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
   │
   └──► Phase 7 (Track B): Full Firmware LZ Decompression & `CNSP` Sample Codec Reverse Engineering
```

---

## Phase 0: Reverse Engineering, Hardware Teardown & Specifications (`ns4-3387`)

- [x] Dissect `Nord Stage 4 Factory Restore RevK v1.66.ns4b` archive (`meta.xml`, `product_id=46`, build `2934`, 1,171 files census) ([research](research/ns4-backup-and-cbin-teardown.md))
- [x] Reverse-engineer the 48-byte `CBIN` v1 header and bitstream embedding across `.ns4p` (868 B), `.ns4y` (541 B), `.ns4o` (183 B), `.ns4n` (195 B), `.ns4l` (868 B), `.ns4t` (124 B) ([spec](specs/cbin-and-presets.md))
- [x] Extract and map the OS v1.66 firmware payload (`RCDATA 101`, 4,037,080 B) into `ARM0` host MCU and `DSPO` / `DSPS` / `DSPF` SHARC DSP partitions ([research](research/ns4-firmware-teardown.md))
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
- [x] Map global 45-byte `.ns4p` header (`0x30..0x5C`: Split points, Crossfade widths, Scene I/II masks, Master Clock BPM, Morph slots)
- [x] `ns4 convert <file.ns4p> --to-json` / `--from-json` human-readable patch converter and `.ns4p` → `Stage4Engine` preset loader

---

## Phase 2: Organ Engine DSP (`DSPO` High-Level Emulation) (`ns4-orgn`)

**Goal:** 100% mathematical physical modeling of the 2-layer Nord Stage 4 Organ section and Model 122 Vintage Rotary Speaker.

- [x] `crates/nord-core/src/organ.rs`: B3 91-tonewheel frequency & phase generator, 9-drawbar (`16'` to `1'`) harmonic bus mixer with octave foldback, single-triggered 2nd/3rd harmonic percussion envelope (`Soft`/`Fast`), key-click transient, and Model 122 Rotary Speaker (800 Hz crossover + Slow/Fast ballistics + stereo Doppler)
- [ ] Exact 196-tooth Hammond drive/driven integer gear ratio table and per-tonewheel pickup calibration curve (`B3` vs `B3 Bass` mode)
- [ ] Tonewheel transformer crosstalk matrix and palladium contact bounce key-click bandpass filter
- [x] 18-section LC phase-shift delay-line **Scanner Vibrato / Chorus** (`V1`, `V2`, `V3`, `C1`, `C2`, `C3`)
- [x] Authentic **Vox Continental** (`16'`, `8'`, `4'`, `II`, `III`, `IV`, Sine + Reed wave) and **Farfisa Compact** (`Bass16`..`Str4` multi-tab bandpass + brilliance boost) models
- [x] **Pipe 1** (liturgical flute/principal with wind chiff) and **Pipe 2** (tracker organ swell/great registration) voicing models
- [ ] Rotary Speaker `Close Mic` / `Angle` / `Stop Mode` acoustic cabinet reflection modeling

---

## Phase 3: Synth Engine DSP (`DSPS` Nord Wave 2 Derived) (`ns4-synt`)

**Goal:** Full 3-layer (`Synth A`, `Synth B`, `Synth C`) 46-voice polyphonic Nord Wave 2 synthesis engine.

- [x] `crates/nord-core/src/synth.rs`: Anti-aliased `PolyBLEP` Virtual Analog oscillator (`Saw`, `Pulse/PWM`, `Triangle`, `Sine`), `Wavetable` & `FM` modes, Zero-Delay Feedback (`ZDF`) 12/24 dB Transistor Ladder (`LP M` / `Ladder24`) & SVF (`LP12`, `BP`, `HP`) filters, and ADR envelope
- [x] Multi-oscillator `ANALOG` configurations: `Super Saw` (7 detuned PolyBLEP saws matching `DETUNE: 3.4` on the Synth OLED), `Sync`, `Bell`, `Sub`, and `Noise`
- [ ] 64 band-limited mipmapped single-cycle Wavetables (`WAVETABLE` mode) with spectral formant shaping
- [ ] 2/3-Operator harmonic & inharmonic `FM` router with feedback and envelope modulation
- [ ] `SAMPLES` mode SFZ/WAV sample player with loop crossfade and `Soft`/`Bright` timbre damping
- [x] Polyphonic voice allocator across `Synth A/B/C` (16 pre-allocated voices per layer with dynamic envelope stealing)
- [x] Tempo-synced `LFO` (Triangle, Saw, Square, Sample & Hold -> `FILTER`, `OSC PITCH`, `OSC CTRL`) and `ARPEGGIATOR / GATE` (`Up`, `Down`, `UpDown`, `Random`, `Poly`, 1-4 octaves)

---

## Phase 4: Piano Engine DSP (`DSPF` Sample + Modal Resonance) (`ns4-pno4`)

**Goal:** 2-layer (`Piano A`, `Piano B`) 120-voice polyphonic acoustic/electric piano engine with sympathetic string resonance and pedal mechanics.

- [x] `crates/nord-core/src/piano.rs`: Multisample voice state, `KB TOUCH` velocity curves (`Light`, `Med`, `Heavy`), `TIMBRE` spectral presets (`Soft`, `Mid`, `Bright`, `Dyno1`, `Dyno2`), `Sustain` damper logic, and 12-band Modal Sympathetic String Resonance bank
- [ ] Streaming / RAM-cached open **SFZ / WAV multisample loader** supporting multi-velocity layers and key-off release samples (`SOFT REL`)
- [x] Pitch-specific **Undamped Note Sympathetic Resonance**: holding silent keys while striking lower notes excites the exact overtone resonators of the held notes
- [ ] Velocity-sensitive **Dynamic Pedal Noise** (`PED NOISE`) for Damper (`CC64`), Sostenuto (`CC66`), and Una Corda (`CC67`)
- [ ] Piano `UNISON` (`1`, `2`, `3` subtle hammer-detune width) and `DYN COMP` (`1`, `2`, `3` dynamic range compression)
- [ ] 120-voice polyphony steal priority algorithm (preserving bass roots, top melody notes, and newly struck notes over decaying sustained mid-register voices)

---

## Phase 5: 7-Layer Routing, Morphing Matrix & Per-Layer FX (`ns4-fxrt`)

**Goal:** Complete 7-layer stage routing, 3-source Morphing, Layer Scene II, and 7 independent per-layer FX racks.

- [x] `crates/nord-core/src/morph.rs`: 3-source Morphing Matrix (`WHEEL`, `A.T.`, `CTRLPED`), `LAYER SCENE II` switcher, and 4-zone Keybed Split router with `Off` / `Small` (6 st) / `Large` (12 st) crossfades
- [x] `crates/nord-core/src/fx.rs`: Per-layer 5-slot FX chain (`MOD 1`, `MOD 2`, `AMP SIM/EQ`, `DELAY`, `REVERB`) + downstream `MasterCompressor` (`COMP`), `MasterClock` (`MST CLK`), and `CS4392` + `LM833` output stage
- [x] `crates/nord-core/src/midi.rs`: Complete Nord Stage 4 MIDI CC decoder/encoder
- [x] `crates/nord-core/src/engine.rs`: Integrated 7-layer `Stage4Engine` rendering `Organ A/B`, `Piano A/B`, and `Synth A/B/C`
- [x] Full 3-band semi-parametric EQ in `AMP SIM / EQ` (`BASS` 100 Hz, sweepable `MID` 200 Hz–8 kHz + gain, `TREBLE` 4 kHz) + Cabinet impulse filters (`Small`, `JC`, `Twin`) + `TO ROTARY` routing switch
- [ ] `DELAY` enhancements: smooth `ANALOG` tape pitch-slew on tempo changes, `PING PONG` stereo tap pattern, and `HP`/`LP`/`BP` feedback loop filters + `VARIATION`
- [x] `REVERB` enhancements: stereo FDN (Feedback Delay Network) for `Booth`, `Room`, `Stage`, `Spring`, `Hall`, `Cath` with `Bright`/`Dark` damping and `VAR/CHORALE` modulation

---

## Phase 6: 1:1 Front-Panel GUI, Standalone App & CLAP/VST3 Plugin (`ns4-707f`)

**Goal:** Deliver both a **CLAP + VST3 DAW Plugin** and a **Standalone desktop application** (`D11`) with a 1:1 visual recreation of the Nord Stage 4 hardware panel ([spec](specs/visual-style-and-panel.md)).

- [ ] `crates/nord-plugin`: `nih-plug` workspace crate exporting **CLAP** (`nih_export_clap!`), **VST3** (`nih_export_vst3!`), and **Standalone** (`nih_export_standalone!`) targets wrapping `Stage4Engine`
- [x] Custom 1:1 Hardware Panel Renderer (following `docs/specs/visual-style-and-panel.md`):
  - [x] **Bay 1 — Left Master & Rotary (`#8E1B1F`):** Wooden Pitch Stick, Stone Mod Wheel, `MASTER LEVEL` knob, `ROTARY SPEAKER` (`DRIVE` 0–10, `CLOSE MIC`, `STOP MODE`, `ANGLE`, `SLOW`/`FAST` LEDs, `MORPH`)
  - [x] **Bay 2 — Organ Section (`#323D47`):** Dual physical faders `A`/`B` with **11-segment green LED ladders**, `ORGAN MODEL` triangle selector, `VIB/CHORUS` matrix, `B3 PERCUSSION` switches, **9 Physical Drawbars (`16'`..`1'`) with B3 cap colors and adjacent 8-segment red LED ladders (`1..8`)**, `OCTAVE SHIFT`, `KB ZONE` (4 green LEDs)
  - [x] **Bay 3 — Piano Section (`#323D47`):** Dual physical faders `A`/`B` with **11-segment green LED ladders**, `ACOUSTICS` (`SOFT REL`, `STRING RES`, `PED NOISE`), `UNISON` (`1/2/3`), `KB TOUCH` (`LIGHT/MED/HEAVY`), `DYN COMP` (`1/2/3`), `TIMBRE` (`BRIGHT/MID/SOFT/DYNO1/DYNO2`), `PIANO SELECT` triangle matrix + `MODEL LIST` stepped encoder
  - [x] **Bay 4 — Central Program & Morph Section (`#8E1B1F`):** `MORPH ASSIGN` (`WHEEL`, `A.T.`, `CTRLPED` hold-to-assign), `SPLIT` (3 zones + crossfade), `MST CLK`, `TRANSP`, **Central Monochrome Program OLED Display (`A:11 Nord Stage 4`)**, `LIVE MODE`, **`LAYER SCENE II`** button, `PROGRAM 1..8` buttons
  - [x] **Bay 5 — Synth Section (`#323D47`):** Triple physical faders `A`/`B`/`C` with **11-segment green LED ladders**, **Synth Graphic OLED Display (`OSC WAVEFORM` + real-time waveform plot + 4 soft-tabs)**, `MODE`, `ARPEGGIATOR / GATE`, `VOICE` / `VIBRATO`, `LFO`, `OSCILLATORS`, `FILTER` (`LP12/24/LP M/BP/HP` + `FREQ`/`RES`), `AMP ENVELOPE`, `UNISON`
  - [x] **Bay 6 — Layer Effects Section (`#323D47`):** Crimson `FX FOCUS` layer selector (`ORGAN A/B`, `PIANO A/B`, `SYNTH A/B/C`, `GROUP`), `MOD 1`, `MOD 2`, `AMP SIM / EQ`, `DELAY`, `COMP`, and `REVERB` boxes
  - [x] **Bottom 88-Key Interactive Keybed:** Triple-sensor velocity click/drag, 3 red Split-Point LEDs with visual crossfade shading, and live MIDI note-on highlights
- [ ] Update `.github/workflows/ci.yml` and `.github/workflows/release.yml` to build and package Standalone executables + `.clap` + `.vst3` bundles for Windows, Linux, and macOS

---

## Phase 7 (Track B): Full Firmware LZ Decompression & `CNSP` Sample Codec (`ns4-trkb`)

**Goal:** Unpack the full unencrypted `CODE` segments of the OS v1.66 firmware (`ARM0`, `DSPO`, `DSPS`, `DSPF`) and reverse-engineer the `CNSP` audio sample compression codec ([D12](DECISIONS.md#d12-pursue-track-b-reverse-engineering-for-cnsp-sample-codec-and-firmware-decompression), [research](research/1to1-emulation-feasibility.md)).

- [x] Prove `CODE` segments are unencrypted and solve the 16-bit Clavia LZ match token format (`distance = token >> 3`, `length = (token & 7) + 2`) ([research](research/clavia-lz-reconstruction.md))
- [x] Implement 16-bit Clavia LZ decompressor (`decompress_clavia_lz` / `compress_clavia_lz`) with sliding-window history buffer in `crates/nord-core/src/firmware.rs`
- [ ] Complete full-stream extraction of Image 1 (`411 KB` -> `798 KB`) and Image 0 (`3.51 MB` -> `5.49 MB`) and map the unpacked `ARM0` host binary and `DSPO` / `DSPS` / `DSPF` SHARC DSP segments
- [ ] Perform differential header analysis across the 195 `.nsmp4` and 38 `.npno` `CNSP` headers (`0x30..0x80`) to map sample rate, bit depth, key-zone split tables, and loop points
- [ ] Reverse-engineer the `CNSP` audio payload decompression routine from the unpacked `ARM0` / `DSPF` / `DSPS` binaries
- [ ] Implement a clean-room, zero-allocation `CNSP` decoder in `crates/nord-core` enabling users to load their own legally owned `.npno` and `.nsmp4` files directly from a local `.ns4b` backup

---

## Parallel Lanes & Bean Dependency Graph

| Lane | Active Beans | Blocked By | Scope |
|---|---|---|---|
| **Lane A — Format & Preset Bitmaps** | `ns4-2ih2` | `ns4-3387` (done) | Decode all bit offsets in `.ns4o`, `.ns4n`, `.ns4y`, `.ns4p` + JSON patch converter |
| **Lane B — Organ, Piano & Synth DSP** | `ns4-yaip` | `ns4-3387` (done) | Complete Vox/Farfisa/Pipe/Scanner, Wave 2 SuperSaw/Wavetable/Arp, SFZ loader, FDN Reverb |
| **Lane C — 1:1 Panel GUI, Standalone & DAW Plugin** | `ns4-707f` | `ns4-3387` (done) | `crates/nord-plugin` (`nih-plug` CLAP + VST3 + Standalone) & 1:1 front-panel UI ([spec](specs/visual-style-and-panel.md)) |
| **Lane D — Track B Firmware & `CNSP` Codec** | `ns4-trkb` | `ns4-3387` (done) | Full custom LZ firmware unpacker + `CNSP` (`.npno` / `.nsmp4`) sample codec decoder |


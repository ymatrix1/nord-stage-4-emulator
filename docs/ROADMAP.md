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
          │
          └──► Phase 7b (Track B): SHARC Oracle, ARM Firmware Emulation & Firmware Parity (opt-in paths in `nord-core`)
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

The ticks below mean the model exists and is tested. Which parts match the firmware (Pipe bank, B3 wheels, weights) and which stay `ASSUMED` is tracked in [Phase 7b.3](#phase-7b-track-b-sharc-oracle-arm-firmware-emulation--firmware-parity-ns4-trkb) and [organ-engine](specs/organ-engine.md) s3.1.

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
- [x] Firmware velocity -> level law traced by running the real firmware with a stubbed sound library (`tools/arm_piano_load.py`): `n = rec[0x5E] + 2X + 60 - sum(b)`, `X(velocity)` table and `level_q23` verified; opt-in model `piano/velocity_level.rs` (not in the default render; [research](research/piano-sound-load.md))
- [x] Level of any note/velocity computed from the parsed `.npno` (`piano/npno_level.rs`): stroke selection, `rec[0x5E]`, `b1..b3`, `X`; 96 of 96 random main-voice cases on four sounds; `PianoLayer::firmware_velocity_level` is the default for `.npno` sounds (D23; `ns4 render-patch --legacy-piano-level` restores the touch curve; extra voices played by `piano/extra_voice.rs`; gain word verified 12 of 12; [research](research/piano-sound-load.md))
- [x] Downstream level audit of the piano (`piano/downstream.rs`, [piano-downstream-level](research/piano-downstream-level.md), D24): SMP bus word `(y << 17) * gain >> 31` tied to the engine unit, layer level, master, string resonance, dyn comp and unison checked at 0 dB; opt-in `firmware_bus_scale` (`--firmware-piano-bus`, 0.5 against the legacy 0.55, -0.83 dB); the +12 dB of the video segment is the fixed replay velocity (+4.8 dB at velocity 64, +2.4 dB against the Pipe yardstick), not an engine stage
- [x] Per-note replay velocity of the Velvet Grand clip (`tools/piano_velocity.py`, `render-patch` per-event `velocity`, [piano-note-velocity](research/piano-note-velocity.md)): per-note inversion is not identified (worse than uniform); a uniform replay velocity of about 88 matches the spectrum (all bands within 2.7 dB, brilliance +0.5 dB) and its +7.3 dB RMS gap equals the Pipe yardstick; piano layer level law `(v / 127)^2` read from handler `0xC011A7A4`
- [x] Piano layer level handler `0xC011A7A4` run live (`tools/arm_layer_level.py`): `(v / 127)^2`, opt-in `--firmware-piano-layer-law` ([piano-bus-convention](research/piano-bus-convention.md), D28)
- [x] SMP post stage run live through the whole audio interrupt (`sharc-ref` `irq ... break=PC` / `resume`, `smp_post_stage.scn`): unity, `ring = bus * 2^-29`; receiver of the ring and the `0xC023195C` slew trace still open ([piano-bus-convention](research/piano-bus-convention.md) s2b, D30)
- [ ] Decode the receiver of the SMP bus (first consumer found: `FLOAT BY -29` at pc `0x1C0940`; the ring is the SMP serial transmit buffer, its receiver and the master scale are open) to promote `firmware_bus_scale` to default (OQ20)
- [x] Receiver of the SMP stream found in the FX image (SPORT3B ten-word frames, no multiply, unity master IIR, master gain `DM(0xB651A)` 0.1866, DAC `fix(clip(2 x, 0.999)) * 2^31`; [piano-receiver-scale](research/piano-receiver-scale.md), D39, `tools/fx_input_path.py`, `piano/downstream.rs::bus_word_dac_fraction`); the defaults stay because `DM(0xB6518)` and the `0xC023195C` target are open
- [x] Master level word `DM(0xB6518)` writer found (`(volume / 127)^2 * knob taper`, 1.0 at the defaults; `tools/arm_master_level.py`, `piano::downstream::master_level_word`), `0xC023195C` targets resolved (PNO core 1 organ chain gains, FX core 1 / 2 bus gains), organ chain word 1.6 at level 127 ([piano-receiver-scale](research/piano-receiver-scale.md) s5, D48)
- [x] Layer mode flag resolved (Reverb GLOBAL) and scale cells per engine (organ 1.6, piano 1.0, synth 1.0; `tools/arm_master_level.py modes`), verified chain exposed opt-in (`--organ-level chain`, `--firmware-piano-chain`), Part A clips re-run ([piano-receiver-scale](research/piano-receiver-scale.md) s6, D49)
- [ ] Close the absolute level chain end to end: the ~25 dB piano / organ spread under the common path, White Grand / DSoundman / Audible Genius clips under the chain, source of the start-up volume event; then flip the level defaults (re-baseline `render_hash`, GUI Firmware bar)

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
- [x] Release pipeline hardened and dry-runnable: `tools/release_package.py` / `release_verify.py`, tag-vs-`Cargo.toml` check, per-OS install README, SHA256 files, no repeated test suite ([RELEASING.md](RELEASING.md))
- [ ] First tagged release published (`v0.1.0`); needs a maintainer-run dry run, then the tag

---

## Phase 6b: 1:1 Hardware Parity & 2.5D Panel (`ns4-xc5n`)

**Goal:** Every front-panel control in the Nord Stage 4 manual (ch. 3-9) exists in the emulator with a layout rect, a renderer and a binding (`Bound(param_id)`, `UiOnly(reason)` or `Unsupported(bean)`), drawn as one 2.5D panel (`crates/nord-panel`) shared by the standalone app and the plugin editor ([control inventory](specs/control-inventory.md), [D15](DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering), [D16](DECISIONS.md#d16-phase-6b-scope-original-art-functional-or-stub-controls-ha88-first-additive-host-ids), OQ13 in [OPEN-QUESTIONS](OPEN-QUESTIONS.md)). Host parameter IDs already shipped are a stable contract: additive changes only.

- [x] **B1** Docs correction: reopen Phase 6 bays, control-inventory spec, visual-spec discrepancies, D15 (`ns4-7ky2`; delivered with the planning PR and kept current since)
- [ ] **B2** Control inventory data: every control from manual ch. 3-9 with page and provenance; counts match manual and photo (`ns4-3n0d`, after B1). **Partly delivered 2026-11-13:** `crates/nord-panel` lists 273 controls checked against the manual text; photo check covers headline counts only, so the box stays open
- [x] **B3** Host parameter-ID stability test: committed snapshot of ID/type/range/default; remove/rename fails, add passes (`ns4-xhbf`, `crates/nord-plugin/param-ids.snapshot`, 755 ids now, additive only)
- [x] **B4** Split oversized `nord-gui` files (`panel.rs`, `library.rs`, `audio.rs`, `widgets.rs`) and add `crates/nord-panel` skeleton; behaviour unchanged (`ns4-r7vt`; `panel/`, `library/`, `audio/`, `widgets/` are directory modules, every file under 400 lines, frame hash byte-identical)
- [x] **B5** Host and behaviour layer: `PanelHost`, `PanelAction`, Shift / hold / double-tap / selector cycling / pot snap / Live Mode with unit tests (`ns4-q6p7`; `crates/nord-panel/src/behaviour/`, timings `ASSUMED` except the 500 ms hold)
- [ ] **B6** Headless golden-image harness: CPU raster via `tiny-skia`, per-pixel tolerance + error budget, goldens at 0.5x; optional local-only photo comparison skipped without `.reference/` (`ns4-7cv6`, after B4)
- [ ] **B7** Asset baker (`xtask bake-panel`) and material/shadow system: Blender-rendered sprites and filmstrips from our own models, committed renders with content hashes, `tiny-skia` for flat vector layers, theme tokens; first proves one knob filmstrip and one fader cap and measures atlas size (D15, `ns4-77s9`, after B1, B4)
- [x] Program OLED boxed `E` edited marker, `Live N` title and per-section rows (`ns4-ow85`; piano size letter and B3 drawbar digits added with `ns4-bjvn`, preset ids still open: no data source). Preset Organize (Swap / Move / Ok / Undo over the Preset Library, written by Export backup) and the three Synth OLED dials as per-page bindings are delivered (`ns4-r8t9`)
- [x] Preset Library STORE and real preset loading (`ns4-rt23`): the preset browser loads `.ns4o` / `.ns4n` / `.ns4y` entries into the section (undoable, sounds resolved like a program's), STORE of a section preset with Preset Organize and Export backup, header checksum = CRC-32 from `0x2C` (D51, `ns4 preset-check`: 544 presets, 0 problems); open: Synth sound edits in a stored preset (OQ30), collapsed decoder bits (OQ31)
- [ ] **B8a-g** Section renderers with a golden image each and every section control bound: master/rotary/wheels (`ns4-0sud`), organ (`ns4-9h25`), piano (`ns4-otuf`), program + both OLEDs (`ns4-z6fi`), synth (`ns4-irgv`), effects (`ns4-hcuh`), keybed (`ns4-jpgx`) (after B2, B5, B6, B7). **Status 2026-11-14:** every one of the 274 inventory controls is drawn at its layout rectangle (`crates/nord-panel/src/layout/`, mm rects from the 2000 px photo, 36 estimated, `ASSUMED`) by the bays under `crates/nord-gui/src/panel/` (master, organ, piano, program + keybed, synth with the OLED page machine, effects) and every wired control sends an `AudioCommand::{Organ,Piano,Synth,Fx,Program}` edit; the Program OLED shows the 8 menus (58 pages) from `nord_panel::menus`. Controls whose engine feature is still missing are drawn dimmed with a tooltip naming it (inventory `Unsupported`: 0 left; SECTION EDIT works since `ns4-fqym`: held or sticky, an edit in the Organ, Piano, Synth or Effects bay reaches every Layer of the Section, [control inventory](specs/control-inventory.md) section 10a; totals `Bound` 208 / `UiOnly` 67 in section 10). Label legibility (2026-11-14, `ns4-` label legibility audit): the headless audit now also checks clipped text, section and group-frame crossings, knob rings and a 0.9 px gap, and passes with an empty allow-list after the fixes (Rotary Speaker box widened leftwards, fit-aware labels, OLED soft labels, SOLO legend, caption spacing; see [GUI testing](gui-testing.md)). Round 3 (2026-11-14, `fix/panel-titles-frames`): the printed identification of controls is in: 29 group frames and titles from `nord_panel::layout::frames` (Piano ACOUSTICS / UNISON / KB TOUCH / DYN COMP / PIANO SELECT with the type names and a corrected TIMBRE order, Organ model / vibrato / percussion, nine Synth boxes, six Layer Effects boxes, Program MORPH ASSIGN / SPLIT / MST CLK / TRANSP / PRESET LIBRARY / PROGRAM), 0-10 scale ends on the Synth and Effects knobs, drawbar names and 1-8 numerals, INFO / LIST names under the OLED dials, L / M / H beside the split LEDs, and dimmed legends held to 4.5:1 contrast; the audit stays at 0 with an empty allow-list (see [visual style](specs/visual-style-and-panel.md)). Round 4 (2026-11-14, `fix/panel-numerals-variations`): numerals around every Synth / Layer Effects / Rotary DRIVE knob (0-10, -15...15, -10...10, 200...8K; knob bodies 3 px smaller), the printed variation lists beside the Effects selectors (names from manual p.49-53), 1 2 3 under the UNISON / DYN COMP LEDs and H M L under KB TOUCH, and the Program box clear of the PAGE / BANK buttons; audit 0 with an empty allow-list. Open: golden images (B6) and the 2.5D look (B7, deferred by decision).
- [ ] **B9a-d** Parameter and engine parity, additive IDs only: synth per layer (`ns4-4w9l`), effects (`ns4-e3zs`), organ/piano per layer (`ns4-h7he`), program/menus/Extern (`ns4-ymci`) (after B3). **Status 2026-11-14:** delivered per-layer params for piano, synth (voice, LFO, arp incl. gate, pattern edit, KB hold, clock sync), organ (poly percussion, Drawbar Live, vibrato, rotary), FX (variations, sharing, A-WAH, PUMP, SPIN, Ensemble, reverb variation), Aux KB, pedals incl. Triple Pedal, Extern MIDI out, program ops (transpose, panic, layer init, undo/redo, copy/paste), system/sound/output settings and the menu model. Remaining: output routing, MIDI filters and Program Change, Extern devices and velocity, organ key trigger point, Foot Switch B, the arp mode bits of a program (Poly / Gate) and the preset-dial readout of the edited state (the factory arpeggiator patterns are decoded and imported, `ns4-l0km`), Live slots inside `.ns4b`, pitch bend on organ/piano layers
- [x] **B10** Lock-free telemetry engine to GUI; test proves `process_block` does not allocate; LED ladders show morphed level (`ns4-gjy2`, after B5). Delivered: seqlock `TelemetryCell` + 34-word `Telemetry` snapshot ([telemetry spec](specs/telemetry.md)); `crates/nord-core/tests/no_alloc_audio_path.rs` counts 0 allocator calls over 400 blocks with publishing; the organ / piano / synth ladders, organ drawbar ladders, Aux KB LEDs, MIDI / EXTERN LEDs, split LEDs, rotary LEDs and the UNDO / REDO soft buttons read the snapshot. Per-layer focus stays panel-only; drawbar morph and an arpeggiator do not exist in the engine yet (those fields are `ASSUMED`)
- [ ] **B11** Plugin editor (`nih_plug_egui`): resizable (letterboxed, no hard aspect lock), HiDPI, size saved; implemented and headless-tested, loading in Reaper and Bitwig still unverified, see `docs/plugin-editor.md` (`ns4-hud1`, after B5, B8)
- [ ] **B12** Standalone integration: panorama view, focused rack view, library drawer (`ns4-vip1`, after B5, B8). Delivered 2026-11-14: library drawer (docked, now slides, `L`), focused section view (`Z`, `1`-`5`), panorama strip with a draggable viewport (`P`, `crates/nord-gui/src/panel/panorama.rs`); headless-tested, not checked on a real display. Open: `PanelHost over AudioHandle`, and the strip draws section outlines, not a live thumbnail
- [ ] **B13** Performance and host verification: 2x-scale frame < 4 ms on the reference machine; host smoke test with evidence (`ns4-4ok5`, after B11, B12) **Status 2026-11-14 (headless part done, B13 stays open):** `ns4 bench` and `nord_core::bench` measure the worst case (7 layers, 88 keys, all FX, 4 outputs: 3.6 to 4.5 ms per 128-frame block, 0.59x to 0.75x realtime; typical program 0.95 to 1.2 ms) against the 0.8 ms PRD budget: **not met** (TD-14, nothing optimised: no clearly wasteful hotspot found). Counting-allocator tests show 0 allocations in `process_block`/`process_block_multi` (worst case) and in `NordStage4Plugin::process`. Panel CPU frame (run + tessellate) at 2x: 2.6 to 3.2 ms full panel, 2.3 to 3.0 ms panorama, under 4 ms in 11 of 12 passes, no regression from PR #306 (GPU time not measured). `clap-validator` 0.4.1: 30 of 44 pass; one real crash found and fixed (CLAP wildcard note 255 panicked the engine); 4 failures are upstream nih-plug (state rescan, corrupt-state allocation). **Not done:** any DAW run (Reaper, Bitwig), VST3 validation, 2x GPU frame time, repaint-on-change audit; the manual smoke checklist is in [HOST_COMPATIBILITY](HOST_COMPATIBILITY.md) 7.7.

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

## Phase 7b (Track B): SHARC Oracle, ARM Firmware Emulation & Firmware Parity (`ns4-trkb`)

**Goal:** Use the firmware's own DSP code as a test oracle ([D18](DECISIONS.md#d18-rust-sharc-oracle-validated-against-the-adi-cycle-accurate-simulator)) and the host firmware as the parameter-stream source ([D19](DECISIONS.md#d19-arm-firmware-emulation-harness-unicorn-as-the-parameter-stream-source)), then move proven constants and algorithms into `nord-core` as opt-in paths ([D22](DECISIONS.md#d22-opt-in-firmware-paths-in-nord-core-defaults-change-only-when-verified-end-to-end)). Specs: [sharc-core](specs/sharc-core.md), [organ-engine](specs/organ-engine.md), [effects-and-routing](specs/effects-and-routing.md), [piano-engine](specs/piano-engine.md), [synth-engine](specs/synth-engine.md). Policy: derived values only ([D20](DECISIONS.md#d20-derived-values-only-no-firmware-bytes-in-the-repository)).

### 7b.1 SHARC oracle (`crates/nord-sharc`)

- [x] M1 decoder: `visa/` ports `tools/sharc_visa.py`; `sharc-disasm` output equals the Python listing on about 970,000 instruction lines ([sharc-core](specs/sharc-core.md) s5)
- [x] M2 core model: registers, DAGs, loops, calls, SIMD, shifter, bit FIFO, float and fixed ALU/multiplier, long-word moves, interrupts, timer, PC/loop/status stacks. Each behaviour is checked against the ADI simulator ([float](research/sharc-float.md), [fixed](research/sharc-fixed.md), [simd](research/sharc-simd.md), [stacks](research/sharc-stacks.md), [loops](research/sharc-loops.md), [type 2c](research/sharc-type2c.md), [bit FIFO](research/sharc-bitfifo.md), [sequencer](research/sharc-sequencer.md), [leftovers](research/sharc-leftovers.md))
- [x] Census diff: every instruction form in the firmware images run on the simulator and the model, 12 model gaps fixed ([sharc-census-diff](research/sharc-census-diff.md)); SHARC+ flag masks measured on the ADSP-21571 simulator ([sharc-plus](research/sharc-plus.md))
- [x] Headless CCES simulator runner (`TpsdkServer`, `docs/research/sharc-seq/sim_run.py`, `sim_batch.py`, 21489 and 21571 targets)
- [x] Chip identity and peripherals: FX/PNO/ORG on ADSP-2157x, SY1SY2/SMP on ADSP-2148x ([sharc-chip-identity](research/sharc-chip-identity.md), [D17](DECISIONS.md#d17-chip-identity-fx-pno-and-org-run-on-adsp-2157x-sharc-sy1sy2-and-smp-on-adsp-2148x)); `core/periph/` models SEC0, SPI2 peek port, descriptor DMA, TRU, LP1, L2 and the 2148x SPI-slave boot
- [x] M3 start-up of FX, ORG, SY1SY2 and SMP to their idle loops; PNO core 1 runs the complete pass (`sharc-pass`, [pno-full-pass](research/pno-full-pass.md))
- [x] Routine-level diff: PNO bank `0x122620` and mix stage `0x12268F` bit-exact against the ADSP-21571 simulator ([sharc-routine-diff](research/sharc-routine-diff.md); audio chain in [organ-oracle-audio](research/organ-oracle-audio.md))
- [x] Binaries: `sharc-run`, `sharc-org`, `sharc-bank`, `sharc-organ`, `sharc-chain`, `sharc-pass`, `sharc-fx`, `sharc-ref`, `sharc-census-run`, `sharc-disasm` ([ARCHITECTURE](ARCHITECTURE.md) s4)
- [ ] Simulator probes P1..P8 (L2 size, SEC0 `CSID`/`END`, memory DMA, 2D DMA, peek-port descriptor timing, SPI2 status, reset values; [sharc-chip-identity](research/sharc-chip-identity.md) s7): promote or correct the ASSUMED peripheral behaviours
- [ ] Model the `0x28xxxxxx` core aliases, SPORT/DAI audio DMA and wire `--sc57x` into `sharc-org`
- [ ] Oracle speed: long passes (`sharc-pass`, census diff on the 21571 target, about 140 s start-up per run) need profiling and a faster step loop before the oracle can sit in CI
- [ ] M5 host protocol and M6 engine comparison for every effect (the oracle plus the ARM harness make this one routine at a time; see 7b.4)

### 7b.2 ARM firmware emulation (`tools/arm_*.py`, [arm-fw-emulation](research/arm-fw-emulation.md))

- [x] `unicorn` harness runs real host-firmware functions and captures every ARM to DSP message as `(target, address, value)` ([arm-dsp-protocol](research/arm-dsp-protocol.md), [arm-param-messages](research/arm-param-messages.md))
- [x] Boot to the main event loop with timer interrupt injection and snapshots (`arm_steady.py`); edit registry of 422 parameters ([arm-edit-params.csv](research/arm-edit-params.csv)); edit ids driven through the real layer clients (`arm_edit_capture.py`)
- [x] Captured streams replay into the DSP images (`sharc-org --msg-script`, `sharc-pass --script`, `sharc-chain --script`)
- [x] ARM-side maps: organ weights and key to wheel map ([organ-weights](research/organ-weights.md)), model to slot table ([organ-model-paths](research/organ-model-paths.md), [b3-tone-source](research/b3-tone-source.md)), PNO module chain ([pno-module-chain](research/pno-module-chain.md)), FX chain install, type map and contexts ([fx-chain-install](research/fx-chain-install.md), [fx-type-map](research/fx-type-map.md), [fx-contexts](research/fx-contexts.md)), piano path ([piano-dsp-path](research/piano-dsp-path.md))
- [ ] Key-down capture: the keybed link (UART2 ring consumer) is not emulated, so no key event reaches the DSP messages (piano velocity law and organ key-down remain open; [piano-dsp-path](research/piano-dsp-path.md) follow-up 1)
- [x] Boot message log: every ARM to DSP store from reset (80,815 boot messages, real EMIFA ports) kept, fingerprinted and replayed into `sharc-pass` / `sharc-org` (PNO core 1 8,300 / 8,306 words, ORG 12,102 / 12,102); census of the stubbed SPI routines ([arm-bulk-tables](research/arm-bulk-tables.md), `tools/arm_bulk_tables.py`)
- [x] Boot state equality: PNO core 1 / 2, ORG core 1 / 2 and FX core 1 / 2 read back identical ([arm-boot-state](research/arm-boot-state.md)); the L2 table is delivered but not read by the ORG pass
- [x] `0xB2ECF` / `0xB2FF0` identified as layer-level gain sentinels; B3 tail re-derived on the full boot state ([arm-boot-rederive](research/arm-boot-rederive.md))
- [x] Corrected B3 tail ported with regenerated chord tests, oracle chords within 0.54 dB and video mid -1.1 dB ([b3-tail-bootstate](research/b3-tail-bootstate.md)); `B3Wheels` stays opt-in (RMS +8.0 dB)
- [x] Engine layer routing: section / layer enables VERIFIED on the ARM (`tools/arm_program_routing.py`), per-layer KB zone codes decoded, opt-in `FirmwareRouting` / `--firmware-routing`; B3 clip sub +16.5 to -1.6 dB, crest 12.8 to 17.2 dB, RMS +8.0 to +3.8 dB ([engine-routing](research/engine-routing.md))
- [x] Firmware map from the `kKbZone` code to the key range (OQ23): setters found (`0xC010ABA4`), case table, bound functions, 11 split keys and fade curves VERIFIED against the ARM, 384 / 384 factory programs equal the zone objects; split position decode table fixed (36 was missing) ([zone-key-ranges](research/zone-key-ranges.md))
- [ ] Flip `FirmwareRouting` to the default with a `render_hash` re-baseline: blocked by OQ25 (the empty-scene rule for nine factory programs is ASSUMED: no loader run, no hardware clip)
- [ ] SY / SMP equality, output-stage level for the B3 clip, Vox level word, Farfisa state; real flash reads (FAT sector reads of the `.ns4b` library: sample maps, stroke tables) instead of zeros
- [x] B3 clip gap decomposed: layer B (section II off in the program) and notes outside the zone cause the sub and most of the RMS gap; Farfisa sounds in the oracle with the key-down words ([organ-output-level](research/organ-output-level.md))
- [x] Engine rotary drive scale: preset unit 0..10 reaches the tube law unscaled (gain 8.3 instead of 1.73); opt-in `--rotary-drive scaled` ([b3-rotary](research/b3-rotary.md)); B3 clip with routing: presence -12 dB still fails, flip not made
- [ ] Engine: honour section I / II and zone flags for organ layers, then re-run `emu_vs_hw.py --only organ_b3`; Farfisa processor port; layer level law; Vox level word; real flash reads (FAT sector reads of the `.ns4b` library: sample maps, stroke tables) instead of zeros

### 7b.3 Organ

- [x] Firmware bank, mix stage, weights with smoothing, vibrato link, stereo lanes in `organ/`: Pipe1 / Pipe2 use the 92-wheel bank by default; weights are opt-in `WeightMode::Firmware` ([organ-engine](specs/organ-engine.md) s3.1.1 to 3.1.5)
- [x] Model to path map: B3 = generator `0x121BBE` + processor `0x121E2B`, Vox = `0x12272D` + `0x1229B3`, Pipe = `0x122620` + `0x12268F` ([D21](DECISIONS.md#d21-organ-model-to-dsp-path-map), [b3-generator](research/b3-generator.md), [organ-post-stage](research/organ-post-stage.md))
- [x] B3 pitch ladder (bit exact) and wheel tables as opt-in `ToneWheelSource::B3Wheels` ([organ-engine](specs/organ-engine.md) s3.1.6)
- [x] Vox chord end to end in the oracle (`sharc-pass`)
- [x] B3: per-key footage weights and per-wheel amplitudes from the captured key-down, frame rate settled ([organ-keydown-weights](research/organ-keydown-weights.md))
- [x] Pipe video gap: Pipe2 lines match the oracle to 0.00 dB on three chords in both weight modes (`organ/pipe_lines.rs`); the Space Drama II clip is synth-dominated (`tools/mix_fit.py`), default and `render_hash` unchanged ([pipe-video-gap](research/pipe-video-gap.md))
- [ ] Pipe-only hardware clip, organ level `L` and the Pipe weight scale (Legacy bus +14.4 dB over the ARM weight unit), wheel steps from the ARM increment words ([pipe-video-gap](research/pipe-video-gap.md) follow-ups)
- [x] B3: processor tail as an oracle-measured amplitude response (`b3_tail.rs`, [organ-b3-tail](research/organ-b3-tail.md))
- [ ] B3: dry gain writer, tail phase, multi-write `A`, drawbar sweep; then make `B3Wheels` the B3 default (3 chords: 10 / 8 / 6 of 13 / 13 / 11 lines within 1 dB; hardware clip mid band -11.9 dB)
- [x] Vox in Rust: post stage `0x12272D` and output stage `0x1229B3` with the captured key-down weights (four footage rows per key, no velocity), 9.9 cent offset explained ([organ-keydown-weights](research/organ-keydown-weights.md))
- [x] Vox oracle criterion: image start phases, 36-sine bank, instant weights; three chords within 0.34 dB on every line ([organ-vox-start-state](research/organ-vox-start-state.md))
- [x] Vox key-down timeline (replaces the click, crest 16.1 dB), drawbar value law and slot wiring, boot gain word ([organ-vox-click](research/organ-vox-click.md))
- [ ] Vox default: organ output level stage (RMS -13.7 dB) and the 1.5 to 8 kHz deficit (mid +7.8, presence -7.3, brilliance -9.5 dB) (`ns4-58cz`)
- [x] B3 / Vox presence-gap diagnosis: per-clip bands, stage ablations, tonal-peakiness and note-register tools (`tools/organ_gap.py`); no firmware stage found, no default flipped ([organ-presence-gap](research/organ-presence-gap.md))
- [ ] B3 / Vox presence: Twin amp type 289 port (Vox residual 7.5 / 9.7 dB) and a right-hand note list for the B3 clip, then re-run the D22 comparison
- [x] Farfisa generator `0x122A71`: bit-exact Python model, opt-in `ToneWheelSource::FarfisaWheels` ([organ-engine](specs/organ-engine.md) s3.1.8, [organ-farfisa-bass](research/organ-farfisa-bass.md)); B3 Bass `0x122296` and the B3 pass replayed in `sharc-pass` (`tools/organ_fb_pass.py`)
- [x] B3 Bass key-down weights decoded for every key from the ARM (`tools/organ_bass_keys.py`, `organ/b3_bass_keys.rs`); finding: the Bass output needs the per-key lattice words, so the line-gain stage is not wired ([organ-farfisa-bass](research/organ-farfisa-bass.md) s10)
- [x] B3 Bass processor state decoded: window increments per key, `G = phi(S)` accumulator and output law, line gain table, oracle chord goldens within 1.45 dB given the ring (`organ/b3_bass_state.rs`, [organ-farfisa-bass](research/organ-farfisa-bass.md) s11; not wired)
- [x] B3 Bass generator weight rule decoded (`T[note][n]`, `organ/b3_bass_gen.rs`, `tools/organ_bass_gen.py`) and the Bass stage wired opt-in (`--b3-bass-processor on`, `organ/b3_bass_stage.rs`, [organ-farfisa-bass](research/organ-farfisa-bass.md) s12): chord 60 64 67 +1.1 to +2.5 dB per oracle line, default not flipped (D41)
- [ ] B3 Bass stage within 1 dB (bit-exact `0x122296`, drawbar and percussion dependent key tables), B3 Bass hardware clip, default decision
- [x] Oracle chords of the Farfisa and B3 Bass processors with the corrected boot log and key-down words (`tools/organ_proc_oracle.py`, `sharc-pass --rec`, [organ-farfisa-bass](research/organ-farfisa-bass.md) s7)
- [x] Farfisa key stage (`--farfisa-processor on`: key-down weights `1/48` x measured per-sum line gain, chord a peak within 0.2 dB, [organ-farfisa-bass](research/organ-farfisa-bass.md) s9)
- [x] `--firmware-routing`: an enabled section with no layer on plays layer A (OQ25, the 9 silent programs become audible)
- [ ] Farfisa processor `0x122B85` bit-exact (lattice, phase) and the B3 / B3 Bass processors `0x121E2B` / `0x122296` (scanner): bit-exact models, then Rust (inputs known, OQ18)
- [ ] Percussion path (second engine family `0xC00BF890`, `0xC00BE348`), key-up and velocity source

### 7b.4 Effects

- [x] Firmware Amp biquad sets, master 4-section IIR, compressor gain computer `0x120630` as opt-in paths (`fx/fw_*.rs`)
- [x] Routine models with opt-in Rust ports: delay `0x1206B0`, table LFO `0x1228CF`, all-pass `0x122C39`, resonant `0x1243F4` / `0x124497` ([fx-routine-models](research/fx-routine-models.md))
- [x] FX type map: 28 modules own all 100 routines; Amp type is data plus a mode word ([fx-type-map](research/fx-type-map.md)); client and context words with measured scalings ([fx-contexts](research/fx-contexts.md))
- [x] FX delay time law: `kDlyRateClkDiv` to the tap word and the oracle echo law, opt-in `FirmwareFx::delay_time` ([fx-delay-time](research/fx-delay-time.md))
- [x] FX Amp type words (`kAmpTypeNew` 1..6) decoded, the Amp drive stage and cabinet verified against the oracle and default (`FirmwareFx::amp`, Vox score 3.98 to 1.30); Leslie drive found to be `v / 127` into a null-stub row ([fx-type-map](research/fx-type-map.md) s8, s9)
- [x] Amp cabinet `0x122A1D` as a measured impulse response (`fx/fw_cabinet.rs`), unity rotary drive flag
- [x] Amp parity re-check on the B3 / Vox clips: Twin, JC, Small drive + cabinet oracle exact (cabinet fingerprint test), Vox residual -7.5 / -9.7 dB shown not to be an amp error, B3 amp-independent ([fx-type-map](research/fx-type-map.md) s12)
- [x] FX Amp EQ: tone stack `0x1243F4` with the `kAmpTreble` / `kAmpBass` / `kAmpMid` / `kAmpMidFreq` words wired behind `FirmwareFx::eq` (D36, [fx-eq-words](research/fx-eq-words.md)); gain law 0.25 dB per count (cut side) VERIFIED, `0x2f730c` is not the EQ, `kCmpResponse` re-checked closed; open: joint mid law capture (`tools/fx_eq_capture.py`), organ layout, synth words, hardware clip
- [ ] Organ unit EQ routines (`0x124344`, `0x124270`), routine `0x122B1D`, cabinet bank transliteration, JC / Small recordings; Leslie rotor: processor `0x1256AD` ported word for word (`fx::FwLeslieE40`, opt-in `--leslie firmware`; [fx-leslie](research/fx-leslie.md) s8); the level hand-over is the ring advance of the pass routine (`RingAdvance`, word for word with the oracle, [fx-leslie](research/fx-leslie.md) s10; B3 clip score 3.13 against 7.55 of the engine rotary, default flip held by D22: presence -6.0 dB); open: ramp time, hardware `DM(0xC00DC)`
- [x] FX reverb amount law (`kRvbAmt`, equal power) wired behind `FirmwareFx::reverb_mix`; `kCmpResponse` closed ([fx-reverb-leslie](research/fx-reverb-leslie.md)); the reverb of types 0 and 1 (wrapper `0x12417E` / `0x12435C` plus core `0x123F61`) is ported to Rust from the instruction model by `tools/fx_rvbcore_gen.py` and is bit exact against the oracle on 3000-call impulse and noise runs (`FwReverb`, opt-in `FirmwareFx::reverb`, `ns4 --firmware-reverb`; oracle responses T60 4.3 s and 5.4 s); types 2..11 (wrapper `0x120FDB` plus packed core `0x120D8A`): instance found static in the image, model generated from the listing (`tools/fx_rvbmulti_gen.py`, 9,660 of 9,660 traced steps per type) and ported to Rust (`FwReverbMulti`, opt-in `FirmwareFx::reverb` for Booth, Room, Stage variation, Hall, Cathedral), bit exact against the 18 oracle golden cases ([fx-reverb-leslie](research/fx-reverb-leslie.md) s13, s14); hardware comparison, type 6 and the organ rows open; Leslie ported separately
- [x] FX Mod 2 CHOR (`kFx2Type` 4, 5: `0x122E24`, `0x122F77`, oracle verified 500/500) ported and wired behind `FirmwareFx::mod2` ([fx-mod2-types](research/fx-mod2-types.md), D32); open: ENS, SPIN, wiring FLANG / PHAS
- [x] FX Mod 2 FLANG (`kFx2Type` 2, 3) and PHAS 0 (`kFx2Type` 0) wired behind `FirmwareFx::mod2` through the law stage (`fx/fw_mod2_law_stage.rs`, instance rows, type words and 11 sample laws as derived data); ENS (`0x123A7B`) and SPIN (`0x123557`) models verified 500/500 twice against the oracle and ported (`fw_ens.rs`, `fw_spin.rs`), delay words rescaled to the engine rate ([fx-mod2-types](research/fx-mod2-types.md) s8, D40)
- [ ] FX Mod 2: wire ENS, SPIN and PHAS 1 (instance words missing, s8.3); Mod 1 RM (`0x1239D0`), A-WAH (`0x123E9B`), PUMP (`0x124374` / `0x122D2C`) not started
- [x] FX Mod 2 VIBE (`kFx2Type` 6, 7: `0x123C84`, oracle verified 500/500 twice) ported and wired behind `FirmwareFx::mod2`; FLANG (`0x12348D`), PHAS 0 (`0x12379B`) and PHAS 1 (`0x1238FC`) verified and ported as routines; generators are committed tools (`tools/fx_listing_gen.py`, `tools/fx_mod2_rust_gen.py`, [fx-mod2-types](research/fx-mod2-types.md) s7, D37)
- [x] FX Mod 1 A-PAN (`0x122D93`, oracle verified) and TREM (`0x122C39` at mix 0) ported and wired behind `FirmwareFx::mod1` ([fx-mod-types](research/fx-mod-types.md), D26); open: RM, A-WAH, WAH, PUMP, the Mod 2 types, the delay feedback-effect types, tempo sync and loop period
- [ ] FX wiring: connect the ported routines to `fx.rs` using the measured context words (compressor amount, delay mix and feedback, Amp drive, phaser rate); the reverb tables (`ns4-ca0i`); the delay time law (`kDlyRateClkDiv`, [fx-delay-time](research/fx-delay-time.md)) is decoded and wired behind `FirmwareFx::delay_time`
- [ ] Enable gating and the command consumer ([fx-contexts](research/fx-contexts.md) s7; wiring plan s8), then sweep the four type ids with the effects enabled

### 7b.5 Piano

- [x] SMP 6-tap Q31 kernel (bit exact, default), SMP voice loop bit-exact on real `.npno` pages ([smp-voice](research/smp-voice.md), [smp-kernel-fit](research/smp-kernel-fit.md))
- [x] Pitch law helper `piano/step_q32.rs` (1,201-entry cents table VERIFIED against ARM `0xC02A6168`) and level law helper `piano/level_q23.rs` (61-entry table VERIFIED against ARM `0xC02A1A08`) ([piano-dsp-path](research/piano-dsp-path.md) s7)
- [ ] Wire the pitch (cents) and level (60 steps per octave) laws into `PianoLayer`
- [ ] Piano velocity law: the call-frame word `X` of the level procedure is not traced (needs the key-down capture); the engine keeps `VelocityTouch` power laws until then
- [ ] Edge handling of the sample stream and the step limit of one decoded sample per frame

### 7b.6 Synth

- [x] Firmware saw `0x12501F` and SY gain de-zipper as opt-in (`synth/hw_saw.rs`, `synth/gain_dezip.rs`, [synth-voice-control](research/synth-voice-control.md)); sinc kernels within 1 ulp ([oracle-vs-engine](research/oracle-vs-engine.md), [sy-smp-audio](research/sy-smp-audio.md))
- [x] Core2 filter stage `0x12566E` (two 4-pole sections, table-interpolated coefficients) reproduced bit-exact in Python ([synth-stage-equations](research/synth-stage-equations.md), `tools/synth_filter_model.py`)
- [x] Firmware filter family `0x12566E` / `0x125913` / `0x125A50` (LP4, BP4, HP4) and the envelope rate tables as opt-in Rust (`synth/fw_filter.rs`, `synth/fw_env_rates.rs`, `SynthLayer::filter_engine`, spec [synth-engine](specs/synth-engine.md) 4.1); defaults and `render_hash` unchanged
- [x] Firmware strip chain as one opt-in switch (`SynthLayer::chain = FirmwareStrip`: firmware saw, ADR record chain with the rate tables, strip word 3 cutoff law, firmware filter, gain de-zipper; `synth/strip_path.rs`, `strip_map.rs`, `render-patch --synth-chain strip`, spec [synth-engine](specs/synth-engine.md) 4.2); stage words checked against the oracle (`tools/synth_strip_e2e.py`), Smooth Lead presence gap -16.5 -> -9.0 dB ([synth-strip-path](research/synth-strip-path.md))
- [x] Synth key-down velocity series `g(vel)` (VERIFIED table, EMPIRICAL S-curve) wired into the strip chain voice level (`synth/key_velocity.rs`, `tools/synth_velocity.py`); Wave edit ids probed in the ARM harness: they do not change strip word 0 (`tools/synth_wave_probe.py`, [synth-strip-path](research/synth-strip-path.md) s6); strip default stays off (presence -9.0 dB, RMS +3.8 dB)
- [x] DSoundman course clips, synth side: LFO rate (OLED Hz = real Hz within 2 %, table VERIFIED), shape order (VERIFIED), filter-LFO depth bound (EMPIRICAL: hardware much deeper than the emulator, octave scale open) and the piano + pad balance (within 1 dB, linear synth layer level favoured) in [dsoundman-synth-lfo](research/dsoundman-synth-lfo.md) and [dsoundman-piano-pad-layer](research/dsoundman-piano-pad-layer.md); no emulator default changes, tests `synth::tests_osc_lfo::lfo_*`; pending clips in `emu_vs_hw_segments.json` (`pending_segments`, spec only)
- [x] ANALOG wave to strip stage table (45 waves, 34 stage routines, `data/synth-wave-types.csv`, [synth-wave-stages](research/synth-wave-stages.md)); Smooth Lead SHAPE wave 0 = stage `0x1212D3`
- [x] Stage `0x1212D3` (ANALOG SHAPE) bit exact in Python (40-bit registers, 320 of 320 oracle words), Rust `synth/fw_shape.rs`, opt-in `strip_shape` ([synth-wave-models](research/synth-wave-models.md))
- [x] Saw `0x1215FB` and square / pulse `0x121B99` stages bit exact in Python (320 of 320 oracle words each); preset category / wave == edit-id domain verified on 578 factory layers
- [x] Sine `0x120CD0`, triangle `0x121183`, Shape Saw `0x12169D` stages bit exact in Python (320 of 320 oracle words each)
- [x] Empirical oscillator P block for the strip chain (`synth/p_block.rs`, `--synth-p fitted`, `tools/synth_p_fit.py`): fit to the Smooth Lead clip with a half-clip hold-out, Pure Saw / Pure Square harmonic checks; Smooth Lead presence -9.0 -> -0.8 dB, RMS +3.8 -> +0.2 dB; default stays `Legacy` (one program, [synth-p-fit](research/synth-p-fit.md))
- [ ] Remaining stage models ( then SAW / SQUARE / TRIANGLE / SUB / SYNC stages) bit exact in Python, then opt-in Rust; synth parity remainder: `0x1257B2`, the knob to strip word maps (ASSUMED), `kFltType` and `osc_wave` to stage selection, then flip the defaults and re-baseline `render_hash` (TD-11)
- [x] 12 dB low pass `0x1253BD` (Lp12, 218 layers) and SHAPE SINE wave 0 `0x123427` bit exact in Python and opt-in Rust (`FwFilterType::Lp2`, `FwStage::ShapeSine`); `kFltType` = table index (filter off = 6), [synth-filter-type-map](research/synth-filter-type-map.md), [synth-stage-models-2](research/synth-stage-models-2.md): 80 % of ANALOG layers, 85 % of filter layers modelled
- [x] SHAPE SINE waves 1, 2, 5 (`0x1235C9`, `0x1232A5`, `0x122F1F`, 15 layers) bit exact in Python and opt-in Rust (`synth/fw_sine_warps.rs`): 85 % of ANALOG layers modelled ([synth-stage-models-3](research/synth-stage-models-3.md))
- [x] SY sample voice kernel rule: the ARM picks the 12-tap kernel from the voice key (`<= 47`, then every 12 keys; `0xC0001BAC` is a static channel record), the firmware compare chain run for keys 0..127 (`tools/arm_sy_kernel_rule.py`), Rust `sinc/key_rule.rs` (default since D46, `--sample-kernel ratio` for the old rule), sweep tags `synth.sample.kernel=K` ([synth-sample-kernel-rule](research/synth-sample-kernel-rule.md), TD-08)
- [x] SY sample voice pitch law: ARM exponent block `0xC01E14AC..0xC01E15A4` run (`tools/arm_sy_pitch_law.py`), Rust `sinc/pitch_law.rs` bit-identical for 23 inputs behind `--sample-pitch fw`; WAVE bank shows no SY / SMP message in the harness ([synth-sample-kernel-rule](research/synth-sample-kernel-rule.md) sections 7 to 9, D35). The SAMPLES voice update is run in section 12 (D46, D47); open: ok" and binds the layer, [section 11](research/synth-sample-kernel-rule.md); voice groups / ring consumer not run; D45, D38), step scale, loop handling
- [x] SAMPLES voice update on the ARM side: the voice update `0xC01E00AC` runs end to end in the harness (`tools/arm_sy_voice.py`: queue pump plus the DSP status bit), the voice key store `0xC01E0A00` and the record write `0x9243B` captured for nine keys, kernel default flipped and `render_hash` re-baselined ([synth-sample-kernel-rule](research/synth-sample-kernel-rule.md) section 12, D46, D47); the ARM pitch word `0x921A6` is bit-exact with `sinc::pitch_law` on nine keys
- [ ] SAMPLES remainder: source of the per-zone key / tune fields in the sample map, the DSP step join of the pitch word (then `--sample-pitch fw` default), the voice stores with Octave Shift / transpose, loop handling and the record words `0x9240A..0x92413`, SY / SMP split of the WAVE bank
- [x] SHAPE SINE wave 4 (`0x120DAB`, 4 layers incl. its output high pass) bit exact in Python and opt-in Rust (`synth/fw_sine_hp.rs`): 86 % of ANALOG layers modelled (269 of 312, [synth-stage-models-4](research/synth-stage-models-4.md))
- [x] SUPER wave 1 (`0x121E9D`), SHAPE SINE wave 3 (`0x12377D`) and SYNC wave 2 (`0x122A32`, with its 258-word interpolation table) bit exact in Python (320 of 320 oracle words each) and opt-in Rust (`synth/fw_super1.rs`, `fw_sine3.rs`, `fw_sync.rs`): 90 % of ANALOG layers modelled (281 of 312, [synth-stage-models-5](research/synth-stage-models-5.md))
- [x] SYNC waves 3..5 (`0x122C5C`) bit exact in Python (320 of 320 oracle words, 960 of 960 over 120 trials) and opt-in Rust (`synth/fw_sync3.rs`, `FwStage::Sync3`); the SYNC 2 interpolation is verified at varying `s5` (the pass source word `DM(0xC271A)` is poked): 91.7 % of ANALOG layers modelled (286 of 312, [synth-stage-models-6](research/synth-stage-models-6.md))
- [x] SYNC waves 1, 6, 7, 8 (`0x1226DD`, `0x1239BE`, `0x123C7C`, `0x123FE9`) bit exact in Python (320 of 320 oracle words each) and opt-in Rust (`synth/fw_sync1.rs`, `fw_sync6.rs`, `FwStage::Sync1 / Sync6 / Sync7 / Sync8`); the oracle pass now varies the pitch (`DM(0xC2716)`) for all SYNC stages and `s5` for the SHAPE SINE stages: 93.9 % of ANALOG layers modelled (293 of 312, [synth-stage-models-7](research/synth-stage-models-7.md))
- [x] MISC waves 0, 1, 2 (`0x122030`, `0x122151`, `0x122417`) and SUB OSC (`0x12179A`) bit exact in Python (320 of 320 oracle words each) and opt-in Rust (`synth/fw_misc.rs`, `fw_sub.rs`, `FwStage::MiscPink / MiscLowpass / MiscFm / SubOsc`): 96.8 % of ANALOG layers modelled (302 of 312, [synth-stage-models-8](research/synth-stage-models-8.md)); SUPER 2..4 and SHAPE 3 / 4 remain
- [x] Ladder24 filter stage `0x12549D` (`kFltType` 2, 68 filter layers) bit exact in Python (1840 of 1840 oracle words over 40 trials, both lanes; the stage computes its own coefficients one call ahead from the cutoff position and resonance words) and opt-in Rust (`synth/fw_ladder.rs`, `FwFilterType::Ladder`, wired for `Ladder24` in the strip chain behind `--synth-stages`): 442 of 442 filter layers have a verified stage; the strip word to cutoff position law for type 2 is ASSUMED ([synth-stage-models-9](research/synth-stage-models-9.md), D50); `fw_stages.rs` split into a directory module
- [ ] Remaining stage models (SUPER 2..4, SHAPE 3 / 4, SAMPLE / WAVE banks) bit exact in Python, then opt-in Rust; synth parity remainder: `0x1257B2`, the knob to strip word maps (ASSUMED), `kFltType` and `osc_wave` to stage selection, then flip the defaults and re-baseline `render_hash` (TD-11)
- [x] Synth default flip (D22): strip chain + stage oscillators + EMPIRICAL per-wave P table (`synth/p_table.rs`, `--synth-p table`) validated on 15 hardware clips (`tools/synth_validate.py`, [synth-validation](research/synth-validation.md)); `render_hash` re-baselined; program sweep clean (0 NaN, 0 newly silent, clipping 18 to 9 programs)
- [ ] Pulse and triangle shapes, amp envelope smoothing
- [ ] Hardware sweeps for the remaining ASSUMED scales ([synth-hardware-sweep-plan](research/synth-hardware-sweep-plan.md), OQ7)

---

## Parallel Lanes & Bean Dependency Graph

| Lane | Active Beans | Blocked By | Scope |
|---|---|---|---|
| **Lane A — Format & Preset Bitmaps** | `ns4-2ih2` | `ns4-3387` (done) | Decode all bit offsets in `.ns4o`, `.ns4n`, `.ns4y`, `.ns4p` + JSON patch converter |
| **Lane B — Organ, Piano & Synth DSP** | `ns4-yaip` (done) | `ns4-3387` (done) | Complete Vox/Farfisa/Pipe/Scanner, Wave 2 SuperSaw/Wavetable/Arp, SFZ loader, FDN Reverb (firmware parity continues in Lane E) |
| **Lane C — 1:1 Panel GUI, Standalone & DAW Plugin** | `ns4-707f` | `ns4-3387` (done) | `crates/nord-plugin` (`nih-plug` CLAP + VST3 + Standalone) & 1:1 front-panel UI ([spec](specs/visual-style-and-panel.md)) |
| **Lane C2 — Phase 6b Hardware Parity & 2.5D Panel** | `ns4-xc5n` | `ns4-707f` | Control inventory, `crates/nord-panel`, baked-sprite renderer, parameter parity, plugin editor ([control inventory](specs/control-inventory.md)) |
| **Lane D — Track B Firmware & `CNSP` Codec** | `ns4-trkb` | `ns4-3387` (done) | Full custom LZ firmware unpacker + `CNSP` (`.npno` / `.nsmp4`) sample codec decoder |
| **Lane E — Track B Oracle & Firmware Parity (Phase 7b)** | `ns4-36hw`, `ns4-doy1`, `ns4-vnp2`, `ns4-qll9`, `ns4-ca0i`, `ns4-af6y`, `ns4-wzyz` | `ns4-trkb` | SHARC oracle, ARM harness, key-down capture, FX wiring, Vox / Farfisa / B3 Rust paths, synth filter and envelope parity, simulator probes |


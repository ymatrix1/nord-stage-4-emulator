# Roadmap

Don't start an unapproved phase without user confirmation. Tick checkboxes (`- [x]`) in the PR or commit that delivers them alongside unit tests.

## Phase 0: Reverse Engineering, Architecture & Specifications

- [x] Dissect `Nord Stage 4 Factory Restore RevK v1.66.ns4b` archive (`meta.xml`, `product_id=46`, build `2934`, file census) ([research](research/ns4-backup-and-cbin-teardown.md))
- [x] Reverse-engineer the 48-byte `CBIN` v1 header and bitstream embedding across `.ns4p` (868 B), `.ns4y` (541 B), `.ns4o` (183 B), `.ns4n` (195 B), `.ns4l` (868 B), `.ns4t` (124 B) ([spec](specs/cbin-and-presets.md))
- [x] Extract and map the OS v1.66 firmware payload (`RCDATA 101`, 4,037,080 B) into `ARM0` host MCU and `DSPO` / `DSPS` / `DSPF` SHARC DSP partitions ([research](research/ns4-firmware-teardown.md))
- [x] Document hardware DAC & reconstruction stage (`CS4392` DAC + `LM833`/`LF353` output filter) ([research](research/hardware-and-dac-analysis.md))
- [x] Author complete subsystem specifications in `docs/specs/*.md` (`cbin-and-presets`, `organ-engine`, `synth-engine`, `piano-engine`, `effects-and-routing`, `morph-and-zones`, `midi-and-control`, `verification`)
- [x] Establish repository scaffolding, `AGENTS.md`, `CLAUDE.md`, `.agents/rules/`, `.agents/skills/`, `.beans/`, and GitHub Actions CI/CD (`ci.yml`, `release.yml`)

## Phase 1: Interoperability & Core Foundation

- [x] `crates/nord-core/src/cbin.rs`: 48-byte `CBIN` header parser, serializer, schema validator (`0x0139`, `0x00D0`, `0x00CD`, `0x00CB`, `0x006C`), bit-stream reader, and `.ns4b` `meta.xml` parser
- [x] `crates/nord-core/src/firmware.rs`: OS update container header and `SRAM` / `CODE` / `ARM0` / `DSPO` / `DSPS` / `DSPF` partition table parser
- [x] `tools/ns4_inspect.py`: Python CLI and library for inspecting `.ns4b` archives, `.ns4*` presets, and firmware images with JSON output
- [x] `tools/check_tests.py`: Per-file unit-test enforcement ratchet across Rust and Python (`tools/test-baseline.txt` at 0 uncovered files)
- [ ] Full bit-field parameter decoding table for all 868 bytes of `.ns4p` (Scene I/II, FX parameters, Morph slots)
- [ ] JSON / TOML human-readable patch export and re-import CLI (`ns4-cli convert`)

## Phase 2: Organ Engine DSP (`DSPO` High-Level Emulation)

- [x] `crates/nord-core/src/organ.rs`: B3 91-tonewheel frequency & phase generator with exact Hammond gear ratios, 9-drawbar (`16'` to `1'`) harmonic bus mixer, 2nd/3rd harmonic percussion envelope, and Model 122 Rotary Speaker crossover/Doppler core
- [ ] Crosstalk & tonewheel transformer leakage matrix + key-click transient burst generator
- [ ] C1–C3 / V1–V3 phase-shift scanner vibrato delay line
- [ ] Vox Continental (`Vox`) and Farfisa Compact (`Farf`) transistor organ oscillator & filter banks
- [ ] Pipe Organ 1 & 2 voicing models

## Phase 3: Synth Engine DSP (`DSPS` Nord Wave 2 Derived)

- [x] `crates/nord-core/src/synth.rs`: Anti-aliased PolyBLEP Virtual Analog oscillator (Saw, Pulse/PWM, Triangle, Sine) and Zero-Delay Feedback (ZDF) 12/24 dB Transistor Ladder filter (LP/BP/HP) + ADR envelope
- [ ] Multi-oscillator configurations: Classic, Wave (64 wavetables), FM (2/3-operator harmonic ratios), and Sample playback (SFZ/WAV)
- [ ] Unison detune/stereo spread, LFO modulation routing, and Clock-synced Arpeggiator
- [ ] 3-layer (`Synth A`, `Synth B`, `Synth C`) 46-voice polyphonic voice allocator

## Phase 4: Piano Engine DSP (`DSPF` / Sample + Modal Resonance)

- [x] `crates/nord-core/src/piano.rs`: Multisample voice state, velocity curve mapper (`Heavy`, `Medium`, `Light`), Timbre EQ presets (`Soft`, `Mid`, `Bright`, `Dyno1`, `Dyno2`), and modal Sympathetic String Resonance resonator bank
- [ ] Open SFZ / WAV multi-velocity & release-sample loader
- [ ] Dynamic Pedal Noise generator (sustain CC64 & una corda CC67 velocity-sensitive mechanical transient)
- [ ] 120-voice polyphony manager with pedal-aware voice stealing

## Phase 5: 7-Layer Routing, Morphing Matrix & Per-Layer FX

- [x] `crates/nord-core/src/morph.rs`: 3-source Morphing Matrix (`ModWheel`, `ControlPedal`, `Aftertouch`), Layer Scene I/II switcher, and 4-zone Keybed Split router with `Off` / `Small` (6 st) / `Large` (12 st) crossfade curves
- [x] `crates/nord-core/src/fx.rs`: Per-layer 5-slot FX chain (`Mod 1`, `Mod 2`, `Amp/Drive/EQ`, `Delay`, `Reverb`) + downstream Master Compressor & Master Clock
- [x] `crates/nord-core/src/midi.rs`: Complete Nord Stage 4 MIDI CC decoder/encoder and Program Change handler
- [x] `crates/nord-core/src/engine.rs`: Integrated 7-layer multitimbral `Stage4Engine` rendering Organ A/B, Piano A/B, and Synth A/B/C through 7 independent FX racks into the Master bus

## Phase 6: Real-Time Host, Standalone UI, WebAssembly & CLAP/VST3

- [ ] Real-time audio + MIDI host (`cpal` / WASAPI exclusive / CoreAudio / ALSA)
- [ ] CLAP & VST3 plugin wrapper (`nih-plug`)
- [ ] Interactive stage front-panel UI (7-layer OLED displays, LED-ring drawbars/knobs, split-zone bar, Scene II button)
- [ ] WebAssembly + WebAudio / WebMIDI browser build

---

## Parallel Lanes

| Lane | Focus | Dependencies |
|---|---|---|
| **Lane A — Format & Tooling** | `.ns4p`/`.ns4y`/`.ns4o`/`.ns4n` bit-field maps, JSON patch converter | Phase 1 core (`cbin.rs`) |
| **Lane B — DSP Engines** | Organ extensions (Vox/Farfisa/Scanner), Synth Wavetable/FM, Piano SFZ loader | Phase 1 core |
| **Lane C — Host & Plugin** | Real-time `cpal` standalone, CLAP/VST3 wrapper, front-panel UI | Phase 5 `Stage4Engine` |

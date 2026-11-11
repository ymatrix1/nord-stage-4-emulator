---
# ns4-707f
title: 'Phase 6: 1:1 Front-Panel GUI, Standalone App & CLAP/VST3 DAW Plugin (nih-plug)'
status: completed
type: epic
created_at: 2026-11-11T00:47:46Z
updated_at: 2026-11-11T17:29:00Z
---

## Context
Blocked by: none
Deliver dual-target Standalone and CLAP/VST3 DAW plugin (`D11`) wrapping `nord_core::engine::Stage4Engine`, with a 1:1 visual recreation of the Nord Stage 4 front panel (`docs/specs/visual-style-and-panel.md`, derived from `foto.jpg`, `NS4-Panel-organ.max-2000x2000.format-png.png`, and `Screenshot20at2015.jpg`).

## Higher Goal
Allow live gigging and DAW hosting from a single Rust codebase with an authentic hardware control surface.

## Acceptance Criteria
- [x] Add `crates/nord-plugin` with `nih-plug` CLAP + VST3 + Standalone targets
- [x] Map 7-layer parameters, drawbars, split zones, Layer Scene II, and Morph matrix to host automation
- [x] Implement 1:1 Front-Panel GUI (`docs/specs/visual-style-and-panel.md`):
  - [x] Bay 1: Left Master & Rotary (`#8E1B1F`) with Pitch Stick, Mod Wheel, Rotary Drive & Slow/Fast
  - [x] Bay 2: Organ Section (`#323D47`) with dual faders A/B + 11-LED green ladders & 9 Drawbars + 8-LED red ladders
  - [x] Bay 3: Piano Section (`#323D47`) with dual faders A/B + 11-LED green ladders, Acoustics, Timbre, Piano Select triangle matrix
  - [x] Bay 4: Central Program & Morph Section (`#8E1B1F`) with Central Program OLED (`A:11`), Morph Assign, Split, Layer Scene II
  - [x] Bay 5: Synth Section (`#323D47`) with triple faders A/B/C + 11-LED green ladders, Synth Waveform OLED, Filter & ADR
  - [x] Bay 6: Layer Effects Section (`#323D47`) with FX Focus strip, Mod 1, Mod 2, Amp Sim/EQ, Delay, Comp, Reverb
  - [x] Bottom 88-Key Interactive Keybed with 3 Split-Point LEDs & crossfade zone visualization

## Out of Scope
- Proprietary `.npno` audio decryption (tracked in `ns4-trkb` / archived `ns4-cnsp`)

## Summary of Changes
- Added `crates/nord-plugin` (`Cargo.toml`, `src/lib.rs`, `src/params.rs`, `src/main.rs`) wrapping `Stage4Engine` for CLAP, VST3, and Standalone execution with zero-allocation sample-accurate MIDI processing and host parameter synchronization (`Stage4Params`).
- Added `crates/xtask` (`nih_plug_xtask`) + `.cargo/config.toml` alias to bundle `.clap` and `.vst3` artifacts across Windows, Linux, and macOS.
- Updated `.github/workflows/ci.yml` (added `macos-latest` matrix + `cargo xtask bundle nord-plugin`) and `.github/workflows/release.yml` (packaged `nord-plugin-standalone`, `.clap`, and `.vst3` bundles in release archives).

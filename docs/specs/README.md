# Technical Specifications (`docs/specs/`)

Status: v1.0 · Synced with `main`

Every subsystem of the Nord Stage 4 Emulator has a dedicated specification document in this directory. Changes to subsystem behavior or public interfaces MUST update the corresponding spec in the same PR/commit.

## Subsystem Index

| Specification | Scope | Core Implementation |
|---|---|---|
| [cbin-and-presets.md](cbin-and-presets.md) | 48-byte `CBIN` v1 header, `.ns4b` backup archive, `.ns4p`/`.ns4y`/`.ns4o`/`.ns4n`/`.ns4l`/`.ns4t` formats, OS v1.66 firmware container | `crates/nord-core/src/cbin.rs`, `crates/nord-core/src/firmware.rs` |
| [organ-engine.md](organ-engine.md) | 2-layer Organ DSP: B3 91-tonewheel physical model, harmonic buses, keyclick, 2nd/3rd percussion, scanner vibrato, Vox/Farfisa/Pipe, Rotary 122 | `crates/nord-core/src/organ.rs` |
| [synth-engine.md](synth-engine.md) | 3-layer Nord Wave 2 Synth DSP: PolyBLEP VA, Wavetable, FM, SFZ/WAV samples, 12/24 dB ZDF Ladder filters, ADR envelopes, LFO, Arpeggiator | `crates/nord-core/src/synth.rs` |
| [piano-engine.md](piano-engine.md) | 2-layer Piano DSP: 120-voice multisample playback, velocity curves, Timbre EQ, modal Sympathetic String Resonance, Dynamic Pedal Noise | `crates/nord-core/src/piano.rs` |
| [effects-and-routing.md](effects-and-routing.md) | 7 independent per-layer FX pipelines (`Mod 1`, `Mod 2`, `Amp/Drive/EQ`, `Delay`, `Reverb`), Master Compressor, Master Clock | `crates/nord-core/src/fx.rs`, `crates/nord-core/src/engine.rs` |
| [morph-and-zones.md](morph-and-zones.md) | 4-zone keybed split router, `Off`/`Small`/`Large` crossfades, Layer Scene II, Triple-Sensor velocity, 3-source Morphing Matrix | `crates/nord-core/src/morph.rs` |
| [midi-and-control.md](midi-and-control.md) | Complete MIDI CC implementation table, Bank Select / Program Change, and panel parameter mapping | `crates/nord-core/src/midi.rs` |
| [visual-style-and-panel.md](visual-style-and-panel.md) | 1:1 Front-Panel UI layout (`#8E1B1F` Crimson + `#323D47` Slate), drawbar/fader LED ladders, dual OLEDs, Morph arcs | `crates/nord-plugin` (Phase 6) |
| [verification.md](verification.md) | CI pipeline, `tools/check_tests.py` coverage ratchet, deterministic offline audio verification, round-trip CBIN tests | `tools/check_tests.py`, `.github/workflows/ci.yml` |
| [sharc-core.md](sharc-core.md) | Track B: ADSP-214xx core model used as a test oracle for single routines of the firmware's DSP images (scope from the instruction census, milestones, risks) | `crates/nord-sharc` (M1: VISA decoder and `sharc-disasm`); the core itself is not implemented yet |

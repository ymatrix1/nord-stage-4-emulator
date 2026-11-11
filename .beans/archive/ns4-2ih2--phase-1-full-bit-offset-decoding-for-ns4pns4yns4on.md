---
# ns4-2ih2
title: 'Phase 1: Full Bit-Offset Decoding for .ns4p/.ns4y/.ns4o/.ns4n & JSON Patch Converter'
status: completed
type: task
priority: normal
created_at: 2026-11-11T00:54:47Z
updated_at: 2026-11-11T01:17:22Z
---

## Context
Blocked by: none
Core 48-byte CBIN header and sub-preset slice extraction implemented in `crates/nord-core/src/cbin.rs`.

## Higher Goal
Achieve full bit-level decoding of Nord Stage 4 programs and sub-presets, enabling lossless JSON round-tripping and instant parameter application into `Stage4Engine`.

## Acceptance Criteria
- [x] Implement big-endian sequential `BitReader` and `BitWriter` in `crates/nord-core/src/cbin.rs`
- [x] Map bit offsets inside the 135-byte `.ns4o` Organ payload (`model`, `drawbars_a/b`, `percussion`, `vibrato`, `rotary`)
- [x] Map bit offsets inside the 147-byte `.ns4n` Piano payload (`category`, `timbre`, `touch`, `string_res`, `pedal_noise`, `dyn_comp`, `unison`)
- [x] Map bit offsets inside the 493-byte `.ns4y` Synth payload (`osc_mode`, `waveform`, `detune`, `filter_model`, `cutoff`, `resonance`, `ADR`, `LFO`, `arp`)
- [x] Map global 45-byte `.ns4p` header (`tempo_bpm`, `master_compressor`, `transpose`, `split_points`, `split_crossfades`, `scenes I/II`)
- [x] Add high-level `ProgramDecoded` struct with pretty JSON export and import (`to_json()`, `from_json()`)
- [x] Add `Stage4Engine::apply_program()` to wire decoded program states directly into the 7-layer real-time DSP engine
- [x] Add `ns4 convert <input> [--to-json | --to-ns4p]` and `ns4 inspect <file> [--json]` to CLI
- [x] Add `--decode` support to `tools/ns4_inspect.py`
- [x] Add unit tests for bit reading/writing, all preset unpack/pack roundtrips, CLI conversion, and engine application

## Out of Scope
- Decrypting proprietary `.npno` audio samples (external SFZ/WAV samples used instead)

## Summary of Changes
- Added `BitReader` and `BitWriter` primitives for arbitrary bit-length reading/writing in `crates/nord-core/src/cbin.rs`.
- Implemented `OrganPresetDecoded`, `PianoPresetDecoded`, `SynthPresetDecoded`, `GlobalProgramDecoded`, and `ProgramDecoded` with full `serde` support.
- Implemented `CbinFile::decode_program`, `decode_organ_preset`, `decode_piano_preset`, `decode_synth_preset` and `ProgramDecoded::to_cbin_file`.
- Implemented `Stage4Engine::apply_program` to load decoded programs into real-time parameters.
- Extended `ns4` CLI with `convert` command for bidirectional `.ns4p` <-> JSON conversion and `--json` inspect flag.
- Extended Python `tools/ns4_inspect.py` with `BitReader` and `--decode` flag.
- Added comprehensive unit tests in Rust and Python.

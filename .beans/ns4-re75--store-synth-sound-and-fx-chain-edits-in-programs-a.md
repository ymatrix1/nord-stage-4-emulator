---
# ns4-re75
title: Store Synth sound and FX chain edits in programs and presets (OQ30)
status: completed
type: task
priority: normal
created_at: 2026-11-14T21:53:00Z
updated_at: 2026-11-14T21:55:03Z
parent: ns4-xc5n
---


## Context
STORE of a program or a section preset patched the panel state onto the base the section was loaded from, but the Synth sound (oscillator, filter, envelopes, LFO, vibrato, arpeggiator, voice mode) and the effect chains came from the base, so panel edits to them were not stored (OQ30, left open by `ns4-rt23`, PR #327). The program encoder also re-encoded without a bit merge, so 224 of 384 factory programs did not come back byte for byte (OQ31 collapsed bits).

## Higher Goal
A stored program or preset carries the user's edits of the sound, and every bit the engine cannot represent stays bit-identical to the base (AGENTS.md rule 5).

## Acceptance Criteria
- [x] `synth_params_export`: `invert_decoded_layer` (inverse of `apply_decoded_layer`) and `export_decoded_layer` (writes a field only where the engine differs from what the base produces); lossy fields listed in `LOSSY_FIELDS`
- [x] `synth_params_arp::invert_arp_modes` / `sync_arp_modes`: run switch, mode, direction, zig zag, KB sync, Master Clock, RANGE, rate
- [x] `engine/program_fx_export`: `invert_fx_chain` (inverse of `apply_fx_chain`) for all three morph-bearing chains of Organ, Piano and Synth; lossy fields listed in `LOSSY_FX_FIELDS`
- [x] `ProgramState::export_into` (program, Live slot, section preset) calls both; `ProgramDecoded::to_cbin_file_like` merges at bit level like `SectionPreset::to_file_like`
- [x] Property tests: random decoder payloads (synthetic, in memory) round trip `pack(inverse(apply(x))) == pack(x)` for Synth layers and FX blocks; unedited layer unchanged
- [x] Factory check: `ns4 preset-check` now also runs the inverse on all 384 Synth presets (1152 layers, 1152 chains) and the unedited STORE byte round trip of 544 presets and 384 programs, plus one cutoff / reverb edit each; 0 problems
- [x] `ns4 store-program --layer --cutoff --resonance --reverb-mix`
- [x] Specs (`cbin-and-presets.md` 5.7), D58, OQ30 narrowed, ROADMAP

## Out of Scope
Pattern words (gate codes `E q Q .`), the oscillator category / wave and sample id, Mod 1 clock sync, the amp filter resonance, the morph bases, the meaning of the collapsed bits (OQ31), the header category (OQ26), hardware acceptance of an edited backup (needs a Stage 4), GUI panel files (`bay_*`, `panel/ctl.rs`).

## Summary of Changes
- `nord-core`: `synth_params_export.rs`, `synth_params_arp.rs` (inverse), `synth_params.rs` helpers (`pulse_base_width`, `glide_knob`, `vibrato_amount_from_cents`), `engine/program_fx_export.rs`, `engine/program_export.rs` wiring, `cbin/program_file.rs` bit merge, `program_check.rs`, `preset_cli.rs` (`check_inverse`, `check_store`, programs, 64 MB check thread), `store_cli.rs` flags, `synth/perf.rs` `ClockDivision::index`.
- Specs and decisions: `cbin-and-presets.md` 5.5 to 5.7, D58, OQ30, ROADMAP, `midi-and-control.md`.

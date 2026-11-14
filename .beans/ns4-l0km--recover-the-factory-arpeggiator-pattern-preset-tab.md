---
# ns4-l0km
title: Recover the factory arpeggiator pattern preset table
status: completed
type: task
created_at: 2026-11-14T12:46:56Z
updated_at: 2026-11-14T18:30:00Z
parent: ns4-xc5n
---

## Context

The synth Arpeggiator/Gate pattern dial (`Edt_Syn_kArpPtnPreset`, edit id `0x114`, range 0..28) has 29 values on the hardware. The first pass (PR #287) recovered the 28 table presets by running the firmware routine; four things stayed open: what dial value 0 does, the gate lengths of `E q Q` (assumed 4, 6, 8 eighths), importing the patterns that factory programs store, and the "preset edited" marker.

## Higher Goal

The pattern presets and the patterns of factory programs sound and behave like the hardware (1:1 parity).

## Acceptance Criteria

- [x] Find the handler of `0x114` in the firmware and the table it indexes (`GlueArpPtnPreset` update method `0xC010FDAC`, 28 structs at `0xC02BA860`).
- [x] Reproducible tool that runs the firmware routine (`tools/ns4_arp_presets.py`) with tests; result in `docs/research/arp-preset-table.csv`.
- [x] Cross-check against factory programs (5 presets equal factory layers exactly, 2 more in gate).
- [x] Engine: `PATTERN_PRESET_COUNT` 29 with gate and pan per preset, deterministic tests; GUI clamp, OLED value and host range 0..28 (no id removed).
- [x] Update `docs/research/synth-arp-patterns.md`, `docs/specs/synth-engine.md`, `docs/README.md`.
- [x] Dial value 0: trace the listener `0xC0102F30` and update method `0xC010FDAC` on the ARM emulator (`tools/ns4_arp_gate.py dial`); engine plays the saved custom pattern for 0 (`PerfState::plays_custom`).
- [x] Gate durations of `E`, `q`, `Q`: found the pattern compiler `0xC02424DC` and ran it (`tools/ns4_arp_gate.py gates`); engine rule `E` 4, `q` 8, `Q` 8 + tie, `.` 0 with 0 differences over the 29 presets and 1360 short strings; tie playback in `synth/arp.rs`, `synth/perf.rs`.
- [x] Import the patterns stored in programs / `.ns4y` layers: `cbin/synth_pattern.rs` (lossless), `synth_params::apply_decoded_layer` to `CustomPattern::from_words`, deterministic tests on synthetic payloads (no firmware or copyrighted data committed).
- [x] Preset edited marker: gate edit on a preset copies it into the custom pattern and the dial reads 0 (`pattern_dial_value`, `select_pattern_preset`, `set_step_gate`); the role of `0x115` stays documented as unknown.
- [x] Docs and provenance tags updated (research note section 7, `synth-engine.md`, `cbin-and-presets.md`, ROADMAP, README).

## Out of Scope

Showing the edited state (dial 0) in the OLED (`panel/bay_program`, `panel/ctl.rs` belong to another lane); locating the record player that walks the compiled slots (the absolute slot = half step is EMPIRICAL); the arp mode bits of a program (Poly / Gate) and a tie editor in the GUI; hardware recordings.

## Summary of Changes

- `tools/ns4_arp_gate.py` (+ tests): replays the Glue listener and update method (dial value 0) and the pattern compiler `0xC02424DC` (gate lengths) on the ARM emulator.
- `synth/pattern_presets.rs`, `pattern.rs`, `perf.rs`, `arp.rs`: `q` is a full step (8 eighths, was 6), `Q` is 8 eighths plus a tie into the next step (no retrigger, no arpeggio advance, never across the pattern end), dial 0 plays the custom pattern, `set_step_gate` customizes a playing preset, `pattern_dial_value`, `select_pattern_preset`, `CustomPattern::from_words`; tests in `synth/tests_pattern_tie.rs`.
- `cbin/synth_pattern.rs`, `synth_preset.rs`, `synth_params.rs`: factory patterns import into the layer's custom pattern.
- Docs: research note section 7, `synth-engine.md`, `cbin-and-presets.md`, `ROADMAP.md`, `docs/README.md`. No host parameter changed (`param-ids.snapshot` untouched).

---
# ns4-4ok5
title: 'Phase 6b B13: performance and host verification'
status: in-progress
type: task
created_at: 2026-11-12T21:36:11Z
updated_at: 2026-11-12T21:36:11Z
parent: ns4-xc5n
blocked_by:
    - ns4-hud1
    - ns4-vip1
---

## Context
The panel must meet its frame budget and run in real hosts.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decision D15 in `docs/DECISIONS.md`, OQ8..OQ14 in `docs/OPEN-QUESTIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [x] Deterministic worst-case audio benchmark (`ns4 bench`, `nord_core::bench`) measured in release and compared with the PRD budget (0.8 ms): measured, **budget not met** (TD-14)
- [x] Counting-allocator tests prove zero allocations in the worst-case engine run and in plugin `process()`
- [x] Panel CPU frame time at 1x and 2x, with and without panorama, measured (`frame_cost_tests.rs`); PR #306 frames and labels add no measurable cost
- [ ] 2x-scale frame under 4 ms on the reference machine, measured: CPU side (run + tessellate) 2.6 to 3.2 ms; GPU side not measured
- [ ] Repaint at most 60 Hz and only on change (not audited)
- [x] `clap-validator` run on the built CLAP: 30 of 44 pass, 1 real crash fixed, 4 failures upstream (nih-plug), details in `docs/HOST_COMPATIBILITY.md` 7.4
- [x] Headless host-lifecycle test for the plugin (`crates/nord-plugin/tests/host_lifecycle.rs`)
- [ ] Host smoke test with evidence in Reaper and Bitwig (no DAW available; checklist in `docs/HOST_COMPATIBILITY.md` 7.7)
- [ ] VST3 validator run (not installable via cargo or pip)

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
- Added `crates/nord-core/src/bench.rs` and `ns4 bench [--blocks N] [--warmup N] [--stereo] [--breakdown]`: deterministic worst case (7 layers, 88 keys, every FX rack, master compressor, 4-output routing), per-block timing, realtime factor, output hash, breakdown by layer and FX.
- Measured on one Windows 11 machine: worst case 3.6 to 4.5 ms per 128-frame block (0.59x to 0.75x realtime), typical program 0.95 to 1.2 ms, PRD budget 0.8 ms not met; no clearly wasteful hotspot found, nothing optimised, tracked as TD-14.
- Extended `crates/nord-core/tests/no_alloc_audio_path.rs` with the worst-case run; added `crates/nord-plugin/tests/host_lifecycle.rs` (activation at 44.1 to 192 kHz, sample-accurate events, reset and re-activation, hostile events, parameter map, persisted fields, zero allocations in `process()`).
- Added `crates/nord-gui/src/panel/frame_cost_tests.rs`: CPU frame (run + tessellate) at 1x and 2x, full panel and panorama: 2.6 to 3.2 ms and 2.3 to 3.0 ms at 2x; no regression from PR #306 (baseline measured on the pre-#306 commit).
- Validated the CLAP with `clap-validator` 0.4.1 (built from source, nothing committed): found and fixed an audio-thread panic on CLAP wildcard note events (note 255 indexed the 128-entry hold table) in `engine/midi_dispatch.rs` and `nord-plugin/src/midi.rs`, with regression tests; 4 remaining failures are upstream nih-plug behaviour.
- `docs/HOST_COMPATIBILITY.md` section 7 records what was and was not verified with command lines, and the manual Reaper/Bitwig smoke checklist; ROADMAP B13 stays open.

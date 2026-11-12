---
# ns4-fx5e
title: 'Phase 5: Delay Analog/Ping-Pong/Filters/Variation and Complete Layer FX DSP'
status: completed
type: task
created_at: 2026-11-11T16:10:00Z
updated_at: 2026-11-11T16:13:00Z
---

## Context
Parent epic: `ns4-fxrt` (Phase 5: 7-Layer Routing, Morphing Matrix & Per-Layer FX). `crates/nord-core/src/fx.rs` previously used a fixed 2048-sample delay tap without `delay_time_ms` control, `MasterClock` sync, `Analog` tape pitch-slew, `Ping-Pong` stereo toggle, feedback loop filters (`HP`/`LP`/`BP`), or `Variation`. Additionally `Mod1Type::Wah`, `Mod2Type` (`Chorus`, `Flanger`, `Phaser`, `Vibe`), `AmpCabinet`, per-layer `Comp`, `ReverbModel::Spring`, and `ReverbColor` (`Bright`, `Dark`, `Chorale`) were stubbed or not yet wired from `FxDecoded` inside `Stage4Engine::apply_fx_chain`.

## Higher Goal
Close the remaining Phase 5 FX DSP item in `docs/ROADMAP.md` and bring all 5 per-layer FX slots (`Mod 1`, `Mod 2`, `Amp Sim / EQ + Comp`, `Delay`, `Reverb`) in `crates/nord-core/src/fx.rs` to full real-time DSP parity with `docs/specs/effects-and-routing.md` and `FxDecoded`.

## Acceptance Criteria
- [x] Expand delay buffer to heap-allocated `DELAY_LINE_LEN = 131_072` (`vec![0.0; N].into_boxed_slice()`), supporting `delay_time_ms` (`10..=2000 ms`), `MasterClock` BPM subdivision sync, smooth `Analog` tape pitch-slew with soft feedback saturation, true `Ping-Pong` stereo cross-feedback, `HP`/`LP`/`BP` feedback loop filters, and `Variation` flutter modulation
- [x] Implement 2-pole resonant swept `Wah` bandpass (`Mod 1`), quadrature modulated delay / 4-stage all-pass `Chorus`/`Flanger`/`Phaser`/`Vibe` (`Mod 2`), `AmpCabinet` (`Small`, `Jc`, `Twin`, `British`) tone shaping, per-layer feed-forward `Comp`, and `ReverbModel::Spring` + `ReverbColor` (`Normal`, `Bright`, `Dark`, `Chorale`)
- [x] Wire all `FxDecoded` parameters and `MasterClock.bpm` inside `Stage4Engine` (`crates/nord-core/src/engine.rs`) while preserving backwards-compatible `LayerFxChain::process_frame`
- [x] Add deterministic unit tests covering Analog delay pitch-slew, Ping-Pong stereo cross-feed, feedback filter attenuation, Wah/Mod2 modulation, Amp cabinets, and Reverb colors with zero allocations in `process_frame`
- [x] Update `docs/specs/effects-and-routing.md` and `docs/ROADMAP.md` checkboxes in sync and pass all Cargo + Python + `tools/check_tests.py` checks

## Out of Scope
- Adding new Bay 6 GUI widgets in `crates/nord-gui` beyond existing `AudioCommand` compatibility
- `crates/nord-plugin` `nih-plug` wrapper (`ns4-707f`)

## Summary of Changes
- Expanded `LayerFxChain` in `crates/nord-core/src/fx.rs` with heap-preallocated `131,072`-sample stereo delay lines, `process_frame_with_bpm`, `snap_delay_time`, `DelaySubdivision` BPM sync, `DelayFilterMode` (`Off`/`Hp`/`Lp`/`Bp`), `delay_analog` tape slew & soft saturation, `delay_ping_pong` cross-feed, and `delay_variation` flutter.
- Added 2-pole resonant `Mod1Type::Wah`, stereo delay/all-pass `Mod2Type` (`Chorus`, `Vibe`, `Flanger`, `Phaser`), `AmpCabinet` (`Off`, `Small`, `Jc`, `Twin`, `British`), per-layer `Comp`, `ReverbModel::Spring`, and `ReverbColor` (`Normal`, `Bright`, `Dark`, `Chorale`).
- Wired all `FxDecoded` fields and `MasterClock.bpm` in `Stage4Engine`, added unit tests in `fx.rs`, and synchronized `docs/specs/effects-and-routing.md` and `docs/ROADMAP.md` (all Phase 5 items `[x]`).

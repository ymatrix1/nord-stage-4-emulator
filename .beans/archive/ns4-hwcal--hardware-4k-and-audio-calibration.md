---
# ns4-hwcal
title: 'Calibrate .npno velocity zones, 0x0B Piano bits, FDN Reverb, and non-motorized GUI against 4K hardware demo'
status: completed
type: task
parent: ns4-trkb
created_at: 2026-11-11T21:18:00Z
updated_at: 2026-11-11T21:22:00Z
---

## Context
Cross-checking the emulator against the 4K Nord Stage 4 demo video (`NEW NORD STAGE 4 - PROGRAMAS PRESETS DEMO (PART A)`), hardware `White Grand` audio, and OS v1.66 factory `.npno`/`.ns4p` files revealed four concrete accuracy gaps:
1. `.npno` byte `+0x0D` (`vel_low`) stores descending attenuation (`0 = fff`, largest value = `ppp`), so sorting ascending mapped piano velocity zones upside down.
2. `0x0B` Piano Layer bitstream decoding misaligned `string_res`/`soft_release`, `unison`, `dyn_comp`, `timbre`, and `kb_touch` (`0 = Medium` LED default).
3. Piano retrigger hard-overwrote active voices, Pedal Noise injected an artificial FM sine chirp, sympathetic string resonance summed anti-phase channels, and the 2048-sample Reverb FDN sounded boxy.
4. `sync_knobs_on_program_load` defaulted to `true`, `prog_id`/`string_res`/`pedal_noise` weren't updated in `apply_program`, and the library drawer stayed open after selection; Linux CI also lacked `libx11-xcb-dev`.

## Higher Goal
Align Piano DSP fidelity, `0x0B` preset decoding, and front-panel UX with the Nord Stage 4 hardware while restoring green Linux & Windows CI builds.

## Acceptance Criteria
- [x] Fix `.npno` descending velocity-zone sorting/partitioning, raise `MAX_PIANO_VOICES` to `64`, damp retriggered voices cleanly, remove Pedal Noise FM chirp, and balance stereo String Resonance panning
- [x] Upgrade FDN Reverb delay buffer to `FDN_BUF_LEN = 8192` with room-scaled prime tap delays and balanced stereo diffusion
- [x] Fix `0x0B` Piano Layer `acoustic_level` bitmask, `unison`, `dyn_comp`, `timbre`, and `kb_touch` decoding in `crates/nord-core/src/cbin/presets.rs`
- [x] Default `sync_knobs_on_program_load` to `false`, sync `prog_id`/`active_program_idx`/`string_res`/`pedal_noise` on program load, and add `auto_close_on_load` (`true`) to `BackupLibraryState`
- [x] Add `libx11-xcb-dev` to `.github/workflows/ci.yml` Linux dependencies
- [x] Update specs/research docs and pass all 7 local CI checks

## Out of Scope
- Custom OLED bitmap font extraction or SHARC native machine-code JIT emulation

## Summary of Changes
- Fixed `.npno` velocity zone sorting (`attenuation` descending `ppp..fff`), raised `MAX_PIANO_VOICES` to `64`, added smooth retrigger damping, removed Pedal Noise FM chirp, and fixed String Resonance pitch-class matching and stereo panning in `crates/nord-core/src/piano.rs`.
- Expanded `LayerFxChain` FDN reverb buffer (`8192` samples) and balanced stereo decorrelation in `crates/nord-core/src/fx.rs`.
- Fixed `0x0B` Piano Layer pack/unpack bit offsets and hardware fixture test in `crates/nord-core/src/cbin/presets.rs`.
- Defaulted `sync_knobs_on_program_load: false`, updated `prog_id`/`active_program_idx`/`string_res`/`pedal_noise` in `crates/nord-gui/src/panel.rs`, and added `auto_close_on_load` (`true` + `"Auto-close"` checkbox) in `crates/nord-gui/src/library.rs`.
- Added `libx11-xcb-dev` in `.github/workflows/ci.yml` and updated `docs/specs/{cbin-and-presets,piano-engine,effects-and-routing,visual-style-and-panel}.md` plus `docs/research/nsmp4-and-npno-cnsp-codec.md`.

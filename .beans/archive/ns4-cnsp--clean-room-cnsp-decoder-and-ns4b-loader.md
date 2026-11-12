---
# ns4-cnsp
title: 'Clean-Room CNSP (Fixed-LPC) Sample Decoder & .ns4b Backup Loader'
status: completed
type: task
created_at: 2026-11-11T17:10:00Z
updated_at: 2026-11-11T17:21:00Z
---

## Context
Parent epic: [ns4-trkb](../ns4-trkb--phase-7-track-b-firmware-lz-and-cnsp-sample-codec.md).
The `.nsmp4` (`\0stk` 32-bit MSB-first variable-frame fixed-LPC) and `.npno` (`1022*ch`-byte `lsb_u16be` paged fixed-LPC with `0x76` stroke table and 64-frame page overlap) `CNSP` audio codecs plus PKZIP `ZIP_STORED` `.ns4b` layout have been solved and verified across all 195 `.nsmp4` and 38 `.npno` factory assets (`docs/research/nsmp4-and-npno-cnsp-codec.md`).

## Higher Goal
Enable users to load and play their own legally owned `.npno`, `.nsmp4`, and `.ns4b` backup libraries directly through `nord-core`, `Stage4Engine`, and the `ns4` CLI with zero real-time heap allocations during synthesis.

## Acceptance Criteria
- [x] Document the solved `.nsmp4`, `.npno`, and `.ns4b` codecs in `docs/research/nsmp4-and-npno-cnsp-codec.md` and `docs/specs/cbin-and-presets.md` (§5.2 & §5.3)
- [x] Implement `.nsmp4` fixed-LPC (`0..=4`) mono & stereo decoder + loop/sample-rate extraction in `crates/nord-core/src/cbin/sample.rs` and wire `SynthSampleSound::from_nsmp4_bytes` in `crates/nord-core/src/synth.rs`
- [x] Update `NpnoInstrument` (`0x064E` key stroke counts, `0x0756` stride-`0x76` stroke table, `0x2C`-relative offsets, `+0x14..+0x24` initial LPC state) and implement paged `lsb_u16be` fixed-LPC decoder in `crates/nord-core/src/cbin/piano_sample.rs`, wired into `NpnoSound::from_npno_bytes` with stereo & multi-velocity/release zone support in `crates/nord-core/src/piano.rs`
- [x] Implement clean-room PKZIP `ZIP_STORED` reader (`crates/nord-core/src/cbin/backup.rs`) and add `Stage4Engine` + `ns4` CLI helpers for `.nsmp4`, `.npno`, and `.ns4b` loading
- [x] Add synthetic in-memory unit tests for all encoders/decoders, `backup.rs`, `synth.rs`, `piano.rs`, and `engine.rs` (`python tools/check_tests.py` stays at 0 uncovered files)
- [x] Update `docs/ROADMAP.md` & archive this bean; verify all 7 CI checks pass and push

## Out of Scope
- Redistributing proprietary Clavia `.ns4b`, `.npno`, or `.nsmp4` assets in git
- Real-time heap allocations inside `process_block()` or `render_frame()`

## Summary of Changes
- Implemented `decode_nsmp_stk_payload` & `decode_nsmp4_strokes` (`CNSP` MSB-first 32-bit word fixed-LPC `0..=4` mono/stereo + seamless `w3` sustain loop detection) in `crates/nord-core/src/cbin/sample.rs` and wired `SynthSampleSound::from_nsmp4_bytes` + true-stereo sample voice filtering in `crates/nord-core/src/synth.rs`.
- Updated `NpnoInstrument::parse` (`0x064A` channels, `0x064E` key stroke counts, `0x0756` stride-`0x76` stroke table, `0x2C`-relative offsets, `+0x14..+0x24` initial L/R LPC states, `35_000` Hz sample rate) and implemented `decode_npno_stroke` (`lsb_u16be` paged fixed-LPC with 64-frame page overlap continuity) in `crates/nord-core/src/cbin/piano_sample.rs`, wired into `NpnoSound::from_npno_bytes` with stereo & velocity/release zones in `crates/nord-core/src/piano.rs`.
- Added zero-dependency PKZIP `ZIP_STORED` `.ns4b` reader (`crates/nord-core/src/cbin/backup.rs`), `Stage4Engine` loading helpers (`crates/nord-core/src/engine.rs`), and `ns4 backup` / `ns4 inspect --decode` CLI support (`crates/nord-core/src/bin/ns4.rs`).

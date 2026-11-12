---
# ns4-dspf
title: 'ARM0 sample hash resolver and SHARC DSP calibration'
status: completed
type: task
parent: ns4-trkb
created_at: 2026-11-11T19:21:00Z
updated_at: 2026-11-11T19:45:00Z
---

## Context
Follows [ns4-trkb](../ns4-trkb--phase-7-track-b-firmware-lz-and-cnsp-sample-codec.md).
User requested two features:
1. Auto-resolve `.npno` and `.nsmp4` files inside `.ns4b` when opening an `.ns4p` program by mapping the `ARM0` hash function linking `name_hash` / `sample_id` to sample banks.
2. Reverse-engineer SHARC `ADSP-21489` DSP images (`DSPO` / `DSPS` / `DSPF`) to extract exact filter, envelope, rotary, and FX curves for `nord-core` DSP calibration.

## Higher Goal
Enable 1-click complete patch restoration from `.ns4b` backups and bring `nord-core` DSP synthesis curves closer to hardware.

## Acceptance Criteria
- [x] Reverse-engineer ARM0 preset `name_hash` / `sample_id` resolution in `bank0_CODE.bin` and verify 100% match against Factory Restore `.ns4b`
- [x] Implement auto-resolution of Piano (`.npno`) and Synth (`.nsmp4`) samples when loading `.ns4p` from `.ns4b` in `nord-core` and `nord-gui`
- [x] Locate, unpack, and map `DSPO` / `DSPS` / `DSPF` `ADSP-21489` SHARC DSP partition descriptors (`bank1_CODE.bin`) and `ARM0` parameter tables from OS v1.66
- [x] Apply extracted filter, envelope, LFO, `kb_track`, and rotary calibration curves to `nord-core` DSP engines with unit tests
- [x] Document findings in `docs/research/` and update `docs/specs/`, verify all 7 CI checks pass

## Out of Scope
- Dumping external mainboard SPI flash raw `.ldr` SHARC instruction images not embedded inside the OS updater's `ARM0` container

## Summary of Changes
- Fixed `.ns4p` bitstream offsets (`0x0B` Piano `name_hash` at `base + 28`, `variation` at `base + 60`; `0x32` Synth `sample_id` at `layer_base + 26`), implemented `nord_name_hash` / `nord_name_hash_matches` (`ARM0 @ 0x0a5a44` big-endian-chunk MurmurHash2 with `0 -> 1` and uppercase fallback), and replaced estimated Synth curves with OS v1.66 `128`-entry lookup tables (`ENV_ATTACK_MS_TABLE`, `ENV_DECAY_MS_TABLE`, `ENV_RELEASE_MS_TABLE`, `FILTER_CUTOFF_HZ_TABLE`, `LFO_RATE_HZ_TABLE`) in `crates/nord-core/src/cbin/presets.rs`.
- Added `ProgramSoundRefs`, `find_by_name_hash`, `find_piano_by_hash`, `find_sample_by_id`, and `resolve_program_sounds` to `crates/nord-core/src/cbin/backup.rs`.
- Wired automatic `.npno` and `.nsmp4` resolution into `Stage4Engine::load_program_from_backup` / `load_program_with_sounds_from_backup` (`crates/nord-core/src/engine.rs`) and 1-click async backup loading in `BackupLibraryState::load_program_entry` (`crates/nord-gui/src/library.rs`) with unit tests.
- Applied hardware `14 Hz` filter floor, `kb_track` octave scaling, `0.5 ms`/`3.0 ms` env floor + `Hold` sustain (`crates/nord-core/src/synth.rs`), and verified `ROTARY_CROSSOVER_HZ = 800.0` + `ROTARY_BASS_HORN_BALANCES` (`crates/nord-core/src/organ.rs`).
- Documented findings in `docs/research/sharc-dsp-and-preset-hash-teardown.md` and updated specs (`cbin-and-presets.md`, `synth-engine.md`, `organ-engine.md`), `docs/TECH_DEBT.md`, `docs/ROADMAP.md`, and `.beans/`.

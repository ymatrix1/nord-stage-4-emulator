---
# ns4-guil
title: 'Integrate .ns4b Backup, .npno Piano & .nsmp4 Sample Loader in nord-gui'
status: completed
type: task
created_at: 2026-11-11T18:14:00Z
updated_at: 2026-11-11T18:20:00Z
---

## Context
Parent epic: [ns4-trkb](../ns4-trkb--phase-7-track-b-firmware-lz-and-cnsp-sample-codec.md).
The clean-room `CNSP` `.npno`/`.nsmp4` decoders and `Ns4BackupArchive` reader in `nord-core` work via CLI (`ns4 backup`, `ns4 inspect --decode`), while `nord-gui` (`nord-stage4`) only accepted `.ns4p` files and lacked any UI, CLI flag, drag-and-drop handler, or real-time-safe `AudioCommand` path to load real `.npno` pianos, `.nsmp4` samples, or browse `.ns4b` archives.

## Higher Goal
Allow users to open their own `.ns4b` backups, `.npno` piano banks, and `.nsmp4` synth sample libraries directly inside the `nord-stage4` desktop application with responsive background decoding and zero deallocations on the CPAL real-time audio thread.

## Acceptance Criteria
- [x] Safely deactivate voices on `PianoLayer::set_sound` and `SynthLayer::load_sample` in `crates/nord-core` so live hot-swapping never reads mismatched zone indices
- [x] Add `AudioCommand::LoadPianoSound` and `AudioCommand::LoadSynthSample` in `crates/nord-gui/src/audio.rs` with unit tests
- [x] Add `crates/nord-gui/src/library.rs` (`BackupLibraryState`) with `.ns4b` catalog indexing, async/background `.npno`/`.nsmp4` decoding, RT-safe `Arc` lifetime management (`reap_retired_sounds`), and synthetic ZIP/`.npno`/`.nsmp4` unit tests
- [x] Wire `--backup`, `--piano`, `--sample`, multi-format drag-and-drop (`.ns4b`/`.ns4p`/`.npno`/`.nsmp4`), OLED status display, and Library browser drawer into `crates/nord-gui/src/panel.rs` and `crates/nord-gui/src/main.rs`
- [x] Update `docs/TECH_DEBT.md` and pass all 7 CI checks (`cargo fmt`, `clippy`, `cargo test`, `ruff`, `pytest`, `check_tests.py`)

## Out of Scope
- Bundling proprietary `.ns4b`, `.npno`, or `.nsmp4` binaries in the repository
- Embedding a full `egui` 0.31 editor inside `nih_plug` (`nord-plugin` editor remains future work)

## Summary of Changes
- Deactivated active layer voices in `PianoLayer::set_sound` (`crates/nord-core/src/piano.rs`) and `SynthLayer::load_sample` (`crates/nord-core/src/synth.rs`) before swapping shared sample banks.
- Added `AudioCommand::LoadPianoSound` and `AudioCommand::LoadSynthSample` in `crates/nord-gui/src/audio.rs` along with unit tests.
- Added `crates/nord-gui/src/library.rs` (`BackupLibraryState`) providing `.ns4b` archive indexing, background `.npno`/`.nsmp4` decoding, UI-thread `Arc` retention/reaping (`reap_retired_sounds`), compact top library bar, and searchable drawer window with unit tests.
- Wired CLI options (`--backup`/`-b`, `--piano`, `--sample`), multi-format drag-and-drop, and loaded instrument names on the Bay 4 & Bay 5 OLEDs in `crates/nord-gui/src/panel.rs` and `crates/nord-gui/src/main.rs`; updated `docs/TECH_DEBT.md` (`TD-01`, `TD-03`, `TD-06`).

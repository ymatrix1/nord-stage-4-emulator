---
# ns4-zm53
title: 'Make the first release shippable: pipeline review, packaging, RELEASING.md'
status: in-progress
type: task
created_at: 2026-11-14T08:00:00Z
updated_at: 2026-11-14T08:00:00Z
---

## Context
`.github/workflows/release.yml` (tags `v*.*.*`, `workflow_dispatch`) had never run and there are no tags or releases. It re-ran the whole test suite in release mode on three OSes, packaged with per-OS shell code that could not be tested, wrote CRLF checksum lines on Windows, checked out the dispatching branch instead of the tag, and had no install documentation.

## Higher Goal
Cut `v0.1.0` (standalone GUI `nord-stage4`, `ns4` CLI, CLAP + VST3 via `cargo xtask bundle nord-plugin`) with a pipeline that has been exercised locally and has a publish-free dry-run path.

## Acceptance Criteria
- [x] Local Windows build with the workflow's exact commands (`--release --locked --target x86_64-pc-windows-msvc`); `nord-plugin.clap` and `nord-plugin.vst3/Contents/x86_64-win/nord-plugin.vst3` exist
- [x] Exports verified (`clap_entry`, `GetPluginFactory`, `InitDll`, `ExitDll`) by the PE parser in `tools/release_verify.py`; DLLs load with `LoadLibrary`, `InitDll` returns true, `GetPluginFactory` non-null, CLAP entry reports version 1.2.2
- [x] `release.yml` reviewed; bugs fixed (see Summary of Changes)
- [x] Publish job runs only for a pushed `v*.*.*` tag or a dispatch with a `tag` input; dispatch without a tag is a dry run (artifacts only)
- [x] Packaging and verification in tested Python (`tools/release_package.py`, `tools/release_verify.py`, tests under `tools/tests/`); allowlist keeps `.reference/`, firmware, presets out of archives
- [x] Archive `README.txt` documents install paths per OS and states the plugin is not DAW-verified
- [x] Plugin metadata URL points at the live repository; test guards it and the manufacturer name
- [x] `docs/RELEASING.md` written; ROADMAP, docs index, README, HOST_COMPATIBILITY updated
- [ ] Maintainer dry run on GitHub Actions is green (needs a push to the default branch first, then Run workflow with empty tag)
- [ ] `v0.1.0` tagged and published (maintainer)

## Out of Scope
Code signing and notarization; Intel macOS, Linux arm64, Windows arm64 builds; AU packaging; LICENSE text (the repo has none, the packager includes any `LICENSE*` it finds); DAW verification of the plugin; bumping any version; pushing a tag or dispatching a publishing run.

## Summary of Changes
- `release.yml`: new `plan` job (tag format, tag exists on the remote, tag equals `Cargo.toml` version) gates `build` and `publish`; build checks out the tag on dispatch; top-level `contents: read`, `write` only on `publish`; Linux apt list now matches ci.yml (it lacked `libx11-xcb-dev`, `libxfixes-dev`, `libxrender-dev`, `libxinerama-dev`); the duplicate release-mode test run is removed; packaging and verification use the Python tools; `.sha256` sidecars uploaded; pre-release flag for hyphenated tags; `fail_on_unmatched_files`; checksum re-verified in `publish`.
- `tools/release_package.py`, `tools/release_verify.py` and tests; `docs/RELEASE_README.txt` shipped as `README.txt`.
- `crates/nord-plugin/src/lib.rs`: `URL` pointed at the deleted `yuriolive` account; now `ymatrix1`, with a test tying it to `CARGO_PKG_REPOSITORY`.
- `docs/RELEASING.md`, README Download section, docs index, ROADMAP, HOST_COMPATIBILITY.
- CI cost delta per release run: removes three release-profile `cargo test --workspace` compile-and-run steps; adds about 1 minute of Python steps per build job. ci.yml unchanged.

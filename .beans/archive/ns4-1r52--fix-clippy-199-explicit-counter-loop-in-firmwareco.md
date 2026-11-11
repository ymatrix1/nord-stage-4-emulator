---
# ns4-1r52
title: Fix clippy 1.99 explicit_counter_loop in firmware::copy_match
status: completed
type: bug
created_at: 2026-11-11T18:31:16Z
updated_at: 2026-11-11T18:31:16Z
parent: ns4-trkb
---

## Context
CI on main runs Rust 1.99 Clippy and failed on crates/nord-core/src/firmware.rs:415 (clippy::explicit_counter_loop).

## Higher Goal
Keep CI green on ubuntu-24.04 and windows-latest.

## Acceptance Criteria
- [x] Replace explicit pos counter loop in copy_match with (out.len() - distance..).take(length)
- [x] Verify all 7 local checks and GitHub Actions CI pass on main

## Out of Scope
- Unrelated LZO1X decompression logic changes

## Summary of Changes
- Replaced explicit pos += 1 counter loop in copy_match (crates/nord-core/src/firmware.rs) with (out.len() - distance..).take(length).

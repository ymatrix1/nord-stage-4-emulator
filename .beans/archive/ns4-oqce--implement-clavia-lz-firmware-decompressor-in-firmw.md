---
# ns4-oqce
title: Implement Clavia LZ firmware decompressor in firmware.rs
status: completed
type: task
created_at: 2026-11-11T12:32:57Z
updated_at: 2026-11-11T12:35:00Z
parent: ns4-trkb
---

## Context
Implement Clavia 16-bit LZ decompressor for firmware CODE segments in crates/nord-core/src/firmware.rs based on proven token bitfield (length = (token & 7) + 2, distance = token >> 3).

## Higher Goal
Enable extraction of ARM0 and SHARC DSP images from OS v1.66 firmware.

## Acceptance Criteria
- [x] Implement decompress_clavia_lz in crates/nord-core/src/firmware.rs
- [x] Handle uncompressed literal runs and back-references
- [x] Add unit test verifying round-trip / decompression of synthetic LZ stream
- [x] Pass check_tests ratchet and CI checks

## Out of Scope
- Modifying .reference/ or packaging proprietary binaries in git

## Summary of Changes
- Implemented `decompress_clavia_lz` and `compress_clavia_lz` in `crates/nord-core/src/firmware.rs` supporting Clavia's 16-bit token bitfield (`length = (token & 7) + 2`, `distance = token >> 3`).
- Added `FirmwareSegment::extract_payload` and `FirmwareContainer::segment` lookup.
- Added comprehensive unit tests for synthetic round-trip, bitfield packing, overlapping match runs, distance-0/out-of-bounds guards, and payload extraction boundaries.
- Formatted and verified front-panel 3D widgets in `crates/nord-gui`.
- All quality gates pass (cargo fmt, cargo clippy, cargo test, ruff, pytest, check_tests).

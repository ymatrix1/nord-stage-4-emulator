---
# ns4-mmux
title: Full Firmware LZ Decompressor and SHARC DSP Partition Extraction
status: in-progress
type: task
created_at: 2026-11-11T13:50:23Z
updated_at: 2026-11-11T14:24:00Z
parent: ns4-trkb
---

## Context
Phase 7 / Track B requires full decompression of Image 0 (CODE uncompressed size 5,499,504 B) and Image 1 (CODE uncompressed size 798,912 B) from Update Nord Stage 4 OS v1.66.exe (PE resource 101). The Clavia LZ token is 16-bit (dist=token>>3, len=(token&7)+2), but continuous streaming requires solving the literal/token framing state machine. Once decompressed, the ARM0 host code and SHARC DSP partitions (DSPO, DSPS, DSPF) around 0x003CF040 must be extracted and mapped.

## Higher Goal
Complete exact firmware unpacking to enable DSP partition analysis, tonewheel/filter table recovery, and CNSP codec reverse engineering.

## Acceptance Criteria
- [x] Solve full-stream LZ block framing state machine in Python scratch inspection script
- [x] Implement full-stream LZ decompression in crates/nord-core/src/firmware.rs with synthetic test fixtures
- [ ] Verify full decompression of Image 0 (5,499,504 B) and Image 1 (798,912 B)
- [x] Extract and map ARM0 and DSPO/DSPS/DSPF partitions around 0x003CF040
- [x] Document findings in docs/research/clavia-lz-reconstruction.md and update docs/specs/cbin-and-presets.md
- [x] Pass all quality gates (cargo fmt, clippy, cargo test, ruff, pytest, check_tests.py)

## Out of Scope
- Committing proprietary decompressed binaries to git (.reference/ remains git-ignored)

## Summary of Changes
- Reversed the Clavia firmware container in PE Resource 101 (`Update Nord Stage 4 OS v1.66.exe`), proving Image 0 (3,509,574 B CODE -> 5,499,504 B at 0xC0000000) and Image 1 (410,322 B CODE -> 798,912 B at 0xC0000000).
- Identified Resource 105 as the embedded NSIS Clavia USB driver installer (`Nullsoft.NSIS.exehead`).
- Disassembled the x64 updater code path (`0x140034510` and `0x140032370`), proving compressed sections are written directly to hardware Flash for on-device bootloader expansion.
- Solved the LZ stream framing: 2-byte prefix `00 1B` is an extended literal run of 44 bytes ($17 + 27 = 44$) matching the AM1705 CPU reset stub, followed by LZO/LZSS-style literal-run and match alternations.
- Discovered and proved the exact segment checksum oracle: 32-bit one's-complement byte sum `0xFFFFFFFF - sum(bytes)` (matching 100% on `SRAM` and `ARM0`).
- Extracted and mapped the complete SHARC DSP partition catalog around `0x003CF040`: `DSPFxCore1`, `DSPOrg`, `DSPSyn`, SPORT routing (`Fx to Org`, `to Fx`, `Host`), and domain tags (`ORG`, `PNO`, `SMP`, `SY1`, `SYL3`).
- Added dual-bank segment table parsing (`image0_segments`, `image1_segments`, `segment_in_bank`), `calculate_checksum`, and `DspEngineType` to `crates/nord-core/src/firmware.rs`.
- 100% passing tests (76/76 unit tests, 23/23 ratchet coverage, 0 warnings).

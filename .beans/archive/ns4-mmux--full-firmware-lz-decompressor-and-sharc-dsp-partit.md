---
# ns4-mmux
title: Full Firmware LZ Decompressor and SHARC DSP Partition Extraction
status: completed
type: task
priority: normal
created_at: 2026-11-11T13:50:23Z
updated_at: 2026-11-11T17:32:11Z
parent: ns4-trkb
---


## Context
Phase 7 / Track B requires full decompression of Image 0 (CODE uncompressed size 5,499,504 B) and Image 1 (CODE uncompressed size 798,912 B) from Update Nord Stage 4 OS v1.66.exe (PE resource 101). The Clavia LZ token is 16-bit (dist=token>>3, len=(token&7)+2), but continuous streaming requires solving the literal/token framing state machine. Once decompressed, the ARM0 host code and SHARC DSP partitions (DSPO, DSPS, DSPF) around 0x003CF040 must be extracted and mapped.

## Higher Goal
Complete exact firmware unpacking to enable DSP partition analysis, tonewheel/filter table recovery, and CNSP codec reverse engineering.

## Acceptance Criteria
- [x] Solve full-stream LZ block framing state machine in Python scratch inspection script
- [x] Implement full-stream LZ decompression in crates/nord-core/src/firmware.rs with synthetic test fixtures
- [x] Verify full decompression of Image 0 (5,499,504 B) and Image 1 (798,912 B)
- [x] Extract and map ARM0 and DSPO/DSPS/DSPF partitions around 0x003CF040
- [x] Document findings in docs/research/clavia-lz-reconstruction.md and update docs/specs/cbin-and-presets.md
- [x] Pass all quality gates (cargo fmt, clippy, cargo test, ruff, pytest, check_tests.py)

## Out of Scope
- Committing proprietary decompressed binaries to git (.reference/ remains git-ignored)

## Summary of Changes
- Reversed the Clavia firmware container in PE Resource 101 (`Update Nord Stage 4 OS v1.66.exe`), proving Image 0 (3,509,574 B CODE table span -> 5,499,504 B at 0xC0000000) and Image 1 (410,322 B CODE table span -> 798,912 B at 0xC0000000) use standard **LZO1X** compression (`decompress_clavia_lz` / `compress_clavia_lz`), with Bank 0 offsets relative to `header_size` (`0x20`) and Bank 1 offsets relative to `image1_offset` (`0x368FA6`).
- Identified Resource 105 as the embedded NSIS Clavia USB driver installer (`Nullsoft.NSIS.exehead`).
- Disassembled the x64 updater code path (`0x140034510` and `0x140032370`), proving compressed sections are written directly to hardware Flash for on-device bootloader expansion.
- Solved the LZO1X stream framing end-to-end: `00 1B` starts with a 45-byte extended literal run (`3 + 15 + 0x1B = 45`), followed by M2/M3/M4 matches and trailing literals ending at `11 00 00`.
- Discovered and proved the exact segment checksum oracle: 32-bit one's-complement byte sum `0xFFFFFFFF - sum(bytes)`, verified bit-exact across all 6 OS v1.66 segments via `ns4 inspect --verify --unpack` (`bank0_SRAM` 64,640 B, `bank0_CODE` 5,499,504 B `0xDE902503`, `bank0_ARM0` 748 B, `bank1_SRAM` 49,496 B, `bank1_CODE` 798,912 B `0xFBB813F8`, `bank1_ARM0` 820 B).
- Extracted and mapped the SHARC DSP partition catalog around `0x003CF040`: `DSPFxCore1`, `DSPOrg`, `DSPSyn`, SPORT routing (`Fx to Org`, `to Fx`, `Host`), and domain tags (`ORG`, `PNO`, `SMP`, `SY1`, `SYL3`).
- Added dual-bank segment table parsing (`image0_segments`, `image1_segments`, `segment_in_bank`, `bank_base`, `extract_segment`), `calculate_checksum`, `DspEngineType`, and CLI `--verify` / `--unpack` flags in `crates/nord-core/src/firmware.rs` and `crates/nord-core/src/bin/ns4.rs`.

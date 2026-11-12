---
# ns4-trkb
title: 'Phase 7 (Track B): Full Firmware LZ Decompression & CNSP Sample Codec Reverse Engineering'
status: in-progress
type: epic
created_at: 2026-11-11T03:30:00Z
updated_at: 2026-11-11T17:21:00Z
---

## Context
Blocked by: none
Decision [D12](../docs/DECISIONS.md#d12-pursue-track-b-reverse-engineering-for-cnsp-sample-codec-and-firmware-decompression) establishes Track B as a formal reverse-engineering and interoperability track. We already proved that the Nord Stage 4 OS v1.66 `CODE` segments are 100% unencrypted and compressed with Clavia LZO1X. Unpacking `ARM0` and `DSPO`/`DSPS`/`DSPF` plus differential analysis on the 233 factory `CNSP` banks solved both the `.nsmp4` and `.npno` fixed-LPC codecs.

## Higher Goal
Enable native, zero-allocation loading of user-supplied `.npno` (Nord Piano Library) and `.nsmp4` (Nord Sample Library 4) files directly from a user's legally owned `.ns4b` backup archive, and extract exact SHARC DSP lookup tables.

## Acceptance Criteria
- [x] Solve the literal-run / control-bit framing state machine in Clavia's custom LZ stream (`docs/research/clavia-lz-reconstruction.md`)
- [x] Implement the firmware `CODE` decompressor in `crates/nord-core/src/firmware.rs` and verify exact output sizes (`798,912 B` for Image 1; `5,499,504 B` for Image 0)
- [x] Map the `ARM0` MurmurHash2 preset link hash (`0x0a5a44`), SHARC `DSPO`/`DSPS`/`DSPF` partition descriptors (`bank1_CODE.bin`), and OS v1.66 5×128 DSP parameter tables (`docs/research/sharc-dsp-and-preset-hash-teardown.md`, `ns4-dspf`)
- [x] Map the `CNSP` header & stroke tables across `.nsmp4` and `.npno` files (sample rate, bit depth, key-split zones, loop points)
- [x] Reverse-engineer the `CNSP` audio payload decompression algorithm from differential bitstream analysis (`docs/research/nsmp4-and-npno-cnsp-codec.md`)
- [x] Implement a clean-room `CNSP` sample decoder and `.ns4b` PKZIP loader in `crates/nord-core` with deterministic unit tests using synthetic fixtures (`ns4-cnsp`)

## Out of Scope
- Redistributing proprietary Clavia `.npno`, `.nsmp4`, or firmware binaries in git (`.reference/` remains strictly git-ignored)

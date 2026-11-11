---
# ns4-trkb
title: 'Phase 7 (Track B): Full Firmware LZ Decompression & CNSP Sample Codec Reverse Engineering'
status: open
type: epic
created_at: 2026-11-11T03:30:00Z
updated_at: 2026-11-11T03:30:00Z
---

## Context
Blocked by: none
Decision [D12](../docs/DECISIONS.md#d12-pursue-track-b-reverse-engineering-for-cnsp-sample-codec-and-firmware-decompression) establishes Track B as a formal reverse-engineering and interoperability track. We already proved that the Nord Stage 4 OS v1.66 `CODE` segments are 100% unencrypted and compressed with a custom 16-bit LZ token scheme (`distance = token >> 3`, `length = (token & 7) + 2`). Unpacking the full `ARM0` and `DSPO`/`DSPS`/`DSPF` binaries and performing differential analysis on the 233 `CNSP` sample headers (`0x30..0x80` in `.npno` and `.nsmp4`) will allow us to decode the `CNSP` audio sample format.

## Higher Goal
Enable native, zero-allocation loading of user-supplied `.npno` (Nord Piano Library) and `.nsmp4` (Nord Sample Library 4) files directly from a user's legally owned `.ns4b` backup archive, and extract exact SHARC DSP lookup tables.

## Acceptance Criteria
- [ ] Solve the literal-run / control-bit framing state machine in Clavia's custom LZ stream (`docs/research/clavia-lz-reconstruction.md`)
- [ ] Implement the firmware `CODE` decompressor in `crates/nord-core/src/firmware.rs` and verify exact output sizes (`798,912 B` for Image 1; `5,499,504 B` for Image 0)
- [ ] Extract and map the unpacked `ARM0` host binary and `DSPO` / `DSPS` / `DSPF` SHARC DSP segments
- [ ] Map the 80-byte `CNSP` sub-header (`0x30..0x80`) across `.nsmp4` and `.npno` files (sample rate, bit depth, key-split zones, loop points)
- [ ] Reverse-engineer the `CNSP` audio payload decompression algorithm from the unpacked firmware and differential bitstream analysis
- [ ] Implement a clean-room `CNSP` sample decoder in `crates/nord-core` with deterministic unit tests using synthetic fixtures

## Out of Scope
- Redistributing proprietary Clavia `.npno`, `.nsmp4`, or firmware binaries in git (`.reference/` remains strictly git-ignored)

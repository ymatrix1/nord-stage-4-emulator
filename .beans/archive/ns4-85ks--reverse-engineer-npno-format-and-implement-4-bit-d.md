---
# ns4-85ks
title: Reverse-engineer .npno format and implement 4-bit DPCM decoder in piano_sample.rs
status: completed
type: task
created_at: 2026-11-11T12:54:01Z
updated_at: 2026-11-11T12:54:01Z
parent: ns4-trkb
---

## Context
Reverse-engineer Nord Piano Library .npno container, 128-key mapping table, cents detuning table, velocity tiers, and 4-bit DPCM audio codec.

## Higher Goal
Enable native loading and playback of Nord Piano .npno instruments directly from .ns4b backups.

## Acceptance Criteria
- [x] Analyze .npno binary format and document in docs/research/nord-piano-npno-and-codec.md
- [x] Prove 4-bit signed nibble DPCM audio encoding and Gaussian distribution
- [x] Implement NpnoInstrument and decode_4bit_dpcm in crates/nord-core/src/cbin/piano_sample.rs
- [x] Document in docs/specs/cbin-and-presets.md §5.2
- [x] Pass all quality gates

## Summary of Changes
- Created crates/nord-core/src/cbin/piano_sample.rs with NpnoInstrument and decode_4bit_dpcm / encode_4bit_dpcm.
- Added 5 unit tests in piano_sample.rs.
- Documented format and codec in docs/research/nord-piano-npno-and-codec.md and docs/specs/cbin-and-presets.md.
- Updated docs/ROADMAP.md.

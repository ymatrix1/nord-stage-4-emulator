---
# ns4-ulax
title: Harden CBIN bitstream bounds and manifest parsing
status: completed
type: bug
created_at: 2026-11-11T12:34:35Z
updated_at: 2026-11-11T12:55:00Z
parent: ns4-707f
---

## Context
The audit found unchecked bit-position arithmetic and a manifest parser that depends on one exact XML attribute spelling.

## Higher Goal
Reject invalid bitstream cursor positions without overflow and parse valid manifest XML independently of whitespace and quote style.

## Acceptance Criteria
- [x] Validate reader/writer cursor seeks against payload bounds
- [x] Use checked arithmetic for bit range validation
- [x] Add boundary and overflow regression tests
- [x] Parse valid XML attribute spacing and single/double quotes
- [x] Preserve invalid XML rejection and existing manifest fields
- [x] Update specs if behavior changes
- [x] Run project quality gates

## Out of Scope
Checksum recomputation and other CBIN interpretation work

## Summary of Changes
- Enforced bounds validation in `BitReader::set_bit_pos` and `BitWriter::set_bit_pos`, returning `CbinError::BitstreamError` if seek target exceeds payload bit length.
- Protected cursor advance calculations using `checked_add` and `checked_mul(8)` in `BitReader` and `BitWriter`.
- Upgraded `BackupManifest::parse_xml` to support arbitrary whitespace around `=` and single/double quotation styles while rejecting unclosed attributes or missing root `<backup>` tags.
- Added comprehensive unit tests in `crates/nord-core/src/cbin/bitstream.rs` and `crates/nord-core/src/cbin/manifest.rs`.
- Documented invariants in `docs/specs/cbin-and-presets.md`.

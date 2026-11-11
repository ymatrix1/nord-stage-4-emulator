---
# ns4-f95l
title: Harden CBIN bitstream bounds and manifest parsing
status: completed
type: bug
priority: normal
created_at: 2026-11-11T12:42:18Z
updated_at: 2026-11-11T12:42:30Z
parent: ns4-707f
---

## Context
The CBIN bitstream cursor accepted arbitrary positions and used unchecked addition. The backup manifest parser depended on exact XML attribute formatting.

## Higher Goal
Reject invalid bitstream operations before they change state and parse valid backup manifests robustly.

## Acceptance Criteria
- [x] Validate reader/writer cursor seeks against payload bounds
- [x] Use checked arithmetic for bit range validation
- [x] Add boundary and overflow regression tests
- [x] Parse XML attribute spacing and single/double quotes with an XML parser
- [x] Preserve rejection of wrong root, malformed XML, missing fields, and non-integer values
- [x] Update the CBIN specification
- [x] Run project quality gates

## Out of Scope
Checksum validation/recomputation and other CBIN interpretation work

## Summary of Changes
- `BitReader` and `BitWriter` return `Result` from cursor seeks, reject positions beyond payload end, and use checked range arithmetic.
- Preset bitstream callers propagate seek errors.
- `BackupManifest` parses a complete `<backup>` XML root with quick-xml, accepting valid spacing and quote styles while validating required numeric fields.
- Added tests for seek boundaries, overflow, XML spacing/quote variants, wrong root, and malformed/incomplete XML.
- Updated `docs/specs/cbin-and-presets.md`.

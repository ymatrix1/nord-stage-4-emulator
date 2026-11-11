---
# ns4-4fdw
title: Parse Nord Sample Library 4 metadata
status: completed
type: task
priority: normal
created_at: 2026-11-11T14:28:48Z
updated_at: 2026-11-11T14:34:07Z
parent: ns4-trkb
---

## Context

The factory-bank download adds exploratory `.nsmp4` chunk parsing while `main` already contains the CBIN core and `.npno` path.

## Higher Goal

Inspect user-owned sample-bank metadata without distributing factory assets.

## Acceptance Criteria

- [x] Parse bounded NSMP chunks and metadata with deterministic synthetic tests.
- [x] Document verified parser behavior and synchronize the roadmap.

## Out of Scope

CNSP audio decoding and factory asset redistribution.

## Summary of Changes

Added `cbin::sample` with bounded chunk parsing, initial header/map/stroke metadata, malformed-input tests, and spec/roadmap updates. Verified the parser against a temporary extraction of `Kalimba_KG 4.1.nsmp4` from the local factory backup; the sample was removed after validation and remains out of git.

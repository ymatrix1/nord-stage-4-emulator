---
# ns4-6rb5
title: Route MIDI events by global and layer channels
status: completed
type: bug
priority: normal
created_at: 2026-11-11T12:24:53Z
updated_at: 2026-11-11T12:32:48Z
parent: ns4-707f
---

## Context
The Stage 4 MIDI specification carries channel numbers, but the engine previously routed notes and global controls without checking them. The hardware supports a Global Channel and receive channels for individual Organ, Piano, and Synth layers.

## Higher Goal
Match Stage 4 MIDI input routing so the Global Channel follows the keyboard scene/split path while layer channels address individual active layers directly.

## Acceptance Criteria
- [x] Configure a Global Channel, including `Off`, and optional per-layer channels
- [x] Route global notes through active scene and split rules
- [x] Route per-layer notes directly, bypassing keybed split assignments
- [x] Match Note Off to the originating channel and layer recipients
- [x] Preserve a held pitch while another channel still holds the same layer/note
- [x] Ignore unmapped channel input and non-global global controls
- [x] Add regression tests and update the MIDI specification against the official manual
- [x] Pass project quality gates

## Out of Scope
- Aux KB mode, per-section `Sect On`, MIDI CC/NRPN receive filters on layer channels, and Program Change engine behavior

## Summary of Changes
- Added configurable Global Channel (`1..16` or `Off`) and optional per-layer MIDI channels.
- Global notes use active scene and split routing; per-layer notes target only configured active layers and bypass split assignments.
- Track note holds by channel, pitch, and layer; Note Off releases only after all channels holding the same layer/pitch have released it.
- Process CC and channel aftertouch only on the configured Global Channel.
- Added regression tests for unmapped channels, Global Channel configuration/Off, direct per-layer routing, channel-specific Note Off, and overlapping same-pitch holds.
- Updated `docs/specs/midi-and-control.md` to align with the Nord Stage 4 OS v1.6x manual.

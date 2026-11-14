---
# ns4-rwhu
title: Arp step/slot time and rate table parity (engine plays 16th at linear 30..300 BPM)
status: todo
type: task
created_at: 2026-11-14T17:16:04Z
updated_at: 2026-11-14T17:16:04Z
parent: ns4-xc5n
---

## Context

`SynthLayer::step_seconds` plays one step as a 16th note at `arp_rate_bpm` (`15 / D` seconds) and `synth_preset.rs` maps the 7-bit stored rate linearly to 30..300 BPM. The fourth research pass (`docs/research/synth-arp-patterns.md` section 8) found three facts that do not agree with that: the firmware rate table has 128 exponential entries (display BPM `16 .. 1200`, ns4decode lists the same values up to 990 then "Fast1..5"), the free-running clock moves `2^19` phase units per firmware beat with a grid point every `2^18` (`15 / D` s), and the manual (p.36) says one pattern step is a 1/8 note (`30 / D` s) at the displayed BPM. Either a grid point is a slot (then the engine plays every step twice as fast) or it is a step (then the manual wording is loose).

## Higher Goal

Arp / Gate rate and step length equal the hardware (1:1 parity).

## Acceptance Criteria

- [ ] Decide step length: locate the record player / grid consumer in the firmware, or measure a hardware recording of the arp at a known rate (dial value and recorded note spacing).
- [ ] Replace the linear 30..300 BPM stored-rate mapping by the 128-entry firmware table (`tools/ns4_arp_gate.py clock` prints it; `/2` is the displayed BPM), widen the GUI knob / host range only additively.
- [ ] Fix `step_seconds` if the recording shows the engine is 2x fast; update the arp / gate tests and the spec.

## Out of Scope

Master Clock sync (already beats-based); pattern content; tie playback.

## Summary of Changes

(open)

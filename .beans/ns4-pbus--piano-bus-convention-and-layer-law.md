---
# ns4-pbus
title: Piano bus convention and layer level law
status: completed
type: task
priority: normal
created_at: 2026-11-14T08:00:00Z
updated_at: 2026-11-14T09:40:00Z
---

**Context** OQ20: SMP bus receiver convention, layer level handler `0xC011A7A4` never run. **Higher Goal** Settle the absolute piano level and the layer law without guessing (D22). **Acceptance Criteria** - [x] Run the handler live - [x] Opt-in layer law with tests - [ ] Receiver / master scale decoded (not reached: ring is the SMP serial TX buffer, receiver not found) - [x] Docs, OQ20, ROADMAP, D26 **Out of Scope** Flipping defaults. **Summary of Changes** `tools/arm_layer_level.py`, `PianoLayer::firmware_layer_law`, `--firmware-piano-layer-law`, `docs/research/piano-bus-convention.md`, D26.

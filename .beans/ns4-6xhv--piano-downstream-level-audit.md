---
# ns4-6xhv
title: Piano downstream level audit
status: completed
type: task
priority: normal
created_at: 2026-11-13T23:48:45Z
updated_at: 2026-11-13T23:49:26Z
---

**Context** After D23 the Velvet Grand segment reads about +12 dB hot. **Higher Goal** Find the stage after the voice that causes it. **Acceptance Criteria** - [x] Trace SMP output stage, layer level, master, resonance - [x] Opt-in bus scale with tests - [x] Before/after emu_vs_hw - [x] Docs, OQ20, ROADMAP **Out of Scope** Decoding the SMP bus receiver. **Summary of Changes** piano/downstream.rs, --firmware-piano-bus, research note, D24.

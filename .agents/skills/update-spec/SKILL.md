---
name: update-spec
description: Update subsystem technical specifications in docs/specs/ and synchronize ROADMAP checkboxes.
---

# Update spec

1. Identify all affected specs in `docs/specs/` (`cbin-and-presets.md`, `organ-engine.md`, `synth-engine.md`, `piano-engine.md`, `effects-and-routing.md`, `morph-and-zones.md`, `midi-and-control.md`, `verification.md`).
2. Update byte tables, equations, or Rust struct/trait interfaces so docs and code match 1:1.
3. Update `docs/ROADMAP.md` checkboxes (`- [x]`) for any completed items.
4. If an architectural choice was made, record it in `docs/DECISIONS.md`.

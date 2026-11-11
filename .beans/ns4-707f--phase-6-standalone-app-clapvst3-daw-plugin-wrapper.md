---
# ns4-707f
title: 'Phase 6: Standalone App & CLAP/VST3 DAW Plugin Wrapper (nih-plug)'
status: todo
type: epic
created_at: 2026-11-11T00:47:46Z
updated_at: 2026-11-11T00:47:46Z
---

## Context
Blocked by: none
Deliver dual-target Standalone and CLAP/VST3 DAW plugin (D11) wrapping 
ord_core::engine::Stage4Engine.

## Higher Goal
Allow live gigging and DAW hosting from a single Rust codebase.

## Acceptance Criteria
- [ ] Add crates/nord-plugin with 
ih-plug CLAP + VST3 + Standalone targets
- [ ] Map 7-layer parameters, drawbars, split zones, and Morph matrix to host automation
- [ ] Add front-panel GUI (
ih_plug_vizia)

## Out of Scope
- Proprietary .npno audio decryption

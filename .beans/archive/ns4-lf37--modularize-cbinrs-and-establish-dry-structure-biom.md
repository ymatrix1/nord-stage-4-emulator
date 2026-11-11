---
# ns4-lf37
title: Modularize cbin.rs and establish DRY structure & Biome config
status: completed
type: task
priority: normal
created_at: 2026-11-11T01:33:22Z
updated_at: 2026-11-11T01:42:41Z
---

Refactor crates/nord-core/src/cbin.rs (1,548 lines) into modular submodules (container, bitstream, presets, manifest) with dedicated unit tests, adhering to file size best practices and adding biome.json for schema/config linting.

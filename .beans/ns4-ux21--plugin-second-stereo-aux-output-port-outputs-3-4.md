---
# ns4-ux21
title: 'Plugin: second stereo aux output port (outputs 3-4)'
status: in-progress
type: task
created_at: 2026-11-14T20:47:51Z
updated_at: 2026-11-14T20:47:51Z
parent: ns4-xc5n
---

## Context
`crates/nord-plugin` exposed one stereo output, so anything the Output menu routes to outputs 3-4 was dropped, although `Stage4Engine::render_frame_multi` / `process_block_multi` already produce all four outputs (`ns4-zb9o` left "multi-output host audio ports" open).

## Higher Goal
Use the engine's Output menu in a DAW: send a source to a second stereo track, as on the hardware's outputs 3-4 (`docs/specs/daw-plugin.md` Audio ports).

## Acceptance Criteria
- [x] `AUDIO_IO_LAYOUTS` lists main 1-2 + aux 3-4 first, stereo only second (`audio_out.rs`)
- [x] The aux buffer is filled from the engine's routed 3-4 bus and is exact zeros when nothing is routed there
- [x] The main pair is bit-identical with and without the aux port (`tests/aux_outputs.rs`)
- [x] Zero allocations and no locks in `process()` with the 2-bus layout (counting allocator test `two_bus_process_never_touches_the_heap`)
- [x] Host parameter ids unchanged (`cargo test -p nord-plugin param_snapshot`)
- [x] `docs/specs/daw-plugin.md`, `docs/specs/effects-and-routing.md` 2.10, `docs/HOST_COMPATIBILITY.md` 7.8 and `docs/ROADMAP.md` updated
- [ ] Layout selection, aux track routing and 3-4 audio checked in a DAW (no DAW available; B13 stays open)

## Out of Scope
- More than one aux pair (no mono outputs 1..4 as separate ports)
- Per-output trims in the host beyond the existing `outx_trim_*`
- Any DAW run (see 7.6 and 7.7 of HOST_COMPATIBILITY)

## Summary of Changes
- New `crates/nord-plugin/src/audio_out.rs`: layouts plus `render_main`, `render_main_aux`, `render_mono`; `process()` in `lib.rs` uses them. The single-bus path renders `render_frame` as before.
- New `crates/nord-plugin/tests/aux_outputs.rs`: bit-identity, routed and silent aux, allocator count with the aux port.
- Docs: daw-plugin.md, effects-and-routing.md, HOST_COMPATIBILITY.md, ROADMAP.md.

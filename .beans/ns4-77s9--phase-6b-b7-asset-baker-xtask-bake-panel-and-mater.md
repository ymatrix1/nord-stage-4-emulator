---
# ns4-77s9
title: 'Phase 6b B7: asset baker (xtask bake-panel) and material/shadow system'
status: todo
type: task
created_at: 2026-11-12T21:36:08Z
updated_at: 2026-11-12T21:36:08Z
parent: ns4-xc5n
blocked_by:
    - ns4-7ky2
    - ns4-r7vt
---

## Context
D15 (accepted) chooses Blender-rendered sprites and filmstrips for static layers, `tiny-skia` for flat vector layers, and the egui painter for dynamic layers. D16 (accepted) fixes the art rules: original art only, no Nord logo or photo-derived assets.

Plan: `docs/ROADMAP.md` Phase 6b, `docs/specs/control-inventory.md`, decisions D15 and D16 in `docs/DECISIONS.md`.

## Higher Goal
Every Nord Stage 4 front-panel control from the manual exists in the emulator with a layout rect, a renderer and a binding (Bound, UiOnly or Unsupported), drawn as a 2.5D panel shared by the standalone app and the plugin editor.

## Acceptance Criteria
- [ ] Proof first: one rendered knob filmstrip and one fader cap with shadow, drawn in egui next to a flat painter version at 1x and 2x; atlas size measured and recorded before the full set (renders and atlas size done; egui comparison waits for B4, a PIL stand-in is in `preview_2x.png`)
- [x] Blender render script committed (headless, scripted; the scene is fully generated, so no `.blend` file exists); Blender is not required by CI or normal builds (proof set; full set still to do)
- [x] Rendered PNGs committed with content hashes; CI verifies hashes and deterministic atlas packing, and does not re-render (proof set; `tools/tests/test_panel_assets.py` runs in the existing pytest job)
- [ ] xtask bake-panel packs 1x and 2x atlases from original art (no photo crops, no assets derived from `.reference/` models) and draws flat vector layers with tiny-skia
- [ ] Theme tokens, one top-left light, per-element height token drives shadow offset/blur for live-drawn elements

## Out of Scope
- Redistributing Clavia photos, manuals or firmware; `.reference/` stays git-ignored
- Renaming or removing host parameter IDs shipped by the DAW plugin (stable contract)

## Summary of Changes
Proof, Blender and asset side (PR 84, reworked in v2 on `feat/blender-panel-proof-v2` after the owner found the first renders too far from the hardware):
- `assets/blender/` (`build_proof.py`, `proof_scene.py`, `proof_geom.py`, `proof_materials.py`, `parts_*.py`): headless Blender 5.2 scene with an original 20-flute knob (bent tapered pointer), separate arc-ring and DRIVE-scale layers, black/white drawbar caps with a shared shadow, grey layer cap with its own shadow, grey/black pill buttons in wells, seamless red/slate/silver tiles and the slotted slate plate; one top-left sun (only shadow caster), perspective top-down camera with a per-class lean.
- `assets/panel/theme_samples.json`: colours and proportions measured on the reference (numbers only).
- `tools/panel_assets.py`, `tools/panel_preview.py`, `tools/panel_compare.py` (+ tests): render driver, filmstrip stitcher, seamless tiles, SHA-256 manifest and verify, size report, composed preview, private side-by-side helper with a per-feature difference metric.
- v3 (`claude/blender-match-organ-a1e8a9`): matched feature by feature to the owner's Organ section reference at the same feature scale.
- v4 (same branch, D28): every knob and button style of the reference. `tools/panel_refbake.py` cuts 15 buttons, 2 triangle selectors with lit overlays, the knob bodies, the drawbar and layer caps with their shadows, the DRIVE scale and the plate tiles from the reference. Blender renders the per-knob pointer filmstrips and shadows. Reference-derived files are listed in D28.
- Measured (v4): whole proof set 824 KB (1x) / 2.76 MB (2x); full-panel estimate 2.16 MB / 6.64 MB PNG (counts `ASSUMED` until B2). See `docs/research/panel-blender-proof.md`, which also lists what still differs from the reference.
- Not done: egui comparison (needs B4), `xtask bake-panel`, tiny-skia layers, theme tokens in code, button down/lit states, per-position lean, full set.

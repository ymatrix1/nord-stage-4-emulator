# Research: Blender panel proof (knob filmstrip, fader cap, atlas size)

- **Checked date:** 2026-11-12
- **Sources:** `assets/blender/build_proof.py` (scene), `tools/panel_assets.py` (pack, hash, measure), [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering), [D16](../DECISIONS.md), [visual-style-and-panel.md](../specs/visual-style-and-panel.md). Reference photos in the owner's `.reference/` were used only to measure proportions and the rotary scale sweep; no pixels or geometry are used (D16).
- **Question:** can a scripted headless Blender pipeline produce original, good-looking 2.5D sprites, and how big is the atlas set at 1x and 2x?

## Reproduce

```bash
python tools/panel_assets.py render    # Blender 5.2 headless -> assets/panel/proof/_raw (git-ignored)
python tools/panel_assets.py pack      # stitch 8x8 filmstrips, write manifest.json, print sizes
python tools/panel_assets.py preview   # assets/panel/proof/preview_2x.png
python tools/panel_assets.py verify    # CI-safe: committed PNGs match manifest hashes
```

`BLENDER=<path to blender.exe>` overrides the default install path. Render time on CPU Cycles (128 samples, OIDN denoise): about 2.5 min for everything. Blender output is not bit-identical across machines, so the committed PNGs are the source of truth; packing is deterministic (same input frames give identical bytes, tested).

## Setup

| Item | Value | Tag |
|---|---|---|
| Light | one sun, top-left, elevation about 55 deg, 9 deg angular size; ambient world 0.22 | `ASSUMED` (placement fixed by D15) |
| Camera | orthographic, tilted 14 deg toward the player so cap sides and slot walls show | `ASSUMED`, confirm in B8 |
| Scale | 64 px per Blender unit at 1x; 2x doubles | `EMPIRICAL` |
| Knob rotation | 64 frames, -135 to +135 deg clockwise (270 deg) | `EMPIRICAL`: the Rotary Speaker `DRIVE` scale on the reference photo runs 0 at lower-left to 10 at lower-right |
| Materials | red lacquer (`#8E1B1F`), dark slate (`#323D47`), rubber (`#1C1F22`), matte cap (`#8C9196`); procedural flake and grain, no image textures | tokens `VERIFIED` from the spec table |
| Shadow layers | knob: baked into each frame (shadow catcher); fader: separate sprite, cap held out | |

Colour check (mean of rendered plate vs token): red plate (138, 39, 41) vs (142, 27, 31); slate (56, 66, 75) vs (50, 61, 71). Slightly lifted by ambient and specular; acceptable for the proof.

## Sizes (committed PNG, 8-bit RGBA)

| Asset | 1x px | 1x bytes | 2x px | 2x bytes |
|---|---|---|---|---|
| Knob filmstrip, 64 frames, 8x8 grid | 768x768 | 372,214 | 1536x1536 | 1,091,749 |
| Fader cap | 96x96 | 5,928 | 192x192 | 17,116 |
| Fader shadow (own layer) | 96x96 | 5,194 | 192x192 | 13,905 |
| Red plate | 128x128 | 12,916 | 256x256 | 53,402 |
| Slate plate with slot | 128x256 | 23,856 | 256x512 | 106,894 |
| **One of each (shared set)** | | **420,108** | | **1,283,066** |

Full-panel estimate (`ASSUMED`: 45 knobs, 30 faders, 150 buttons until B2 verifies counts; 4 knob styles and 3 fader styles, each style one filmstrip or cap pair; buttons not rendered yet, counted in GPU memory only as 3 sprites of 64 px square):

| | PNG on disk (excl. buttons) | RGBA in GPU memory (incl. buttons) |
|---|---|---|
| 1x | about 1.56 MB | about 12.3 MB |
| 2x | about 4.62 MB | about 49.1 MB |

All knobs of one style share a filmstrip, so the count of knobs does not change size; only the number of distinct styles does.

## What looks good

- Tilted orthographic camera gives real depth: knob flutes, shoulder and dished top read well; the wedge fader cap shows its stepped front and grip ridges.
- The fader shadow separates cleanly (alpha only in the shadow layer), so it can be offset or recoloured in egui.
- The flat painter stand-in in `preview_2x.png` (bottom row) is clearly flatter: no flutes, no soft shadow.

## What looks poor or is unfinished

- Knob filmstrip is large because Cycles grain survives denoising (about 1.1 MB at 2x for one style). Palette quantisation or lower-noise settings should cut it.
- Red plate is slightly lighter and less saturated than the token.
- Cap top is flat grey; no wear or label. Ridges are plain boxes.
- Pointer is a plain white bar; the scale arc and LEDs are dynamic layers, not rendered here.
- The comparison drawn in egui is not done: it needs `nord-panel` from B4. `preview_2x.png` uses PIL as a stand-in.

## Implications

- Per-style size is dominated by the filmstrip; budget by styles (about 7 sprite styles at 2x is about 5 MB PNG), not by control count.
- 2x GPU memory is about 49 MB for this estimate: fine, but atlas packing should target 2048 or 4096 square, and 1x only on low-DPI.
- The render script is the only source: no `.blend` is committed because the scene is fully generated.

## Follow-ups

- Reduce filmstrip size (palette quantisation, fewer samples with a better denoise, or 32-frame strips for small knobs).
- Add buttons, drawbars, LED lenses and key sprites; verify counts in B2.
- egui side-by-side at 1x and 2x once B4 exists; `xtask bake-panel` replaces `tools/panel_assets.py` packing.
- Calibrate camera tilt and light against the manual photos in B8.

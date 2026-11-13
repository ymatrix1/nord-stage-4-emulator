# Research: Blender panel proof v3 (matched to the Organ section reference)

- **Checked date:** 2026-11-13 (v1 2026-11-12, PR 84; v2 PR 109; v3 matches the renders feature by feature against one reference image)
- **Sources:** `assets/blender/build_proof.py` (stages), `proof_scene.py` (camera, light, world), `proof_materials.py`, `proof_geom.py`, `parts_knob.py`, `parts_cap.py`, `parts_button.py`, `parts_plate.py` (scene), `tools/panel_assets.py` (pack, seamless tiles, hash, sizes), `tools/panel_preview.py` (composition), `tools/panel_compare.py` (look comparison and metric), `assets/panel/theme_samples.json` (measured numbers), [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering), [D16](../DECISIONS.md), [visual-style-and-panel.md](../specs/visual-style-and-panel.md). The owner's private reference is a 1961 x 1185 render of the Master Level, Rotary Speaker and Organ section. It was read only for proportions, positions, colour numbers and material response; no pixels or geometry are used or committed (D16).
- **Question:** how close can scripted, original Blender sprites get to that reference at the same feature scale, and what do they cost at 1x and 2x?

## Reproduce

```bash
python tools/panel_assets.py render    # Blender 5.2 headless -> assets/panel/proof/_raw (git-ignored)
python tools/panel_assets.py pack      # filmstrips, seamless tiles, manifest.json, size report
python tools/panel_assets.py preview   # assets/panel/proof/preview_2x.png from the committed layers
python tools/panel_assets.py verify    # CI-safe: committed PNGs match manifest hashes
python tools/panel_compare.py --ref <NS4-Panel-organ.png> --out .reference/panel-compare-v3.png
```

`BLENDER=<path>` overrides the install path. Look development: `blender -b --factory-startup -P assets/blender/build_proof.py -- --out <dir> --quick [--only knob,caps,buttons,plates]` renders 2x only, 48 samples, six knob frames (about 25 s). Blender output is not bit-identical across machines, so the committed PNGs are the source of truth; packing and the seamless step are deterministic (tested).

## Method

1. Scale. The reference knob base is 103.4 px across (circle fit to the ring and silhouette extents), which is 0.8 Blender units, so the reference has 129 px per unit and our 2x sprites (128 px per unit) sit at the same feature scale. `panel_compare.py` crops each reference feature to the panel area our sprite covers, so rows compare 1:1.
2. Positions. Feature centres come from a circle fit (Master Level ring), silhouette extents (caps, knobs) and a small offset search maximising grey-level correlation (buttons, white cap, layer cap). Only the centre numbers are kept, in `panel_compare.FEATURES`.
3. Colours. Medians of flat patches (`theme_samples.json`), and the material gains are solved by rendering, taking the median of the same flat face and multiplying the linear albedo by target / rendered until each channel lands (`GAIN` in `proof_materials.py`).
4. Look. Each part was iterated by side-by-side at 2x zoom (ours left, reference right) plus mean absolute difference (MAD) per feature crop.

## What changed from v2 and why

| Owner remark / measured gap | v3 |
|---|---|
| Reference is a near top-down perspective render; each control leans by its position | **Camera:** perspective, looking straight down from 13.5 u (1750 reference px) above the panel. The lean is a camera offset per sprite class (`panel_assets.LEAN`) with a lens shift, so the panel plane stays undistorted and plate, ring, shadow and part layers stay aligned. Height fitted from the Master Level top face (23 px off its base at 848 px from the image centre for a 0.4 u cap). Offsets: knob = Master Level, drawbar = between the 16' and 8' caps, layer = layer fader A, button = ORGAN. Evidence: the one fixed 30 deg orthographic camera of v2 showed a front view of every part; the reference shows tops and only the side facing away from the centre |
| Drawbar caps: sloped side wall, larger soft bevels, brighter edge highlights | Cap lofted from cross-sections: back chamfer, long flat top, a slope that narrows into a lower nose shifted left, then a low front skirt. Side walls lean in 5 %, bevel 0.07 u with 8 segments. Edge highlights come from the glossy environment below. Cap 0.78 x 1.79 u (100 x 231 reference px) |
| Black and white caps need correct shading contrast | Sun made diffuse-only (no specular lobe on flat tops) and a shadowless glossy "ring softbox" (`GLOSS_RING`, 35 to 46 deg elevation) lights only reflections, so tops stay matte while rounded edges and knob rims glint. Diffuse ambient is unchanged from v2. The sun is still the only shadow-casting light (D15) |
| Knob top soft highlight, tapered bent pointer | Wall and shoulder re-profiled (7 rings), grooves run up to the rim, top face slightly domed; pointer is a ribbon that follows the surface from near the centre over the shoulder and down the wall, tapering 15 % |
| Arc ring cleaner and brighter | Ring fitted to the reference: outer radius 1.31 x cap radius (67.7 px), thickness 3 to 15 px linear over 270 deg; flat inlay, no denoiser, calibrated to (246, 244, 242) |
| DRIVE knob numbered scale | New layer `knob_scale`: numerals 0 to 10 on the 270 deg arc, Blender's built-in font (freely licensed), emboldened |
| Grey and black pill buttons with wells | New sprites `button_grey`, `button_black`: rounded base 1.05 x 0.5 u in a black well, raised stadium pill 0.76 x 0.36 u (135 x 64 and 98 x 46 reference px) |
| Plates: leather grain on red, speckled slate, light-silver header | Procedural pebble (smooth voronoi bump in object space) plus noise and speckle; rendered without the denoiser, then made seamless by a renormalised half-offset cross-fade (`make_seamless`). New tiles `plate_slate`, `plate_silver` |
| v2 textures were nearly invisible | Root cause: procedural textures used bounding-box generated coordinates, so "scale 90" meant 90 cells per 4 u plate. Now object coordinates, scales are per unit |

## Setup

| Item | Value | Tag |
|---|---|---|
| Camera | perspective, straight down, 13.5 u high, lens shift per sprite; lean offsets in `panel_assets.LEAN` | `EMPIRICAL` (fitted from the Master Level lean); the reference image centre as optical axis is `ASSUMED` |
| Key light | one sun, top-left, elevation about 61 deg, 22 deg angular size, diffuse only | `ASSUMED` (D15) |
| Ambient | uniform diffuse ambient 0.58 (v2); glossy-only ring 35 to 46 deg up, strength 2 | `EMPIRICAL` (tuned on edge highlights at the same feature size) |
| Scale | 64 px per unit at 1x, 128 at 2x; reference 129 | `EMPIRICAL` |
| Knob | base 0.8 u, height 0.4 u, 20 flutes, top face 0.74 R, pointer 0.11 of the diameter | `EMPIRICAL` except height (`ASSUMED`) |
| Materials | procedural only; gains solved per material against `theme_samples.json` | `EMPIRICAL` |

## Sizes (committed PNG, 8-bit RGBA)

| Asset | 1x px | 1x bytes | 2x px | 2x bytes |
|---|---|---|---|---|
| `knob_filmstrip` | 768x768 | 444,580 | 1536x1536 | 1,293,106 |
| `knob_ring` | 96x96 | 5,321 | 192x192 | 15,006 |
| `knob_scale` | 96x96 | 4,238 | 192x192 | 11,893 |
| `drawbar_cap_black` | 96x160 | 10,100 | 192x320 | 30,543 |
| `drawbar_cap_white` | 96x160 | 10,199 | 192x320 | 30,821 |
| `drawbar_shadow` | 96x160 | 8,917 | 192x320 | 24,941 |
| `layer_cap_grey` | 96x96 | 4,936 | 192x192 | 13,437 |
| `layer_shadow` | 96x96 | 5,914 | 192x192 | 16,005 |
| `button_grey` | 80x48 | 4,526 | 160x96 | 12,764 |
| `button_black` | 80x48 | 4,003 | 160x96 | 11,910 |
| `plate_red` | 128x128 | 22,820 | 256x256 | 86,350 |
| `plate_slate` | 128x128 | 17,967 | 256x256 | 75,574 |
| `plate_silver` | 128x128 | 22,541 | 256x256 | 87,398 |
| `plate_slate_slot` | 128x256 | 34,285 | 256x512 | 133,736 |
| **Whole proof set (one knob style)** | | **600,347** | | **1,843,484** |

Full-panel estimate (`ASSUMED`: 4 knob styles, each one more filmstrip; button down and lit states counted in GPU memory only):

| | PNG on disk | RGBA in GPU memory |
|---|---|---|
| 1x | 1.93 MB | 10.2 MB |
| 2x | 5.72 MB | 40.8 MB |

v2 was 1.82 MB / 5.13 MB PNG; the rise is the new sprites (scale layer, layer cap, buttons, two more tiles) and the visible plate grain.

## How close, and what is still different

Private comparison sheet (not committed): `.reference/panel-compare-v3.png`, ten rows, ours left and the reference right at the same feature scale, 2x zoom. Mean absolute difference per crop (0..255, includes background, text and LEDs that are out of scope, so it is a trend number, not a score):

| Feature | MAD |
|---|---|
| master_knob | 26.4 |
| drive_knob | 25.1 |
| drawbar_black | 23.9 |
| drawbar_white | 31.3 |
| layer_grey | 31.7 |
| button_grey | 17.1 |
| button_black | 18.7 |
| plate_red | 6.9 |
| plate_slate | 11.4 |
| plate_silver | 13.3 |

Calibrated flat faces (checked by `test_committed_sprites_match_sampled_colours`): plates red, slate, silver within 6 per channel; white cap, grey button within 6; black cap, grey layer cap within 8.

Close now: knob silhouette, lean and ring position; ring width profile and brightness; pointer shape and bend; flute count and rim; DRIVE numeral layout; drawbar cap outline (top, slope, nose shifted left), cap proportions, black, white and grey tones, soft shadows; button proportions, well and pill relief; plate colours and grain.

Still different:

- **Lean is per class, not per position.** The reference camera makes every control lean by its own position; one sprite per class carries one lean (for example the drawbar sprite leans like a cap just left of centre, while the reference 1' cap at the far right leans the other way). A per-position lean would need several sprite variants or a vertex-shader skew; not done.
- **Drawbar caps.** Edge highlights are softer and fainter than the reference's crisp lines, most visibly on the front edge of the top face; the slope into the nose is a straight loft rather than the reference's curved transition; the front skirt reads lighter than the reference's near-black shadowed foot.
- **Layer cap.** Our grey cap is smoother; the reference has a coarse speckled plastic and a rounder pillow shape.
- **Knob.** The reference flutes have sharper facet ridges and the top rim has small notches; the reference top face is a hair darker in the upper left.
- **Buttons.** The black button reads slightly lighter than the reference (its glossy rim lifts the median); the reference pill has a crisper rim highlight.
- **Plates.** Grain scale and contrast are close; the reference red has finer light specks. Plate panel seams, screws and silkscreen are out of scope (vector or dynamic layers).
- **Numerals** use Blender's built-in font, not the hardware's typeface (by design, D16).

## Implications

- Budget by styles (about 4 knob styles, 3 cap shapes, 2 button colours): see the size table; the filmstrip dominates.
- Ring and scale are separate layers, so the app can colour, mask or replace them with vector drawing.
- Colours come from `theme_samples.json`; B8 still has to reconcile the spec tokens with it (decision for the owner).

## Follow-ups

- Lean variants per panel region (left, centre, right) or a skew in the app, if the owner wants the per-position lean.
- Button down and lit states; LED lenses; drawbar variants; keys.
- Reduce filmstrip size (palette quantisation, 32 frames for small knobs).
- egui side-by-side at 1x and 2x once B4 exists; `xtask bake-panel` replaces `tools/panel_assets.py` packing.

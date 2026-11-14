# Research: Blender panel proof v2 (knob + arc ring, fader caps, atlas size)

- **Checked date:** 2026-11-13 (v1 2026-11-12, PR 84; v2 reworks it after the owner found it too far from the hardware)
- **Sources:** `assets/blender/build_proof.py`, `proof_parts.py`, `proof_materials.py` (scene), `tools/panel_assets.py` (pack, hash, measure), `tools/panel_compare.py` (look comparison), `assets/panel/theme_samples.json` (measured numbers), [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering), [D16](../DECISIONS.md), [visual-style-and-panel.md](../specs/visual-style-and-panel.md). The owner's private reference images (an Organ section render and a Master Level knob close-up) were read only for proportions, colour numbers and material response; no pixels or geometry are used or committed (D16).
- **Question:** can a scripted headless Blender pipeline produce original sprites that read as close to the hardware as possible, and how big is the atlas set at 1x and 2x?

## Reproduce

```bash
python tools/panel_assets.py render    # Blender 5.2 headless -> assets/panel/proof/_raw (git-ignored), about 2.5 min
python tools/panel_assets.py pack      # stitch 8x8 filmstrips, write manifest.json, print sizes
python tools/panel_assets.py preview   # assets/panel/proof/preview_2x.png (composed from the committed layers)
python tools/panel_assets.py verify    # CI-safe: committed PNGs match manifest hashes
python tools/panel_compare.py --knob-ref <knob.png> --caps-ref <organ.webp>   # private side-by-side into .reference/
```

`BLENDER=<path to blender.exe>` overrides the default install path. For look development, `blender -b --factory-startup -P assets/blender/build_proof.py -- --out <dir> --quick` renders only 2x, 48 samples and five knob frames (12 s). Blender output is not bit-identical across machines, so the committed PNGs are the source of truth; packing is deterministic (tested).

## What changed from v1 and why

| Problem the owner found | Cause | v2 |
|---|---|---|
| Third grip line on the fader cap wrong (short, offset on the bevel) | Grip boxes were laid on a sloped face and clipped by the bevel | Removed. The reference caps have no grip lines at all; they are smooth chunky blocks. Nothing left to misalign, and 1x/2x silhouettes are tested to match |
| Fader cap a grey slab | Single sloped wedge, one colour | Stepped block: rounded rear shoulder, long flat top, bevel ledge, a lower nose block set forward and shifted left, as on the reference. Three colour sprites (black, white, grey) share one shadow layer |
| Knob busy and red-tinted skirt, flat top, small pointer, no ring | Skirt ring and 36 fine ribs, red plate reflected in rubber, 0.05 wide pointer | No skirt; 20 flutes split by narrow grooves, rounded shoulder, satin top; pointer 0.14 of diameter, bar on top bending down the side; rubber no longer picks up the plate (catcher ground is plain grey) |
| No arc ring | Not modelled | Separate transparent layer `knob_ring_*`: white arc, 270 deg sweep, thin start to thick end, lifted to cap-top height so it centres on the cap like the reference. The app can draw it live instead |
| Plate colours off | Spec tokens used, then lifted by ambient and specular | Colours measured from the reference (`theme_samples.json`) and the plates calibrated to land on them |
| Shadows hard | 55 deg sun, little ambient | 61 deg sun, 22 deg angular size, ambient 0.58: soft, short, one top-left light |

## Setup

| Item | Value | Tag |
|---|---|---|
| Light | one sun, top-left, elevation about 61 deg, 22 deg angular size, strength 1.7; ambient world 0.58 | `ASSUMED` (single top-left light fixed by D15) |
| Camera | orthographic, tilted 30 deg toward the player | `ASSUMED`: the reference is a near top-down perspective render (the arc ring reads almost circular, 0.93) yet cap sides and fronts show a lot; 30 deg is the compromise that keeps both readable |
| Scale | 64 px per Blender unit at 1x; 2x doubles | `EMPIRICAL` |
| Knob | cap diameter 0.8 u, height 0.54 of diameter (0.6 read on the reference, trimmed for the lower tilt), 20 flutes, pointer 0.14 of diameter, ring outer 1.25 of cap radius, thickness 0.07 to 0.20 of diameter, sweep 270 deg | proportions `EMPIRICAL`, see `theme_samples.json` |
| Knob rotation | 64 frames, -135 to +135 deg clockwise | `EMPIRICAL`: Rotary Speaker `DRIVE` scale runs lower-left to lower-right |
| Fader cap | width 0.62 u (0.85 of knob diameter), slot 0.18 of cap width | `EMPIRICAL` |
| Materials | procedural only: leather-grain red, speckled anodised slate, satin rubber, satin plastic caps | `ASSUMED` shader values, colours measured |

Measured colours (sRGB 8-bit medians of small flat patches, numbers only; carry baked lighting so they are appearance targets): red panel (134, 31, 41), dark slate (55, 64, 76), header silver (179, 191, 205), black cap top (57, 53, 50), white cap (233, 232, 228), grey cap (155, 151, 147), knob top (63, 59, 55), knob sides (47, 41, 39), ring white (250, 244, 242). The spec tokens `#8E1B1F` (142, 27, 31) and `#323D47` (50, 61, 71) differ by up to 10 levels per channel from these; the renders follow the measurements. Rendered plate medians: red (133, 34, 42), slate (55, 63, 74), each channel within 6 of the target (checked by `test_committed_plates_match_sampled_colours`).

## Sizes (committed PNG, 8-bit RGBA)

| Asset | 1x px | 1x bytes | 2x px | 2x bytes |
|---|---|---|---|---|
| Knob filmstrip, 64 frames, 8x8 grid | 768x768 | 433,541 | 1536x1536 | 1,211,104 |
| Knob arc ring | 96x96 | 4,264 | 192x192 | 11,432 |
| Fader cap black / white / grey | 96x96 | 5,637 / 5,737 / 5,677 | 192x192 | 17,063 / 16,976 / 17,131 |
| Fader shadow (own layer, shared) | 96x96 | 5,918 | 192x192 | 16,323 |
| Red plate | 128x128 | 14,152 | 256x256 | 56,589 |
| Slate plate with slot | 128x256 | 28,210 | 256x512 | 119,584 |
| **One knob style + all three caps + shadow + plates** | | **503,136** | | **1,466,202** |

Full-panel estimate (`ASSUMED`: 45 knobs, 30 faders, 150 buttons until B2 verifies counts; 4 knob styles, each one filmstrip plus ring; the three cap colours already cover the fader styles; buttons not rendered yet, counted in GPU memory only as 3 sprites of 64 px square):

| | PNG on disk (excl. buttons) | RGBA in GPU memory (incl. buttons) |
|---|---|---|
| 1x | about 1.82 MB | about 10.0 MB |
| 2x | about 5.13 MB | about 39.9 MB |

v1 estimate was 1.56 MB / 4.62 MB; the rise is the richer fluted cap and the soft shadow in each knob frame, plus the ring and two more cap sprites. Knobs of one style share a filmstrip, so the number of knobs does not change size; only the number of distinct styles does.

## How close, and what is still different

Compared side by side at the same feature size (private sheet, `.reference/panel-compare-v2.png`, not committed):

- Close: overall block shape of the drawbar caps (rear shoulder, long top, ledge, offset nose), black/white/grey tones, plate colours, ring size and thin-to-thick taper, flute count and knob proportion, soft single-light shadows, leather grain on the red.
- Still different:
  - **Camera.** The reference is a perspective render looking almost straight down, so each control leans a little depending on where it sits on the panel; ours is one orthographic 30 deg camera. Cap sides and the ring centre are therefore approximations.
  - **Cap detail.** Reference caps have a sloped side wall and slightly softer, larger bevels; ours has hard box walls with a 0.03 bevel. The reference black cap is a touch darker with brighter bevel highlights.
  - **Knob top.** The reference has a visible soft highlight on the dished top and a bent, tapered pointer; ours is a flat satin disc with a straight bar.
  - **Ring.** The reference ring is cleaner and brighter at the thick end; ours shows slight denoise texture at 1x.
  - **Plates.** Ours are small flat tiles with procedural grain, not yet seamless tiles; no bevelled seams, screws or silkscreen (dynamic or vector layers).
- File size: the knob filmstrip is about 1.2 MB at 2x; palette quantisation is not applied (lossless).

## Implications

- Budget by styles (about 4 knob styles, 3 cap colours): roughly 5 MB PNG and 40 MB GPU at 2x.
- Because the ring is its own layer, the app can colour it, mask it to the live value, or draw it vector; the baked ring is only the default look.
- Colours should come from `theme_samples.json`; B8 should also reconcile the spec tokens with it (decision for the owner).

## Follow-ups

- Reduce filmstrip size (palette quantisation, 32 frames for small knobs).
- Seamless plate tiles; buttons, drawbar variants, LED lenses, keys.
- Consider a per-section camera or a small perspective offset table if the owner wants the reference lean.
- egui side-by-side at 1x and 2x once B4 exists; `xtask bake-panel` replaces `tools/panel_assets.py` packing.
- Soften cap walls (sloped side) and add a top highlight map to the knob.

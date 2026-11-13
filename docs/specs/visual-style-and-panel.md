# Front-Panel Visual Style & UI Specification

## 1. Scope

Specifies the 1:1 visual layout, color palette, typography, hardware widgets, LED indicators, OLED displays, and interactive behaviors for the Nord Stage 4 Emulator GUI (Standalone & CLAP/VST3 DAW Plugin).

Derived directly from the hardware reference images in `.reference/`:
- `foto.jpg` (Full Nord Stage 4 Hammer Action 88 top panel & keybed)
- `NS4-Panel-organ.max-2000x2000.format-png.png` (2000px close-up of Master, Rotary Speaker, and Organ Section)
- `Screenshot20at2015.jpg` (Close-up of Piano Section)
- `12916-Lg.jpg` (Front-panel PCB showing physical drawbar sliders, LED ladders, LED-ring encoders, and OLED module)

---

## 2. Color Palette & Surface Tokens

The Nord Stage 4 panel alternates between **Nord Signature Crimson Red** (`#8E1B1F`) for global/master/program/rotary/FX-focus zones and **Anodized Slate Blue-Grey** (`#323D47`) for the four sound/effect module bays (`ORGAN`, `PIANO`, `SYNTH`, `LAYER EFFECTS`), topped by a **Brushed Silver-Blue Header Bar** (`#A3AFB9`).

| Token | Hex | Usage |
|---|---|---|
| `chassis_red_primary` | `#8E1B1F` | Outer chassis, Left Master/Rotary strip, Central Program/Morph bay, FX Focus strip |
| `chassis_red_dark` | `#641014` | Recessed borders, end-cheeks shadow, chamfered panel seams |
| `module_slate_bg` | `#323D47` | Main speckled metallic background of `ORGAN`, `PIANO`, `SYNTH`, and `LAYER EFFECTS` sections |
| `module_header_bar` | `#A3AFB9` | Top header strip of each module (`ORGAN SECTION`, `PIANO SECTION`, `SYNTH SECTION`, `LAYER EFFECTS`) |
| `group_frame_border` | `#9CA8B3` | 1.5 px rounded-corner outline boxes (`ORGAN MODEL`, `VIB/CHORUS`, `B3 PERCUSSION`, `PIANO SELECT`, `MOD 1`, `MOD 2`, `AMP SIM/EQ`, `DELAY`, `REVERB`) |
| `button_grey_layer` | `#8C9196` | Primary section/layer toggle buttons (`A`, `B`, `C`, `ON`, `ORGAN`) |
| `button_black_pill` | `#1C1F22` | Mode/selector pill buttons (`MODEL`, `VIB/CHORUS`, `PERC`, `OCTAVE SHIFT`, `TIMBRE`) |
| `button_store_red` | `#B8252A` | Recessed `STORE` button in the central Program bay |
| `led_red_active` | `#FF2A2A` | Active state LEDs, 8-segment drawbar ladders, model/category triangle indicators |
| `led_green_meter` | `#5CE62E` | 11-segment layer level fader ladders, `ON/OFF` halo rings, `KB ZONE` split indicators, `SLOW` rotary LED |
| `led_amber_morph` | `#FFB020` | Morph assignment preview / secondary status |
| `oled_bg` | `#080A0C` | High-contrast monochrome OLED screen background |
| `oled_fg` | `#F0F6FC` | Crisp white/ice-blue pixel text & waveform graphics on both OLED displays |
| `drawbar_white` | `#ECEBE6` | Harmonic footage caps for `8'`, `4'`, `2'`, `1'` |
| `drawbar_black` | `#1A1C1E` | Sub/mutation footage caps for `16'`, `5 1/3'`, `2 2/3'`, `1 3/5'`, `1 1/3'` |

---

## 3. Left-to-Right Panel Architecture (6 Horizontal Bays)

The top panel is laid out horizontally above the 88-key triple-sensor keybed (with 3 split-point red LEDs above the keybed lip):

```text
┌──────────┬────────────────────┬───────────────────┬────────────────────┬─────────────────────────┬───────────────────────┐
│  1. LEFT │  2. ORGAN SECTION  │ 3. PIANO SECTION  │ 4. PROGRAM / MORPH │    5. SYNTH SECTION     │   6. LAYER EFFECTS    │
│  MASTER  │   (Slate #323D47)  │  (Slate #323D47)  │   (Red #8E1B1F)    │     (Slate #323D47)     │    (Slate #323D47)    │
│ & ROTARY │                    │                   │                    │                         │                       │
│ (#8E1B1F)│ • Faders A & B     │ • Faders A & B    │ • MORPH ASSIGN     │ • Faders A, B & C       │ • FX FOCUS (Org/Pno/  │
│          │   + 11-LED ladders │   + 11-LED ladders│   (Wheel/AT/Pedal) │   + 11-LED ladders      │   Syn A/B/C, Group)   │
│ • Pitch  │ • ORGAN MODEL      │ • ACOUSTICS       │ • SPLIT (3 zones)  │ • SYNTH OLED DISPLAY    │ • MOD 1 (Rate/Amount) │
│   Stick  │   (B3/Vox/Farf/    │   (String Res,    │ • MST CLK / TRANSP │   (Waveform, Detune,    │ • MOD 2 (Rate/Amount) │
│ • Mod    │    Pipe1/Pipe2)    │    Pedal Noise,   │ • MAIN OLED SCREEN │    Mode soft-tabs)      │ • AMP SIM / 3-Band EQ │
│   Wheel  │ • VIB/CHORUS       │    Soft Rel)      │   (Program #/Name, │ • MODE (Sample/Analog/  │   (Drive, Bass, Mid   │
│ • Master │   (V1-V3, C1-C3)   │ • UNISON (1/2/3)  │    Layer Summary)  │    Wavetable/FM)        │    Gain+Freq, Treble) │
│   Level  │ • B3 PERCUSSION    │ • KB TOUCH        │ • LIVE MODE        │ • ARPEGGIATOR / GATE    │ • DELAY (Tempo,       │
│ • Rotary │   (Soft/Fast/3rd)  │ • DYN COMP        │ • LAYER SCENE II   │ • LFO & OSCILLATORS     │   Feedback, PingPong, │
│   Drive  │ • 9 PHYSICAL       │ • TIMBRE          │ • PROGRAM 1..8     │ • FILTER (LP12/24/Lad/  │   Analog, Filter, Mix)│
│ • Slow / │   DRAWBARS +       │ • PIANO SELECT    │   Selector Buttons │   BP/HP, Freq, Res)     │ • COMP (Amount, Fast) │
│   Fast   │   8-LED RED LADDERS│   + MODEL Encoder │                    │ • AMP ENV & UNISON      │ • REVERB (6 Rooms,    │
│          │ • KB ZONE (4 LEDs) │ • KB ZONE (4 LEDs)│                    │ • KB ZONE (4 LEDs)      │   Bright/Dark, Chorale│
└──────────┴────────────────────┴───────────────────┴────────────────────┴─────────────────────────┴───────────────────────┘
```

---

## 4. Custom Hardware Widget Specifications

Every custom UI widget in `crates/nord-gui` / `crates/nord-plugin` must render the exact hardware cues shown in `NS4-Panel-organ.max-2000x2000.format-png.png` and `Screenshot20at2015.jpg`:

### 4.1 Layer Volume Fader + 11-Segment Green LED Ladder (`LayerFaderWidget`)
- **Physical Fader:** Vertical slot track with a sculpted matte-grey rectangular fader cap (`#8C9196` with top/bottom bevel highlights).
- **11-Segment Green LED Bar (`#5CE62E`):** Positioned immediately to the right of each fader track. Shows the effective layer level (including active **Morphing** offsets in real time!) so the player sees both the physical fader position and the morphed level simultaneously.
- **Layer Activation Button (`A`, `B`, `C`):** Above each grey rectangular button sits a pill label (`ON/OFF ▼`) with a circular green ring LED (`#5CE62E` when active in the current Scene, dim when off, blinking when edited in FX Focus).
- **Zone Indicator (`◄ KB ZONE ►`):** 4 circular green LEDs at the bottom of each section's mixer strip showing which of the 4 split zones the selected layer spans.

### 4.2 Physical Organ Drawbar + 8-Segment Red LED Ladder (`DrawbarWidget`)
- **9 Drawbar Tracks (`16'`, `5 1/3'`, `8'`, `4'`, `2 2/3'`, `2'`, `1 3/5'`, `1 1/3'`, `1'`):**
  - Pulled **down** (`0` at top, `8` fully pulled out at bottom), matching real Hammond B3 / Nord Stage 4 mechanics.
  - Top labels show Pipe/Farfisa/Vox equivalents (`BASS16 / 16'`, `STR16 / 8'`, `FLUTE8 / 4'`, `OBOE8 / 2'`, `TRMP8 / II`, `STR8 / III`, `FLUTE4 / IV`, `STR4`, `2 2/3 ~`).
  - Bottom labels show B3 harmonic footages (`16'`, `5 1/3'`, `8'`, `4'`, `2 2/3'`, `2'`, `1 3/5'`, `1 1/3'`, `1'`).
- **Adjacent 8-Segment Red LED Ladder (`1..8`):**
  - Next to every drawbar slot is a white-framed vertical strip numbered `1` through `8` with 8 rectangular red LEDs (`#FF2A2A`).
  - Lights from the top down (`1..N`) to display the active drawbar registration (and any live Morph transition).
- **Drawbar Cap Colors:**
  - Indices `0, 1` (`16'`, `5 1/3'`): Black (`#1A1C1E`)
  - Indices `2, 3` (`8'`, `4'`): White (`#ECEBE6`)
  - Index `4` (`2 2/3'`): Black (`#1A1C1E`)
  - Index `5` (`2'`): White (`#ECEBE6`)
  - Indices `6, 7` (`1 3/5'`, `1 1/3'`): Black (`#1A1C1E`)
  - Index `8` (`1'`): White (`#ECEBE6`)

### 4.3 Dual-Column Red Triangle Matrix Selector (`TriangleSelectorWidget`)
- Used in `ORGAN MODEL` (`FARF / VOX / B3` vs `PIPE1 / PIPE2 / B3 BASS`), `VIB/CHORUS` (`C2 V3 C3 / V2 C1 V1`), and `PIANO SELECT` (`ELECTRIC / UPRIGHT / GRAND` vs `CLAV / DIGITAL / MISC`).
- Recessed dark bezel containing left/right pointing red LED triangles (`◄` `►`) above a black pill selector button.

### 4.4 Rotary Knobs & Morph Range Arcs (`NordKnobWidget`)
- Dark charcoal ribbed cylindrical cap with a crisp white radial pointer line (or endless stepped encoder with `LIST` badge for `MODEL` / `OSC`).
- **Morph Assignment Visualization:** Holding `WHEEL`, `A.T.`, or `CTRLPED` in the `MORPH ASSIGN` bar (or hovering a morphed knob) draws a glowing arc around the knob perimeter from `base` to `base + delta`, allowing instant visual creation and inspection of Morph ranges just like LED-ring encoders on hardware.

### 4.5 Dual Monochrome OLED Displays (`ProgramOledWidget` & `SynthOledWidget`)
1. **Central Program OLED (`128×64` aspect):**
   - Top row: Bank & Location (`A:11`) + Program Title (`Nord Stage 4`) + Category icon.
   - Bottom 3 rows: Active Organ model/drawbar preview (`888000000`), active Piano model (`White Grand XL`), and active Synth patch (`Vista Pad WH`) with `A/B/C` layer badges.
2. **Synth Section Graphic OLED (`128×64` aspect):**
   - Header: `OSC WAVEFORM`
   - Center graphic: Real-time anti-aliased waveform / wavetable / FM operator visualization + primary parameter readout (`Super Saw`, `DETUNE: 3.4`).
   - Bottom bar: 4 inverted/outlined soft-label tabs (`ANALOG`, `SUPER`, `1 (6)`, `WAVE`) controlled by the 3 encoders below the screen.

### 4.6 Known discrepancies (open, Phase 6b `ns4-xc5n`)

| Item | This spec says | Manual / evidence | Status |
|---|---|---|---|
| Morph target display (§4.4) | Amber glowing arc around the knob | Manual p.11: green Morph LEDs at the bottom-left of the knob show morph assignment | Spec wrong; renderer to follow the manual (B8, [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering)) |
| Fader LED ladder (§4.1) | 11 segments | Not yet checked against manual p.11 | `ASSUMED`; verify in B2 ([control inventory](control-inventory.md)) |

---

## 5. Responsive Layout Modes (Plugin vs Standalone)

To keep every label legible on standard 1080p/1440p/4K monitors and inside DAW plugin windows:
1. **Full Panorama View (`1600×520` aspect, scalable `0.75x..2.0x`):** Shows the entire Nord Stage 4 top panel + 88-key interactive keyboard with split-zone color overlays and pitch/mod wheels.
2. **Focused Rack View (`1100×640` compact DAW mode):** Keeps the Top Master/Program/Morph/Scene bar + 88-key Split bar permanently visible while allowing large, zoomed-in inspection of `[ORGAN + PIANO]` or `[SYNTH + LAYER EFFECTS]` at full 1:1 readability.
3. **Docked `.ns4b` Archive Browser & Knob Recall Mode:** The `.ns4b` library drawer docks as a resizable left `SidePanel` before the front-panel `CentralPanel` (never overlapping hardware controls), with an `Auto-close` toggle (enabled by default) that dismisses the browser upon selecting a Program, Piano, or Sample so the full top panel remains unobstructed. `Sync Knobs on Preset` defaults to `false` to match authentic non-motorized Nord Stage 4 hardware behavior (where OLEDs, `A:11..H:88` location, LED button rings, `fx_focus`, and layer/drawbar LED meters update while potentiometer caps keep their physical angle).

---

## 6. Baked sprite proof (B7, Phase 6b)

Measured output of the D15 baker (v4, matched feature by feature to the owner's private Organ section render; since [D17](../DECISIONS.md#d17-panel-proof-sprites-may-be-derived-from-the-reference-render) buttons, triangle selectors, knob bodies, the DRIVE scale and plate tiles are cut from the reference by `tools/panel_refbake.py`, tagged `REFERENCE-DERIVED (D17)`): a knob filmstrip with two separate overlay layers (arc ring, numbered DRIVE scale), drawbar caps (black, white) with one shared shadow, a grey layer-fader cap with its own shadow, grey and black pill buttons in their wells, three seamless plate tiles (red leather, slate, silver header) and the slotted slate plate. Rendered by `assets/blender/build_proof.py`, committed under `assets/panel/proof/` with SHA-256 hashes in `manifest.json` (`python tools/panel_assets.py verify`). Colours and proportions are numbers measured on the reference, stored in [`assets/panel/theme_samples.json`](../../assets/panel/theme_samples.json) (no pixels, D16). Details, method, sizes and what still differs: [panel-blender-proof.md](../research/panel-blender-proof.md).

| Item | Value | Tag |
|---|---|---|
| Scale | 64 px per Blender unit at 1x, 128 at 2x (reference: 129); knob base 0.8 u | `EMPIRICAL` |
| Camera | perspective, straight down, 13.5 u above the panel; per-class lean as a camera offset with lens shift (`panel_assets.LEAN`) so all layers stay aligned | `EMPIRICAL` (fitted from the Master Level lean) |
| Light | one top-left sun (elevation about 61 deg, soft, diffuse only, the only shadow caster); uniform diffuse ambient; glossy-only ring softbox for edge highlights | sun `ASSUMED` (D15), ambient `EMPIRICAL` |
| Knob filmstrip | 64 frames, 8x8 grid, frame 96 px (1x) / 192 px (2x), transparent, pointer -135 to +135 deg clockwise; 20 flutes, bent tapered pointer | rotation and proportions `EMPIRICAL` |
| Knob arc ring | own layer, 270 deg, outer 1.31 x cap radius, thickness 3 to 15 reference px | `EMPIRICAL` (circle fit) |
| Reference knobs | Master Level and DRIVE: cap body from the reference (pointer removed), Blender pointer filmstrip (64 frames, cap holdout) and shadow at each knob's lean; DRIVE numeral scale layer from the reference | `REFERENCE-DERIVED (D17)` body and scale, pointer `EMPIRICAL` |
| Drawbar caps | black (16'), white (8'); 96 x 160 px (1x); one shared 3D part (block with tapered sides, sloped front, long lower nose offset left, chamfered front, low foot), same camera for both colours, calibrated materials, Blender shadow | shape from the owner's photos of the real part, size `EMPIRICAL` |
| Layer-fader cap | grey; 96 x 96 px (1x); 3D rounded frustum, calibrated material, own Blender shadow | shape from the owner's photo, size `EMPIRICAL` |
| Buttons | all 15 buttons of the reference (black and grey pills, octave pair), up state with well, one sprite per instance (about 68 x 38 px at 1x) | `REFERENCE-DERIVED (D17)` |
| Triangle selectors | ORGAN MODEL and VIB/CHORUS matrices, unlit base plus lit overlay (left, up; right and down mirrored) | `REFERENCE-DERIVED (D17)` |
| Plate tiles | red, slate, silver; 128 px (1x), seamless, quilted from clean reference patches shifted to the measured colours | `REFERENCE-DERIVED (D17)` |
| Size, whole proof set | 824 KB (1x), 2.76 MB (2x) PNG | measured |
| Full-panel estimate (6 knob, 15 button styles) | 2.16 MB (1x), 6.64 MB (2x) PNG | counts `ASSUMED` until B2 |

Note: the sampled colours differ from the §2 tokens (red (130, 27, 38) vs `#8E1B1F`; slate (55, 64, 76) vs `#323D47`; silver (179, 191, 205) vs `#A3AFB9`). The sprites follow the samples; which one becomes the theme token is open for B8. Still different from the reference (summary): buttons and selectors match up to resampling. The knobs differ in the ring (full track vs lit value), the static fluted body and a slightly softer pointer bend. Each cap colour is one sprite, so it keeps the lean of the drawbar it was cut from. Full per-style list in the research note.

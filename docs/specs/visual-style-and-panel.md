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
   - Edited marker: a boxed `E` follows the location while the program differs from what was loaded or stored (VERIFIED, course footage and manual p.13). The panel counts the sound edits it sends (`AudioHandle::edit_count`, `AudioCommand::is_program_edit`) and compares the count with its value at the last load, program change or STORE (`bay_program/edited.rs`). Setters sent with the same value still count (ASSUMED). In Live Mode the title is `Live N` and no `E` shows (ASSUMED: Live programs auto-store).
   - Section rows (Layers view): one row per section with a layer on (`Organ B3`, `Piano ...`, `Synth ...`). Hardware also prints the preset id (`1:05`) or source name with a size letter and mini drawbars; those are not modelled yet.
   - Bottom 3 rows: Active Organ model/drawbar preview (`888000000`), active Piano model (`White Grand XL`), and active Synth patch (`Vista Pad WH`) with `A/B/C` layer badges.
2. **Synth Section Graphic OLED (`128×64` aspect):**
   - Header: `OSC WAVEFORM`
   - Center graphic: Real-time anti-aliased waveform / wavetable / FM operator visualization + primary parameter readout (`Super Saw`, `DETUNE: 3.4`).
   - Bottom bar: 4 inverted/outlined soft-label tabs (`ANALOG`, `SUPER`, `1 (6)`, `WAVE`) controlled by the 3 encoders below the screen.

### 4.6 Known discrepancies (open, Phase 6b `ns4-xc5n`)

| Item | This spec says | Manual / evidence | Status |
|---|---|---|---|
| Morph target display (§4.4) | Amber glowing arc around the knob | Manual p.11: green Morph LEDs at the bottom-left of the knob show morph assignment | Spec wrong; renderer to follow the manual (B8, [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering)) |
| Fader LED ladder (§4.1) | 11 segments | Manual text does not state the LED count (p.18, 23, 27 only say "LED graph"); the drawbar ladders are 8 LEDs (p.21) | `ASSUMED`; needs a panel-photo count ([control inventory](control-inventory.md)) |
| Split-point LEDs (§3, keybed) | 3 red LEDs | Manual p.15: green LEDs above the keyboard, plus L/M/H LEDs above the SPLIT ON/SET button | Spec wrong; inventory records both (`keybed.split_leds`, `split.leds`) |

---

## 5. Responsive Layout Modes (Plugin vs Standalone)

To keep every label legible on standard 1080p/1440p/4K monitors and inside DAW plugin windows:
1. **Full Panorama View (`1600×520` aspect, scalable `0.75x..2.0x`):** Shows the entire Nord Stage 4 top panel + 88-key interactive keyboard with split-zone color overlays and pitch/mod wheels.
2. **Focused Rack View (`1100×640` compact DAW mode):** Keeps the Top Master/Program/Morph/Scene bar + 88-key Split bar permanently visible while allowing large, zoomed-in inspection of `[ORGAN + PIANO]` or `[SYNTH + LAYER EFFECTS]` at full 1:1 readability.
3. **Docked `.ns4b` Archive Browser & Knob Recall Mode:** The `.ns4b` library drawer docks as a resizable left `SidePanel` before the front-panel `CentralPanel` (never overlapping hardware controls), with an `Auto-close` toggle (enabled by default) that dismisses the browser upon selecting a Program, Piano, or Sample so the full top panel remains unobstructed. `Sync Knobs on Preset` defaults to `false` to match authentic non-motorized Nord Stage 4 hardware behavior (where OLEDs, `A:11..H:88` location, LED button rings, `fx_focus`, and layer/drawbar LED meters update while potentiometer caps keep their physical angle).

---

## 6. Baked sprite proof (B7, Phase 6b)

First measured output of the D15 baker: one knob filmstrip and one fader cap with a separate shadow, rendered by `assets/blender/build_proof.py`, committed under `assets/panel/proof/` with SHA-256 hashes in `manifest.json` (`python tools/panel_assets.py verify`). Details and sizes: [panel-blender-proof.md](../research/panel-blender-proof.md).

| Item | Value | Tag |
|---|---|---|
| Knob filmstrip | 64 frames, 8x8 grid, frame 96 px (1x) / 192 px (2x), pointer -135 to +135 deg clockwise | rotation `EMPIRICAL` (rotary scale on reference photo, measurement only) |
| Fader cap | 96 px / 192 px sprite, separate shadow sprite of the same size | `ASSUMED` geometry (original) |
| Light | one top-left sun, elevation about 55 deg | `ASSUMED`, D15 |
| Camera | orthographic, 14 deg tilt toward player | `ASSUMED` |
| Size, one of each asset | 420 KB (1x), 1.28 MB (2x) PNG | measured |
| Full-panel estimate (4 knob styles, 3 fader styles) | 1.56 MB (1x), 4.62 MB (2x) PNG | counts `ASSUMED` until B2 |

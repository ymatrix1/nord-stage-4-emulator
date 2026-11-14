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

#### Parameter hint line (`ns4-wxly`)

While a knob moves, both OLEDs show `<label> <value>` (`LFO Rate 0.08 Hz`, `LFO Amount 7.1`, `Filter Frequency 1.2 kHz`) for 1.5 s after the last movement (hold time `ASSUMED`, no timing measured).

| Display | Where the hint goes | Source |
|---|---|---|
| Synth OLED | replaces the page title in the header row (right-hand layer letter stays) | LFO course video 0:27-0:45 (EMPIRICAL) |
| Program OLED | replaces the rows below the separator on the home screen; menus, STORE, LIST and split screens keep their text | manual p.42 (VERIFIED) |

Data: `panel/param_hint/table.rs` maps an inventory control id to label and value formatter (`fmt.rs`); every `widgets::knob` drag records `(id, position)` in the egui context (`param_hint::touch`), owners with a non-linear value (Master Clock divisions, stored layer value, per-type mod rate) refine it with `touch_value`. The LFO DEST button taps raise `LFO Destination <Filter|Osc Pitch|OscCtrl|Off>`.

| Item | Status |
|---|---|
| Labels `Oscillator Ctrl`, `Oscillator Env Amount`, `Filter Frequency`, `Filter Resonance`, `Filter Env Amount`, `LFO Destination`, `LFO Rate`, `LFO Amount`, `Glide Rate`, `Arpeggiator Rate`, `Arpeggiator Range`, `Filter`, `Osc Pitch`, `OscCtrl` | VERIFIED in the firmware pool, `tools/tests/test_ns4_oled_strings.py::test_param_hint_labels_are_in_the_firmware` |
| Effects and Master labels (`Amp Bass`, `Delay Feedback`, `Master Level`, ...), `Off` | ASSUMED: not firmware strings; built from the panel silkscreen |
| Hz precision (2 decimals below 1 Hz, 1 below 10 Hz, 0 above) | EMPIRICAL: LFO clip overlay readings, [dsoundman-synth-lfo](../research/dsoundman-synth-lfo.md) section 2 |
| `dB` with one decimal, `bpm` integer | VERIFIED unit strings (`%.1f dB`, `%d bpm`, `0x2CA110..`) |
| Panel-unit scale `0..10` (osc ctrl, resonance, LFO amount), `-10..10` (env amounts) | EMPIRICAL for LFO amount (`7.1`, one reading); ASSUMED for the others |
| Knob laws (log or linear sweeps and their ranges) | ASSUMED, as in `ui_state/synth/pots.rs` |
| Open | The hardware LFO RATE table (`LFO_RATE_HZ_TABLE`, 128 steps) is not used for the hint: it shows the emulator's own rate (log 0.05..40 Hz); moving the knob law to the table is a separate change |

#### 4.5.1 Firmware messages and toasts (`bay_program/fw_messages.rs`, bean `ns4-0u5u`)

The Program OLED shows short messages ("toasts") for events. They use the firmware's own strings; every offset is a file offset in `bank0_CODE.bin` (OS v1.66, local only), checked by `tools/tests/test_ns4_oled_strings.py::test_message_strings_are_in_the_firmware`.

| Message | Offset | Shown when | Status |
|---|---|---|---|
| `Piano not found` | `0x2CA180` | a program load finds an enabled piano layer whose model the opened `.ns4b` lacks, or a piano is requested with no archive open | string `VERIFIED`, trigger `ASSUMED` |
| `Sample not found` | `0x2CA198` | same for an enabled synth layer in Sample mode | string `VERIFIED`, trigger `ASSUMED` |
| `Please wait...` | `0x2C75D0` | a piano / sample decode is running (`pending_jobs > 0`); removed when it ends, never hides another message | string `VERIFIED`, trigger `ASSUMED` |
| `Pasted` / `Swapped` | `0x283758` / `0x283760` | Paste / Swap soft buttons with a filled clipboard | `VERIFIED` |
| `Nothing to Paste` / `Nothing to Swap with` | `0x28377D` / `0x2837E5` | Paste / Swap soft button with an empty clipboard | `VERIFIED` |
| `Are you sure?` | `0x2C0E38` | first press of System > Reset all settings (`0x2C0E08`, the string right before it); a second press within 2 s runs it | string `VERIFIED`, pairing with Reset `ASSUMED` (neighbouring strings, caller not traced) |
| `Unable to Paste` + `Memory Protected` | `0x283790` (one string with `\n%c Memory Protected`) | pasting a whole program while Memory Protect is on | string `VERIFIED`, use `ASSUMED` |
| `Memory Protected` | `0x2837A0` (`%c Memory Protected`) | STORE / Organize blocked by Memory Protect | `VERIFIED` |

`%c` in `%c Memory Protected` is the character code `0x89` (`mov r3, #0x89` before the print call at `0xC003D32C` and `0xC003D344`, `VERIFIED` by `python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC003D2E0 0xC003D360`), a glyph of the panel's OLED font. Its shape is not decoded (a padlock is `ASSUMED` from context); the emulator font has no such glyph, so only `Memory Protected` is drawn.

Mechanism: `ProgramUi::set_message` / `show(FwMessage)` store `(text, end_ms)` and `begin_frame(now_ms, ..)` drops the message at `end_ms`, so the lifetime follows the injected frame clock and tests pass a time instead of sleeping. The lifetime `TOAST_MS = 2000` is `ASSUMED`. A message with a line break is drawn as one OLED row per line. `ProgramUi::confirm(Confirm)` is the two-press prompt: the first press shows `Are you sure?` and arms the action for `TOAST_MS`, the same action again inside the window returns `true`. Library loads queue `FwMessage`s in `BackupLibraryState` and the panel poll turns them into toasts.

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
   **Implemented (B12):** the overview scale-to-fits the window; the panorama is the `P` mode below, not a separate fixed-size layout. The rack view is the focus view (`Z`, `1`-`5`).

   | Key / gesture | Effect |
   |---|---|
   | `P` | Panorama on (viewport opens on the focused section, else a centred 2x window) / off |
   | strip drag or click | Centre the viewport on the pointer (`panel/panorama.rs`) |
   | wheel over the strip, `+` / `-` | Zoom the viewport about its centre (scale 0.25..1 of the panel; the egui zoom caps at 4x) |
   | arrows | Pan by 15 % of the viewport |
   | `Esc` | Leave the panorama (then focus, as before) |
   | `Z`, `1`-`5`, section header | Focus a section; clears the panorama |

   The strip (56 logical px high, panel aspect) sits under the detailed view and draws section outlines only. State is `ViewState::panorama: Option<Rect>` in logical panel px; the default view is unchanged. Not persisted: the app persists no view settings (no eframe storage), so a new session starts in the overview with the drawer closed. The library drawer slides in and out (`SidePanel::show_animated`), `L` toggles it; library logic is unchanged.
3. **Docked `.ns4b` Archive Browser & Knob Recall Mode:** The `.ns4b` library drawer docks as a resizable left `SidePanel` before the front-panel `CentralPanel` (never overlapping hardware controls), with an `Auto-close` toggle (enabled by default) that dismisses the browser upon selecting a Program, Piano, or Sample so the full top panel remains unobstructed. `Sync Knobs on Preset` defaults to `false` to match authentic non-motorized Nord Stage 4 hardware behavior (where OLEDs, `A:11..H:88` location, LED button rings, `fx_focus`, and layer/drawbar LED meters update while potentiometer caps keep their physical angle).

---

## 6. Baked sprite proof (B7, Phase 6b)

Measured output of the D15 baker: a knob filmstrip with a separate arc-ring layer, three fader cap sprites (black, white, grey) with one shared shadow layer, and two plates, rendered by `assets/blender/build_proof.py`, committed under `assets/panel/proof/` with SHA-256 hashes in `manifest.json` (`python tools/panel_assets.py verify`). Colours and proportions come from numbers measured on the owner's private reference images, stored in [`assets/panel/theme_samples.json`](../../assets/panel/theme_samples.json) (no pixels, D16). Details, sizes and what still differs from the hardware: [panel-blender-proof.md](../research/panel-blender-proof.md).

| Item | Value | Tag |
|---|---|---|
| Knob filmstrip | 64 frames, 8x8 grid, frame 96 px (1x) / 192 px (2x), transparent (composes on any plate), pointer -135 to +135 deg clockwise | rotation `EMPIRICAL` (rotary scale on reference, measurement only) |
| Knob arc ring | own transparent layer, 270 deg, thin start to thick end, outer 1.25 x cap radius | proportions `EMPIRICAL` |
| Fader caps | black, white, grey; 96 px / 192 px; one shadow sprite of the same size shared by all | shape `ASSUMED` (original), proportions `EMPIRICAL` |
| Plate colours | red (134, 31, 41), slate (55, 64, 76) sampled from the reference; rendered within 6 per channel | `MEASURED` (see note) |
| Light | one top-left sun, elevation about 61 deg, soft | `ASSUMED`, D15 |
| Camera | orthographic, 30 deg tilt toward player | `ASSUMED` |
| Size, one knob style + all caps + plates | 503 KB (1x), 1.47 MB (2x) PNG | measured |
| Full-panel estimate (4 knob styles) | 1.82 MB (1x), 5.13 MB (2x) PNG | counts `ASSUMED` until B2 |

Note: the sampled colours differ from the §2 tokens (red (134, 31, 41) vs `#8E1B1F`; slate (55, 64, 76) vs `#323D47`). The sprites follow the samples; which one becomes the theme token is open for B8.

## Label legibility rules (B8)

Printed text never goes below 7.5 px (`widgets::MIN_FONT_PX`) and must not touch anything it does
not describe. The headless audit in `crates/nord-gui/src/panel/legibility_*.rs` enforces it per
rule (overlap and 0.9 px gap, control bezel, clipping, section edge, group frame, knob ring; table
in [GUI testing](../gui-testing.md)). Layout decisions it led to:

- Rotary Speaker box: the frame extends 52 px left of the button column (the Master area is empty
  there) and 30 px right, capped 3 px inside the Master section. Wide legends (`SLOW/STOP - FAST`,
  title) are centred on the frame, narrow ones on their button. The `rotary.speed_leds` rectangle
  (`ESTIMATED`) is widened from 20 to 33 photo px so the row holds `LED SLOW   LED FAST`.
- Shift legends place their state LED after the measured text, not at a character-count guess.
- Effects captions start 6.5 px below the control (clear of knob shadow and button bezel) with
  8.5 px between stacked lines.
- The Synth ON button prints SOLO to its right (the OLED sits right below it); the OLED soft
  labels use two 9.5 px lines and are kept inside the glass; the vibrato selector no longer repeats
  the VIBRATO heading (SELECT).
- Two-word percussion names (`VOLUME SOFT`, `HARMONIC 3RD`) stack when the neighbour is closer
  than the full name.

# Hardware Control Inventory Specification

Status: B2 delivered (Phase 6b, `ns4-xc5n`, `ns4-3n0d`) · 275 controls checked against the OS v1.6x manual text (2026-11-13); panel-photo check limited to headline counts

## 1. Scope

One machine-checked list of every Nord Stage 4 front-panel control (manual ch. 3-9), shared by the standalone app and the plugin editor through `crates/nord-panel`. The inventory answers, per control: what it is, where the manual documents it, how sure we are, and what it drives.

Sources: Nord Stage 4 User Manual (local copy in `.reference/`, git-ignored, never committed), reference panel photos listed in [visual-style-and-panel.md](visual-style-and-panel.md) §1, and the current code (`crates/nord-gui/src/panel.rs`, `crates/nord-plugin/src/params.rs`).

## 2. Record format

Each control is one `ControlEntry` in `crates/nord-panel/src/inventory/` (one file per section or half-section, each < 400 lines; `all_controls()` returns them in panel order).

| Field | Type | Meaning |
|---|---|---|
| `id` | `String` | Stable dotted id `group.name[.layer]`, lowercase ASCII (e.g. `organ.drawbar.3`, `program.store`). Never renamed once shipped. A string, not an enum, keeps the 274 ids greppable and additive (D16). |
| `section` | `Section` | `Master`, `Organ`, `Piano`, `Program`, `Synth`, `Effects`, `Keybed` |
| `kind` | `ControlKind` | `Knob`, `Encoder`, `Fader`, `Drawbar`, `Button`, `Selector`, `Led`, `LedLadder`, `Oled`, `Wheel`, `Stick`, `Key` |
| `layer` | `Option<Layer>` | `A`/`B`/`C` for per-layer controls; `None` for global |
| `manual_page` | `u16` | Printed page in the OS v1.6x manual (equals the PDF page) where the control is described |
| `provenance` | `Provenance` | `Verified` / `Empirical` / `Assumed` per [provenance-in-code](../../.agents/rules/provenance-in-code.md), with source text |
| `binding` | `Binding` | See §3 |
| `access` | `Access` | `Direct`, `Shift` (function printed below the control) or `Hold` (press and hold) |
| `morphable` | `bool` | Manual lists it as a Morph destination (green Morph LED); 35 controls today |
| `label` | `&str` | Label as printed on the panel or used in the manual |
| `source` | `&str` | Evidence text for the provenance tag |
| `rect_mm` | `Rect` | Layout rect in panel millimetres, **not yet present**: added by the layout step of B8 |

## 3. Binding

```rust
enum Binding {
    Bound(&'static str),      // host param id (nord-plugin) / engine path / dial table / GUI path
    UiOnly(&'static str),     // reason: no audio effect (e.g. Shift, Prog View)
    Unsupported(&'static str) // bean id tracking the missing behaviour
}
```

`Bound` has four forms: a host parameter id (`master_level`, `pianox_unison_{n}`), `engine:<path>` (a public field or method of `nord-core`, section 7b), `dial:<left|middle|right>` (the Synth dial table, section 9b.1) and **`ui:<area>.<name>`**: behaviour that lives in the panel and has no host parameter, such as the Program browser, Store, Undo / Copy / Paste, Morph Assign, the Synth page and shift buttons and the Layer Effects ON buttons. `<area>` is `program`, `synth` or `fx`. A `ui:` control is `Passive` in the plugin editor (`editor/coverage.rs`): drawn and working as in the standalone, no automation.

- `Bound` IDs follow the stable host-parameter contract from the DAW plugin: additive only, never renamed or removed (enforced by B3 `ns4-xhbf`).
- `UiOnly` controls still get a renderer and behaviour (LED state, menus).
- `Unsupported` controls are drawn and visibly marked; the bean id must exist in `.beans/`.

## 4. Checks (tests in `crates/nord-panel/src/inventory/tests.rs`)

| Check | Rule | Status |
|---|---|---|
| Unique ids | Lowercase ASCII, no duplicates | implemented |
| Provenance | Every entry has page 11-53, a label and a source; `Assumed` entries state why | implemented |
| Bindings | Every binding is non-empty; every `Bound` id appears as a quoted literal in the `nord-plugin` sources, or as `<prefix>_<name>[_{n}]` where `<prefix>` is a nested `id_prefix` in `params.rs` and `<name>` a literal `#[id]` (per-layer ids such as `pianox_unison_{n}`), or is `engine:<path>` whose dot-separated fields are `pub` fields of `nord-core` (`engine:split_router.layer_zone_masks`), or is `ui:<area>.<name>` with `<area>` in `program`, `synth`, `fx`; every `Unsupported` bean exists in `.beans/`; the set of remaining `Unsupported` controls is pinned (`the_unsupported_set_is_pinned`) | implemented |
| GUI proof of `ui:` | every `ui:` control is pressed through the painted button path, the commands are applied to a real `Stage4Engine` and its state asserted; each area test fails when a `ui:` id has no proof: `nord-gui` `panel/bay_program/binding_tests.rs` (Program, 30 ids), `panel/ui_state/synth/binding_tests.rs` (Synth, 18), `panel/bay_effects/tests/binding.rs` (Effects, 5) | implemented (`ns4-lu50`) |
| Counts | Per-section, per-kind, per-binding, `Assumed` and `morphable` totals pinned (§5) | implemented |
| Layout completeness | Every inventory id has a layout rect (`layout/tests.rs`) | implemented |
| Geometry | Rects have positive size; each lies inside its section rect and the panel; section rects do not overlap; two rects overlap only when both share a base (`layout::SHARED`) | implemented |
| Renderer | Every control has a renderer | B8 |

## 5. Counts

Source: the manual text (`.reference/Nord Stage 4 User Manual v1.6X-Edition-N.pdf`, ch. 3-9, extracted with `pdftotext -layout`; PDF page = printed page) plus section context. The pins in `inventory/tests.rs` are regression guards for this list; they are not an independent count. Photo check (`foto` at 2000 px, 2026-11-13) confirmed only the headline counts: 2 organ faders + 9 drawbars, 2 piano faders, 3 synth faders, 3 synth OLED dials, 8 Program buttons, 3 Morph Assign buttons, 2 OLEDs. Per-knob and per-LED positions are placed in §7 (layout), with the less certain ones in `ESTIMATED`. Counts of physical positions that only appear in the panel figures (LED counts per ladder, knob rows) are `Assumed`.

| Section | Knobs/encoders | Faders/drawbars | Buttons/selectors | LEDs/ladders | Displays, wheel, stick, keys | Total |
|---|---|---|---|---|---|---|
| Master & Rotary | 2 | - | 6 | 1 | 2 (wheel, stick) | 11 |
| Organ | - | 11 (2 faders + 9 drawbars) | 21 | 18 (7 LEDs + 11 ladders) | - | 50 |
| Piano | 2 | 2 | 19 | 9 (7 LEDs + 2 ladders) | - | 32 |
| Program & Morph | 2 | - | 50 | 3 | 1 (OLED) | 56 |
| Synth | 17 (12 knobs + 5 encoders) | 3 | 41 | 11 (8 LEDs + 3 ladders) | 1 (OLED) | 73 |
| Layer Effects | 17 | - | 34 | 1 | - | 51 |
| Keybed | - | - | - | 1 | 1 (keys) | 2 |
| **Total** | | | | | | **275** |

By binding: `Bound` 208, `UiOnly` 67, `Unsupported` 0 (from the pins in `inventory/tests.rs`; section 10 lists how the last 65 `Unsupported` were decided). By provenance: `Verified` 250, `Assumed` 24, `Empirical` 0 (no panel-photo check yet). Shift or Hold access: 76 controls. Morphable: 35.

PSTICK range (`ns4-dsjv`): there is no separate range control. The Synth strip's `synth.pstick` entry is the printed `PSTICK/RNG` (manual p.28: hold the button and pick a semitone value) and its host binding `ped_pstick_{n}` pairs with `ped_range_{n}` (`1..=12`, default 2). Only the Synth layers honour the range; Organ and Piano `pstick` entries bend a fixed 2 semitones (p.18, p.23). `master.pitch_stick` stays `UiOnly` (the stick is MIDI pitch bend, no host parameter). The totals above did not change.

Modelling choices: a button with its own LED is one entry; standalone LEDs, LED groups and ladders are separate entries; a Shift function printed under a control is its own entry (`access: Shift`); SOLO is both a Hold gesture on each section ON and a dedicated Program button (p.42).

## Gap list

The old code-review gap list is replaced by the `Unsupported` entries, each tied to a bean. After the classification pass of section 10 and SECTION EDIT (`ns4-fqym`, section 10a) none remains. Controls the manual documents that the earlier list missed: organ PRESET LED / Drawbar Live and per-layer vibrato ON, Poly percussion, piano KB TOUCH and DYN COMP, the three Synth OLED dials with INFO/LIST, Mod 2 as a separate unit with its own Variation, Delay effect Variation and Analog mode, Amp Sim Variation and filter select, and the Reverb Bright/Dark mode.

## 5b. Behaviour layer

`crates/nord-panel/src/behaviour/` holds the panel logic shared by both front ends; it takes caller-supplied millisecond time and never touches the engine.

| Piece | Rule | Status |
|---|---|---|
| Hold | 500 ms, fires once while held | `VERIFIED` (text, manual p.18 "roughly half a second") |
| Double tap | second press within 300 ms of the previous tap's release | `ASSUMED`; manual p.39, p.43 describe the gesture without an interval |
| SHIFT resolve | tap + SHIFT held on a control with a printed shifted function = shifted; hold on such a control = shifted (p.44); hold otherwise = Hold (Layer Off, SOLO) | manual p.44 |
| Latch | double tap toggles; Shift/Exit clears (Morph Assign p.39, Section Edit p.43) | manual |
| Selector | next/previous index wraps over the LED count | manual p.12 |
| Pot pick-up | after a stored-value change the knob ignores input until it crosses or comes within 2 % of the stored value | `ASSUMED`, OQ13 |
| Live Mode | Program buttons 1-8 pick slots; dirty slot is saved on switch or leave | manual p.13, p.44 |
| `PanelHost` | `get`/`set`/`begin_gesture`/`end_gesture` by host parameter id; `PanelAction::Press` is panel-internal | design |

## 6. Decisions and open questions

Answered: trademark and art, parity scope, keybed variant, per-layer IDs and Phase 6 ticks in [D16](../DECISIONS.md#d16-phase-6b-scope-original-art-functional-or-stub-controls-ha88-first-additive-host-ids); toolkit, egui pinning and baker in [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering). Still open: OQ13 (unconfirmed hardware behaviour) in [OPEN-QUESTIONS.md](../OPEN-QUESTIONS.md).

## 7. Layout (`crates/nord-panel/src/layout/`)

`layout::rect_for(id) -> Option<Rect>` gives every control a rectangle in panel millimetres; `layout::section_rect(Section)` gives the section frames and `layout::panel_rect()` the whole instrument. `Rect {x, y, w, h}` is toolkit-free (no egui). Origin: top-left of the instrument body, x right, y down. Control area `y = 0 .. 179 mm` (`PANEL_HEIGHT_MM`), keybed below it; instrument width 1290 mm.

| Item | Value | Status |
|---|---|---|
| Source | one 2000 px product photo (`foto`, 2026-11-13), body x 67..1932 px, y 81..571 px | `ASSUMED` |
| Scale | 1290 mm / 1865 px = **0.6917 mm per photo px** (single scale, straight-on render) | `ASSUMED`: body width taken from the product data, confirm by measurement |
| Tables | authored in photo pixels, converted by `layout::px`; one file per section (`master`, `organ`, `piano`, `program`, `synth`, `effects`, `keybed`) | `ASSUMED`, accuracy about +-3 px (+-2 mm) |
| Shared rects | 81 ids (all 76 Shift/Hold functions plus 5 direct ones) use the rect of the control they hang off (`layout::SHARED`, e.g. `organ.sync` -> `organ.preset`) | `ASSUMED` from printed labels |
| Estimated | **36** ids listed in `layout::ESTIMATED`: layer/zone/focus LEDs a few pixels wide, LED groups drawn as one rect (`fx.focus.leds`, `split.leds`, `keybed.split_leds`), halves of one button (`organ.vibrato_on.a/b`), and unclear function mapping (`amp.res`, `amp.filter`, `amp.to_rotary`, `synth.voice.priority`, `synth.filter.res`, `synth.dial_list`) | `ASSUMED` |

Conventions: a drawbar rect is its slot (8 px wide) and its LED ladder the 15 px column to its right; the three `keybed.split_leds` are one strip along the top of the keys; `program.shift.right` (the SHIFT rocker next to Layer Effects) is the only rect that lies outside its inventory section (listed in `OUTSIDE_OWN_SECTION` in the tests).

Frames (`layout/frames.rs`, `layout::FRAMES`, `frames_of`, `frame_rect`): the printed boxes and bare titles of the panel (`ORGAN MODEL`, `VIB/CHORUS`, `B3 PERCUSSION`, `ACOUSTICS`, `UNISON`, `KB TOUCH`, `DYN COMP`, `PIANO SELECT`, the nine Synth boxes, the six Layer Effects boxes, `MORPH ASSIGN`, `SPLIT`, `MST CLK`, `TRANSP`, `PRESET LIBRARY`, `PROGRAM`; 29 in all). A frame is not a free rectangle: it is the union of the rectangles of its member controls grown by a pad `[left, top, right, bottom]` in photo px, so a nudged member carries its frame along. Tests: every member has a rect, ids are unique, no title names the manufacturer, every frame lies inside its section. The GUI adds that boxes of one section do not overlap (after nudges). `ASSUMED`: membership and pads read from the photos in `.reference/crops` and tuned until the legibility audit (rule `G`) passes; `piano.type_leds` was widened from 12 to 58 photo px so it holds the six type names.

What would confirm or replace the table: a higher-resolution front photo (at least 4000 px wide) or a tape measure on the instrument for the 36 estimated entries and the overall scale. The B8 renderers must not hard-code positions; they read these rects. The GUI draws them at `LAYOUT_SCALE` 2 (3120 px wide) and applies per-id nudges from `crates/nord-gui/src/panel/layout_tweaks.rs` (see `docs/gui-testing.md`); the tables themselves stay as measured.

## 7b. Engine bindings (`engine:<path>`)

A control that has no host parameter but acts on the engine through the panel is `Bound("engine:<path>")`, as in `menus/` (the menu model's own totals, 133 items: 111 bound / 22 `UiOnly` / 0 `Unsupported`, are in `midi-and-control.md` section 5; the inventory counts above are panel controls only and do not change with the menu bindings). `inventory/tests.rs` accepts the id when each dot-separated segment is a public field (`pub seg:`) or method (`pub fn seg(`) in the engine sources it lists (`engine/mod.rs`, `engine/zone_assign.rs`, `morph.rs`, `synth/sound_init.rs`). Used by `synth.sound_init` (`engine:synth_a.sound_init`). Rewired by the Synth / Effects bays: `synth.sound_init`, `reverb.variation` (`fxz_rev_var_{n}`) and the new `mod1.mst_clk` (`fxz_mod1_beats_{n}`, a Shift function of the Mod 1 RATE knob, shared rectangle). The three Synth OLED dials are `Bound("dial:<left|middle|right>")`: one dial maps to many parameters, so the binding is a per-page table (section 9b), checked by `inventory/tests.rs` (`dial:` ids resolve to non-empty columns whose host ids exist).

## 8. Standalone Program area and keybed (`crates/nord-gui`, B8d / B8g)

`panel/bay_program/` draws every `program.*`, `morph.*`, `split.*`, `mst_clk.*`, `transp.*`, `preset_library.*` id and `panel/keybed.rs` the two `keybed.*` ids at `layout_px::rect_or_empty`. Shifted ids share the base rectangle and draw a legend. Gestures use `nord_panel::behaviour` (`ButtonTracker`, `resolve`, `Latch`, `LiveMode`); a long press on SPLIT, MST CLK, TRANSP, SECTION EDIT and the Morph buttons is their primary function (screen or hold), not the shifted one. Program buttons 1-4 are soft buttons on screens that label them (Shift+n still opens the menu).

| Area | Behaviour | Status |
|---|---|---|
| Programs | 8 banks x 8 pages x 8 programs; location index = position in the sorted `.ns4b` program list | bank count `VERIFIED` (manual p.12, banks A-H; `G:82`, `H:13` seen on hardware, [dsoundman-course-findings](../research/dsoundman-course-findings.md)); mapping `ASSUMED` |
| LIST | Num = archive order, Abc = by name, Cat = by archive folder (the real category is inside the program) | `ASSUMED` (Cat) |
| Store / Store As | in-memory user library (no disk); name 16 chars; Abc / Cat / Ins / Del; category list = the 26 firmware categories (`bay_program/name.rs`, pointer table `0xC02BF3C8`, the name getter of the firmware's category list model; the `Cat:` caption `0xC014D4EC` VERIFIED, the value source open: [synth-wave-categories](../research/synth-wave-categories.md) s5); `STORE PROGRAM TO` shows the target name or `<empty>` then `As: name`; ABC strip runs blank, a-z, A-Z, 0-9, `-` | screen texts and strip order `VERIFIED` (hardware video, see findings note); `ASSUMED` (length, strip tail, and that the 26-entry firmware table is the store `Cat:` list, [oled-strings-vs-firmware](../research/oled-strings-vs-firmware.md) s4); category names `VERIFIED` |
| Split | 11 positions C2..C7 = MIDI 36..96 step 6; xFade Off / 3 / 6 notes (engine `half_semitones`); pressing the selected point's soft button again switches it off | `ASSUMED` (step, deactivation) |
| Mst Clk | tap tempo from 4 taps (manual p.40), reset after 2 s; KBS is UI-only | `ASSUMED` (2 s) |
| Morph | hold or latch a source, dial = depth, PAGE = target (7 layer levels, 3 synth cutoffs); knob-move assign needs bay hooks | step 0.05 `ASSUMED` |
| Live Mode | every slot switch stores the slot (bays do not report edits yet) | `ASSUMED` |
| Menus | Shift+Program 1..8 open the 8 menu screens of `nord_panel::menus` (UiOnly entries: the items are bound in the model, see `midi-and-control.md` section 5.1); PAGE steps pages, Program 1-4 are soft buttons (3 items + More on pages with more than four), the dial edits the focused item, Shift/Exit leaves | flow per manual p.44, p.57-64 |
| Memory Protect | System page 1, default On; blocks STORE to a bank location, Live programs exempt (Sound Manager is not in the GUI) | `VERIFIED` (manual p.57) |
| Pedal Tap | Shift+Mst Clk toggles PEDAL TAP: `ProgramEdit::PedalTap` sets the engine's panel-assigned pedal function to Master Clock tap (`triple.panel_assigned = MstClkTap`, back to Layer Scene II when off). A Left / Middle pedal set to Panel Assigned in the Pedal menu then taps the tempo (`tap_tempo`, interval window `ASSUMED`, `pedal_assign.rs`); MIDI CC66 / CC67 reach it through the standalone MIDI input | manual p.40, p.60; proof `audio/edits/program.rs` `pedal_tap_makes_a_panel_assigned_pedal_tap_the_master_clock` |
| Dimmed | SOLO (needs the Layer buttons of the other bays; UiOnly in the inventory) | follow-up `ns4-ymci` |

Allocating engine operations run on the audio thread today: TD-12 in [TECH_DEBT](../TECH_DEBT.md).

## 9. Organ, Piano and Master rewire (`crates/nord-gui`, `ns4-h7he`)

Since poly percussion, Drawbar Live, Aux KB, pedals and system settings landed in the engine, every Organ and Master control and every Piano control except `piano.select_dial` is `Bound` and no longer dimmed (`panel/organ_master_tests.rs` and `panel/organ_rewire_tests.rs` pin the dimmed set to the `Unsupported` set). Engine edits are `OrganEdit` / `PianoEdit` variants (`audio/edits/{organ,piano}.rs`), each applied without allocation.

| Control | Binding | Engine effect | Status |
|---|---|---|---|
| `organ.on`, `piano.on` | `organ_a_enabled` / `piano_a_enabled` (both layers' enable params) | tap switches every layer of the section off (scene masks via `SetLayerActive` / `PianoEdit::LayerActive`) or brings the remembered layers back (layer A when none were remembered); LED lit while any layer is active | `ASSUMED`: the emulator has no section-enable flag separate from the layer enables, so ON is the layer set |
| `organ.perc_poly` | `organz_poly_perc_{n}` | `OrganEdit::PercPoly` sets `percussion_poly` on both layers | engine `percussion_poly.rs` |
| `organ.preset` | `organz_drawbar_live_{n}` | `OrganEdit::DrawbarLive` (both layers); entering Live first sends the slider positions (`LiveDrawbars`, `organz_live1..9_{n}`). Sliders are the physical drawbars: Preset mode edits the preset and the physical bars, Live edits only the physical bars; the ladders show the preset and are dark in Live | manual p.21 |
| `organ.sync` | `organz_sync_{n}` | `OrganEdit::SyncPreset`: `sync_organ_preset` for A and B, panel preset takes the slider values | manual p.21 |
| `organ.vibrato_on.a/b` | `organx_scanner_{n}` (Off = scanner mode 0) | already sent `OrganEdit::Scanner`; binding corrected | - |
| `{organ,piano}.kb_zone_down/up`, `zone_leds` | `engine:split_router.layer_zone_masks` | Shift + OCTAVE steps the focused layer through 5 positions (all zones, zone 1..4); `Stage4Engine::set_layer_zone_leds` writes the legacy router mask and the firmware `kKbZone` code (`engine/zone_assign.rs`). Zones act once a Split is on; with no split any non-empty set spans the keyboard. Default = all zones (bit-identical) | stepping `ASSUMED` (`panel/kb_zone.rs`); code table as `routing.rs` |
| `{organ,piano}.aux_led.a/b` | `auxkb_organ` / `auxkb_piano` | LED click sends `AuxKb { layer, on }` -> `Stage4Engine::set_layer_aux` (also sets Aux KB Enable when switching on). The LED shows the UI flag through `aux_flag_from_telemetry`, to be replaced by the engine read-back | enable-on-switch `ASSUMED` |
| `rotary.organ` | `organy_rotary_on_{n}` | `OrganEdit::RotaryOrgan` sets `rotary_enabled` on both layers | engine default on |
| `rotary.stop_angle` | `organy_stop_angle_{n}` | Shift + STOP MODE opens the ANGLE popup (`panel/rotor_angle.rs`: `Free`, `1: Front Center` .. `6: Back Center`); a pick sends `OrganEdit::RotaryStopAngle(0..=6)` -> `rotary_stop_angle` on both layers (manual p.53, course video ORG 10:08-10:22) | list text `VERIFIED` for 5 of 7 entries, rest phases `ASSUMED` |
| `rotary.angle` | `organy_mic_angle_{n}` | Last row of the ANGLE popup: `OrganEdit::RotaryAngle`, wide 120 deg (engine default), narrow 60 deg. The manual does not describe a mic separation control, so this entry is `Assumed` | both degrees `ASSUMED` |

Still dimmed in these bays: `piano.select_dial` (`ns4-h7he`; the dial still cycles the loaded library models, the inventory keeps it `Unsupported` until the model list is a host parameter). 
## 9b. Synth strip and Extern rewire (`ns4-4w9l`)

The Synth strip now matches Organ and Piano: `synth.on`, `synth.kb_zone_down/up`, `synth.zone_leds`, `synth.aux_led.a/b/c` and `synth.extern` are `Bound`, so 8 entries leave `Unsupported` (`Bound` 142 to 150, `Unsupported` 75 to 67). The inventory totals per section do not change.

| Control | Binding | Engine effect | Status |
|---|---|---|---|
| `synth.on` | `synth_a_enabled` | tap switches the three Synth layers off or back on (`SetLayerActive`, existing behaviour) | `ASSUMED` as `organ.on` |
| `synth.kb_zone_down/up`, `synth.zone_leds` | `engine:split_router.layer_zone_masks` | Shift + OCTAVE steps the focused layer through the shared 5 positions of `panel/kb_zone.rs` and sends `SynthEdit::Zone` -> `Stage4Engine::set_layer_zone_leds`; the LEDs show the position (before: one LED per position 0..3, no engine edit) | stepping order `ASSUMED` |
| `synth.aux_led.a/b/c` | `auxkb_synth` | LED click sends `SynthEdit::AuxKb` -> `Stage4Engine::set_layer_aux` (enable-on-switch `ASSUMED`, same as Organ / Piano); the LED shows the engine read-back when telemetry is live | `ASSUMED` |
| `synth.extern` | `engine:set_layer_extern` | Shift+Mode toggles Extern on the focused layer: `SynthEdit::Extern` -> `Stage4Engine::set_layer_extern` (internal voices released on entry, Extern notes released on exit) and opens the EXTERN page. The page dials are CC VAL 1, CC VAL 2, PROGRAM (manual p.47) and send `ExternCc` / `ExternProgram` -> `extern_set_cc_value` / `extern_set_program` | manual p.46-47 |

The Synth page selectors, VELOCITY and GROUP shifts, ARP RUN, OSC CTRL and the octave buttons were `Unsupported` here and are `Bound("ui:synth.*")` since `ns4-lu50` (section 10).

### 9b.1 The three Synth OLED dials (`ns4-4w9l`)

`synth.dial.left/middle/right` are `Bound("dial:left")` / `"dial:middle"` / `"dial:right"` (`Bound` 150 to 153, `Unsupported` 67 to 64). The binding is the table [`inventory/synth_dials.rs`](../../crates/nord-panel/src/inventory/synth_dials.rs): one `DialCell { page, dial, label, target }` per dial of each of the 16 display pages (Waveform and PITCH/SMP once per oscillator mode, ARP MENU 1..5, Filter type, three envelopes, LFO wave, Vibrato, Extern). `target` is one of:

| Target | Meaning | Cells |
|---|---|---|
| `Edit { edits, host }` | the dial emits that `SynthParam` / `SynthEdit` variant (`|` separates alternatives, such as the waveform pick sending `Waveform(`, `FmRatio(` or `WavetableIndex(`); `host` is the host parameter of the same setting when one exists | 33 |
| `Ui(reason)` | navigation or browsing that the dial does itself (ARP page number, pattern step cursor, sample list browse); no layer edit | 10 |
| `Gap(reason)` | engine cannot honour it | 1 (`pitch_smp.sample` right dial, SAMPLE OPT: Natural / No Dyn / Fast Atk need the sample preset data) |

Tests: `synth_dials.rs` checks every named edit exists in the audio edit enums (`audio/edits/synth/{param,pattern,mod}.rs`) and the inventory test checks every `host` id in `nord-plugin`; the GUI test `ui_state/synth/dials_table_tests.rs` turns every dial of every page (`SynthPage::dial_key(mode)`), compares the edit it emits with the cell (`Edit` cells emit one of the listed variants, `Ui` and `Gap` cells emit no audio edit) and checks the labels and the gap flag against `dial_defs`. Provenance: page and dial names `VERIFIED` against the manual (p.28-37, p.46-47); the table is the behaviour (the test proves it).

What stays unsupported on the dials: only the SAMPLE OPT dial (one `Gap` cell, same as the dimmed dial on the display). The right dial's `Bound` tag therefore covers 15 of its 16 page cells; the entry is not marked `Unsupported` because the dimming of that single dial is already drawn per page by `dial_defs`.

## 10. Classification of the former `Unsupported` controls (`ns4-lu50`)

After the Organ, Piano, Synth-strip, dial and Effects rewires, 65 controls were still `Unsupported`. Each was decided one by one against the code: does a behaviour exist, does it need a host parameter, or is it missing? Totals now: `Bound` 208, `UiOnly` 67, `Unsupported` 0 (275 controls).

| Class | Controls | Binding | Proof |
|---|---|---|---|
| a. Behaviour exists, panel-only | Program: PROGRAM DIAL, PROGRAM 1-8 (8), NUM PAD, STORE, STORE AS, LIVE MODE, UNDO, LAYER INIT, MON/COPY, PASTE, MORPH ASSIGN WHEEL / A.T. / CTRLPED, CLEAR MORPH, SET KEY, PEDAL TAP, PANIC, PRESET LIBRARY ORGAN / PIANO / SYNTH, SINGLE LAYER PIANO / SYNTH (29) | `ui:program.*` | `bay_program/binding_tests.rs` |
| a. | Synth: WAVEFORM, PITCH/SMP, OSC / FILTER / AMP ENVELOPE, FILTER TYPE, VIBRATO MENU, ARP MENU (8 page selectors: the page decides which parameters the three dials edit, per the dial table of 9b.1), OSC / FILTER / AMP VELOCITY, FILTER / LFO / ARP GROUP, ARP RUN, OSC CTRL, OCTAVE SHIFT +/- (18) | `ui:synth.*` | `ui_state/synth/binding_tests.rs` |
| a. | Effects: LAYER EFFECTS ON, MOD 1 ON, MOD 2 ON, AMP SIM/EQ ON, REVERB ON (5): no ON flag exists in the engine, the panel sends `Off` types and neutral values (`ui_state/fx/chain.rs`) | `ui:fx.*` | `bay_effects/tests/binding.rs` |
| a. host id | DELAY TAP/SET (writes the delay time), AMP FILTER LP24/HP24 (a `CabinetSel` value, as TO ROTARY) | `delay_ms`, `fxsel_cabinet_{n}` | `bay_effects/tests/flows.rs` `tap_tempo_sets_the_delay_time`, `amp_filter_and_to_rotary_are_exclusive_cabinet_choices` |
| b. UiOnly | PAGE -/+ and BANK -/+ (move the browse cursor, no program changes until a Program button picks a slot), PROG VIEW (cycles the OLED view), PRESET NAME (name display latch), FX FOCUS ORGAN / PIANO / SYNTH (select the addressed chain) (9) | `UiOnly(reason)` | `binding_tests.rs` `page_bank_view_and_name_are_pure_display_state`, `bay_effects/tests/binding.rs` `fx_focus_buttons_only_change_which_chain_the_bay_addresses` |
| c. implemented now | PEDAL TAP (Shift+Mst Clk): engine function existed (`TripleFunc::MstClkTap`, `PanelFunc`), the panel control did not reach it. Manual p.40 states the behaviour exactly | `ui:program.pedal_tap` | `audio/edits/program.rs` `pedal_tap_makes_a_panel_assigned_pedal_tap_the_master_clock` |
| d. implemented now (`ns4-fqym`) | SECTION EDIT (1): held or sticky (double tap), manual p.43: an edit reaches every Layer of the Section. The panel reads `ProgramUi::section_edit_active()` once per frame in each bay and fans the edit out (section 10a) | `ui:program.section_edit` | `bay_program/binding_tests.rs` `section_edit` (latch), `panel/section_edit_tests.rs` (every layer of Organ, Piano, Synth and Effects in a real engine, plus the latch-off negative per bay) |

Honest limits of the `Bound` count:

- A `ui:` binding says the panel performs the function and a test proves it reaches the engine or the program loader; it does not say a host parameter exists. The plugin editor draws them as `Passive`.
- The eight Synth page selectors change the OLED page only; the effect on sound happens through the dials, which the dial table and `dials_table_tests.rs` prove. The binding test checks that every page has at least one dial that emits an engine edit.
- Program loading (PROGRAM DIAL, Program buttons, NUM PAD) is a `LoadRequest` the app serves from the open `.ns4b`; without a backup the display shows `<empty>`. The Preset Library for Piano and Synth needs an open backup too (`Support::Partial` in `bay_program/controls.rs`); the Organ models need none.
- STORE writes the in-memory user library, not the backup file.
- PEDAL TAP: that the panel-assigned pedal function becomes Master Clock tap while the button is on, and returns to Layer Scene II when off, is `ASSUMED` (the manual describes the effect, not the internal state); the tap interval window is `ASSUMED` as before.

## 10a. SECTION EDIT fan-out (`ns4-fqym`)

Manual p.43: "While SECTION EDIT is pressed, edits done to a parameter are performed on all Layers of that Section"; a double tap makes it sticky, a tap or Shift/Exit leaves. The panel does the fan-out, not the engine: the commands it sends already name each layer, so the standalone queue, the control thread and the plugin editor (same panel code, host writes per layer) need no change. The engine helpers `Stage4Engine::section_edit_*` stay as the tested copy primitives.

| Bay | Where | Reaches |
|---|---|---|
| Organ | `organ_strip.rs`, `bay_organ.rs` loop over `Stage4PanelState::edit_layers(focus, 2)` | OCTAVE SHIFT, KB ZONE, SUSTPED / PSTICK, the two level faders, VIB/CHORUS type and ON (A and B) |
| Piano | `piano_logic.rs`, same helper | level faders, OCTAVE SHIFT, KB ZONE, SOFT RELEASE, UNISON, DYN COMP |
| Synth | `SynthUi::section_edit` folded into `SynthUi::grouped`, so every `SynthUi::edit` (all knobs, dials and buttons that carry a `Scope`) writes `SynthTarget::All`; `ui_state/synth/section.rs` for the waveform selection and KB ZONE; `bay_synth/strip.rs` for the faders | every per-layer parameter, the oscillator selection (type, category, index, FM algorithm, OSC CTRL; each layer keeps its own Sample / Synth mode), KB ZONE (manual p.37, "all Synth Layers at once"), the three faders |
| Effects | `FxUi::section_edits`, called by `FxUi::after_edit` | the edited unit (Mod 1, Mod 2, Amp, Comp, Delay or Reverb, with the LAYER EFFECTS ON gate that scales it) copied to the other chains of the section, UI mirror and engine; Organ has one shared chain; with GROUP on the whole chain is already shared, so nothing is added |

Not fanned out (left per layer): the Layer buttons and ON (layer and scene enable), Aux KB LEDs (a routing flag of one layer, not a parameter), SOUND INIT and LAYER INIT (their own functions), EXTERN and its dials, sample and piano-model loads (`LoadSynthSample`, Piano model dial), and Morph Assign (a control moved while a Morph button is held assigns a morph and is not an edit). Controls that are already section-wide (organ drawbars, model, percussion, rotary; Piano type / timbre / touch; SUSTPED / PSTICK of Piano and Synth; KB HOLD; the Synth PSTICK range) need nothing. `ASSUMED`: faders count as "parameters" (the bean lists faders); a hardware check of a Section Edit fader move would confirm. Plugin editor: `nord-plugin/src/editor/section_edit_tests.rs` (the editor draws the same panel, so it writes each layer's host parameter).

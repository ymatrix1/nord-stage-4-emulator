# Hardware Control Inventory Specification

Status: B2 delivered (Phase 6b, `ns4-xc5n`, `ns4-3n0d`) · 273 controls checked against the OS v1.6x manual text (2026-11-13); panel-photo check limited to headline counts

## 1. Scope

One machine-checked list of every Nord Stage 4 front-panel control (manual ch. 3-9), shared by the standalone app and the plugin editor through `crates/nord-panel`. The inventory answers, per control: what it is, where the manual documents it, how sure we are, and what it drives.

Sources: Nord Stage 4 User Manual (local copy in `.reference/`, git-ignored, never committed), reference panel photos listed in [visual-style-and-panel.md](visual-style-and-panel.md) §1, and the current code (`crates/nord-gui/src/panel.rs`, `crates/nord-plugin/src/params.rs`).

## 2. Record format

Each control is one `ControlEntry` in `crates/nord-panel/src/inventory/` (one file per section or half-section, each < 400 lines; `all_controls()` returns them in panel order).

| Field | Type | Meaning |
|---|---|---|
| `id` | `String` | Stable dotted id `group.name[.layer]`, lowercase ASCII (e.g. `organ.drawbar.3`, `program.store`). Never renamed once shipped. A string, not an enum, keeps the 273 ids greppable and additive (D16). |
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
    Bound(&'static str),      // host param id (nord-plugin) / AudioHandle field
    UiOnly(&'static str),     // reason: no audio effect (e.g. Shift, Prog View)
    Unsupported(&'static str) // bean id tracking the missing behaviour
}
```

- `Bound` IDs follow the stable host-parameter contract from the DAW plugin: additive only, never renamed or removed (enforced by B3 `ns4-xhbf`).
- `UiOnly` controls still get a renderer and behaviour (LED state, menus).
- `Unsupported` controls are drawn and visibly marked; the bean id must exist in `.beans/`.

## 4. Checks (tests in `crates/nord-panel/src/inventory/tests.rs`)

| Check | Rule | Status |
|---|---|---|
| Unique ids | Lowercase ASCII, no duplicates | implemented |
| Provenance | Every entry has page 11-53, a label and a source; `Assumed` entries state why | implemented |
| Bindings | Every binding is non-empty; every `Bound` id appears as a quoted literal in the `nord-plugin` sources, or as `<prefix>_<name>[_{n}]` where `<prefix>` is a nested `id_prefix` in `params.rs` and `<name>` a literal `#[id]` (per-layer ids such as `pianox_unison_{n}`); every `Unsupported` bean exists in `.beans/` | implemented |
| Counts | Per-section, per-kind, per-binding, `Assumed` and `morphable` totals pinned (§5) | implemented |
| Layout completeness | Every inventory id has a layout rect (`layout/tests.rs`) | implemented |
| Geometry | Rects have positive size; each lies inside its section rect and the panel; section rects do not overlap; two rects overlap only when both share a base (`layout::SHARED`) | implemented |
| Renderer | Every control has a renderer | B8 |

## 5. Counts

Source: the manual text (`.reference/Nord Stage 4 User Manual v1.6X-Edition-N.pdf`, ch. 3-9, extracted with `pdftotext -layout`; PDF page = printed page) plus section context. The pins in `inventory/tests.rs` are regression guards for this list; they are not an independent count. Photo check (`foto` at 2000 px, 2026-11-13) confirmed only the headline counts: 2 organ faders + 9 drawbars, 2 piano faders, 3 synth faders, 3 synth OLED dials, 8 Program buttons, 3 Morph Assign buttons, 2 OLEDs. Per-knob and per-LED positions are placed in §7 (layout), with the less certain ones in `ESTIMATED`. Counts of physical positions that only appear in the panel figures (LED counts per ladder, knob rows) are `Assumed`.

| Section | Knobs/encoders | Faders/drawbars | Buttons/selectors | LEDs/ladders | Displays, wheel, stick, keys | Total |
|---|---|---|---|---|---|---|
| Master & Rotary | 2 | - | 5 | 1 | 2 (wheel, stick) | 10 |
| Organ | - | 11 (2 faders + 9 drawbars) | 21 | 18 (7 LEDs + 11 ladders) | - | 50 |
| Piano | 2 | 2 | 19 | 9 (7 LEDs + 2 ladders) | - | 32 |
| Program & Morph | 2 | - | 50 | 3 | 1 (OLED) | 56 |
| Synth | 17 (12 knobs + 5 encoders) | 3 | 41 | 11 (8 LEDs + 3 ladders) | 1 (OLED) | 73 |
| Layer Effects | 16 | - | 33 | 1 | - | 50 |
| Keybed | - | - | - | 1 | 1 (keys) | 2 |
| **Total** | | | | | | **273** |

By binding: `Bound` 112, `UiOnly` 49, `Unsupported` 112. By provenance: `Verified` 250, `Assumed` 23, `Empirical` 0 (no panel-photo check yet). Shift or Hold access: 75 controls. Morphable: 35.

Modelling choices: a button with its own LED is one entry; standalone LEDs, LED groups and ladders are separate entries; a Shift function printed under a control is its own entry (`access: Shift`); SOLO is both a Hold gesture on each section ON and a dedicated Program button (p.42).

## Gap list

The old code-review gap list is replaced by the 112 `Unsupported` entries, each tied to a bean (`ns4-4w9l` synth, `ns4-e3zs` effects/rotary, `ns4-h7he` organ/piano, `ns4-ymci` program/menus/Extern, `ns4-q6p7` behaviour layer). Controls the manual documents that the earlier list missed: organ PRESET LED / Drawbar Live and per-layer vibrato ON, Poly percussion, piano KB TOUCH and DYN COMP, the three Synth OLED dials with INFO/LIST, Mod 2 as a separate unit with its own Variation, Delay effect Variation and Analog mode, Amp Sim Variation and filter select, and the Reverb Bright/Dark mode.

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
| Shared rects | 80 ids (all 75 Shift/Hold functions plus 5 direct ones) use the rect of the control they hang off (`layout::SHARED`, e.g. `organ.sync` -> `organ.preset`) | `ASSUMED` from printed labels |
| Estimated | **36** ids listed in `layout::ESTIMATED`: layer/zone/focus LEDs a few pixels wide, LED groups drawn as one rect (`fx.focus.leds`, `split.leds`, `keybed.split_leds`), halves of one button (`organ.vibrato_on.a/b`), and unclear function mapping (`amp.res`, `amp.filter`, `amp.to_rotary`, `synth.voice.priority`, `synth.filter.res`, `synth.dial_list`) | `ASSUMED` |

Conventions: a drawbar rect is its slot (8 px wide) and its LED ladder the 15 px column to its right; the three `keybed.split_leds` are one strip along the top of the keys; `program.shift.right` (the SHIFT rocker next to Layer Effects) is the only rect that lies outside its inventory section (listed in `OUTSIDE_OWN_SECTION` in the tests).

What would confirm or replace the table: a higher-resolution front photo (at least 4000 px wide) or a tape measure on the instrument for the 36 estimated entries and the overall scale. The B8 renderers must not hard-code positions; they read these rects.

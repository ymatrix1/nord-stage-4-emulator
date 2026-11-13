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
| Bindings | Every binding is non-empty; every `Bound` id appears as a quoted literal in `nord-plugin/src/{params,fx_params,fx_selectors,morph_assign}.rs`; every `Unsupported` bean exists in `.beans/` | implemented |
| Counts | Per-section, per-kind, per-binding, `Assumed` and `morphable` totals pinned (§5) | implemented |
| Completeness | Every control has a layout rect and a renderer | B8 |
| Geometry | No two rects overlap; each rect lies inside its section rect | B8 |

## 5. Counts

Source: the manual text (`.reference/Nord Stage 4 User Manual v1.6X-Edition-N.pdf`, ch. 3-9, extracted with `pdftotext -layout`; PDF page = printed page) plus section context. The pins in `inventory/tests.rs` are regression guards for this list; they are not an independent count. Photo check (`foto` at 2000 px, 2026-11-13) confirmed only the headline counts: 2 organ faders + 9 drawbars, 2 piano faders, 3 synth faders, 3 synth OLED dials, 8 Program buttons, 3 Morph Assign buttons, 2 OLEDs. Per-knob and per-LED positions need a higher-resolution photo or the B8 layout step. Counts of physical positions that only appear in the panel figures (LED counts per ladder, knob rows) are `Assumed`.

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

By binding: `Bound` 66, `UiOnly` 49, `Unsupported` 158. By provenance: `Verified` 250, `Assumed` 23, `Empirical` 0 (no panel-photo check yet). Shift or Hold access: 75 controls. Morphable: 35.

Modelling choices: a button with its own LED is one entry; standalone LEDs, LED groups and ladders are separate entries; a Shift function printed under a control is its own entry (`access: Shift`); SOLO is both a Hold gesture on each section ON and a dedicated Program button (p.42).

## Gap list

The old code-review gap list is replaced by the 158 `Unsupported` entries, each tied to a bean (`ns4-4w9l` synth, `ns4-e3zs` effects/rotary, `ns4-h7he` organ/piano, `ns4-ymci` program/menus/Extern, `ns4-q6p7` behaviour layer). Controls the manual documents that the earlier list missed: organ PRESET LED / Drawbar Live and per-layer vibrato ON, Poly percussion, piano KB TOUCH and DYN COMP, the three Synth OLED dials with INFO/LIST, Mod 2 as a separate unit with its own Variation, Delay effect Variation and Analog mode, Amp Sim Variation and filter select, and the Reverb Bright/Dark mode.

## 6. Decisions and open questions

Answered: trademark and art, parity scope, keybed variant, per-layer IDs and Phase 6 ticks in [D16](../DECISIONS.md#d16-phase-6b-scope-original-art-functional-or-stub-controls-ha88-first-additive-host-ids); toolkit, egui pinning and baker in [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering). Still open: OQ13 (unconfirmed hardware behaviour) in [OPEN-QUESTIONS.md](../OPEN-QUESTIONS.md).

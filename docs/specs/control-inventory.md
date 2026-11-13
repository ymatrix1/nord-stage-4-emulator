# Hardware Control Inventory Specification

Status: draft (Phase 6b, `ns4-xc5n`) · Counts and pages **unverified** against the manual

## 1. Scope

One machine-checked list of every Nord Stage 4 front-panel control (manual ch. 3-9), shared by the standalone app and the plugin editor through `crates/nord-panel` (planned). The inventory answers, per control: what it is, where the manual documents it, how sure we are, and what it drives.

Sources: Nord Stage 4 User Manual (local copy in `.reference/`, git-ignored, never committed), reference panel photos listed in [visual-style-and-panel.md](visual-style-and-panel.md) §1, and the current code (`crates/nord-gui/src/panel.rs`, `crates/nord-plugin/src/params.rs`).

## 2. Record format

Each control is one `ControlEntry` in `crates/nord-panel/src/inventory/` (planned, one file per section, each < 400 lines).

| Field | Type | Meaning |
|---|---|---|
| `id` | `ControlId` | Stable enum variant, `Section_Layer_Name` (e.g. `OrganA_Drawbar3`, `Program_Store`). Never renamed once shipped. |
| `section` | `Section` | `Master`, `Organ`, `Piano`, `Program`, `Synth`, `Effects`, `Keybed` |
| `kind` | `ControlKind` | `Knob`, `Encoder`, `Fader`, `Drawbar`, `Button`, `Selector`, `Led`, `LedLadder`, `Oled`, `Wheel`, `Stick`, `Key` |
| `layer` | `Option<Layer>` | `A`/`B`/`C` for per-layer controls; `None` for global |
| `manual_page` | `u16` | Page in the OS v1.x manual where the control is described |
| `provenance` | `Provenance` | `Verified` / `Empirical` / `Assumed` per [provenance-in-code](../../.agents/rules/provenance-in-code.md), with source text |
| `binding` | `Binding` | See §3 |
| `rect_mm` | `Rect` | Layout rect in panel millimetres (filled by `layout/`, B8) |

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

## 4. Checks (planned tests in `nord-panel`)

| Check | Rule |
|---|---|
| Completeness | Every `ControlId` has a `ControlEntry`, a layout rect, a renderer and a binding |
| Geometry | No two rects overlap; every rect lies inside its section rect |
| Counts | Per-section and per-kind counts equal the table in §5 (after B2 verifies them) |
| Bindings | Every `Bound` id exists in the host param list; every `Unsupported` bean exists |
| Provenance | Every entry has a non-empty source string |

## 5. Counts

**Unverified.** To be filled by B2 (`ns4-3n0d`) from manual ch. 3-9 and checked against the reference photo. Until then, no count in this repo is authoritative.

| Section | Knobs/encoders | Faders/drawbars | Buttons/selectors | LEDs/ladders | Displays | Status |
|---|---|---|---|---|---|---|
| Master & Rotary | ? | ? | ? | ? | - | unverified |
| Organ | ? | ? | ? | ? | - | unverified |
| Piano | ? | ? | ? | ? | - | unverified |
| Program & Morph | ? | - | ? | ? | 1 | unverified |
| Synth | ? | ? | ? | ? | 1 | unverified |
| Layer Effects | ? | - | ? | ? | - | unverified |
| Keybed & wheels | - | - | - | ? | - | unverified |

## Gap list

Controls with no state, host parameter or widget today (code review of `crates/nord-gui` and `crates/nord-plugin` on 2026-11-12; not yet checked against the manual, so it may be incomplete).

| Section | Missing |
|---|---|
| All sections | `SOLO`, `AUX KB`, `SUSTPED` / `PSTICK` enables, per-layer `KB ZONE`, `FX FOCUS` LED |
| Organ | `PRESET`, `SYNC`, `LIVE`, `B3 BASS`, Vox/Farf/Pipe registers, swell, key click, per-layer A/B settings; rotary `CLOSE MIC`, `ANGLE`, `STOP MODE` |
| Piano | `SOFT RELEASE`, `UNISON 1/2/3`, `DYN COMP`, `TYPE`, `MODEL` dial + `LIST`, per-layer settings (panel keeps one shared `piano_cat` / `piano_timbre`) |
| Synth | GUI exposes about 12 fields. Missing: Arp/Gate (Rate, Pattern, Range, KB Sync, Master Clock), LFO, Voice (Mono, Legato, Glide), Vibrato, Osc Ctrl / Pitch / Env Amt, filter envelope, KB track, Unison, KB Hold, Sound Init, 3 OLED dials, per-layer settings. `nord-core/src/synth_types.rs` already has `ArpMode`, `LfoShape`, `VibratoMode`, `NotePriority`. |
| Layer Effects | Mod 1 / Mod 2 variations, Amp Sim model, `TO ROTARY`, LP/HP filters, Delay (Tap, Ping-Pong, Analog, Filter, Master Clock), Comp Fast, Reverb Bright/Dark + Global mode, `ON` buttons, Morph LEDs |
| Program | `STORE` / `STORE AS`, `PROG VIEW`, Preset Name, Preset Library, `PROGRAM` dial + `LIST`, Page/Cat, Bank, Num Pad, `LIVE MODE`, Pedal, Solo, Undo, Section Edit / Layer Init, Mon/Copy, Paste, Shift/Exit, Transp On/Set, Panic, Mst Clk Tap/Set, Split On/Set/Set Key, Clear Morph, MIDI/Extern indicators. The 8 Program buttons double as menu buttons (System, Sound, Organize, Aux KB, Output, Pedal, MIDI, Extern); Extern (manual ch. 8) is missing. |
| Host params | `nord-plugin` has about 180 params after the DAW-plugin PR (morph depths and per-layer FX added there). Synth params are shared by A/B/C; none exist for Piano Unison / Dyn Comp. Per-layer naming: additive `synth_b_*` / `synth_c_*`, existing IDs stay Layer A ([D16](../DECISIONS.md#d16-phase-6b-scope-original-art-functional-or-stub-controls-ha88-first-additive-host-ids)). |

## 6. Decisions and open questions

Answered: trademark and art, parity scope, keybed variant, per-layer IDs and Phase 6 ticks in [D16](../DECISIONS.md#d16-phase-6b-scope-original-art-functional-or-stub-controls-ha88-first-additive-host-ids); toolkit, egui pinning and baker in [D15](../DECISIONS.md#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering). Still open: OQ13 (unconfirmed hardware behaviour) in [OPEN-QUESTIONS.md](../OPEN-QUESTIONS.md).

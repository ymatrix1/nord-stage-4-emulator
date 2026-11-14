# Running the GUI and testing firmware paths

The standalone `nord-stage4` (crate `nord-gui`) plays through cpal and midir. Every opt-in firmware
engine path of `ns4 render-patch` can be chosen live from dropdowns in the bar above the panel, or
at start-up from the same CLI flags. Code: `crates/nord-gui/src/firmware_panel.rs`, shared option
struct `nord_core::render_opts::FirmwareOptions`.

## Commands

```bash
# Build and run (release is needed for real-time audio)
cargo run --release -p nord-gui --bin nord-stage4
# Everything verified, on from the start
cargo run --release -p nord-gui --bin nord-stage4 -- --firmware-all
# Pick paths one by one (same flags as `ns4 render-patch`)
cargo run --release -p nord-gui --bin nord-stage4 -- --tone-source b3wheels --b3-tail off --synth-chain strip --filter-engine firmware --firmware-fx --firmware-piano-level --measured-laws
# Load a backup and a piano library (large files live in .reference/, git-ignored)
cargo run --release -p nord-gui --bin nord-stage4 -- ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" .reference/piano.npno
```

Positional arguments are routed by extension (`.ns4b`, `.ns4p`, `.npno`, `.nsmp4`); `--backup`,
`--program`, `--piano`, `--sample` also work, and files can be dropped on the window.

The bar has the `Firmware preset` / `Legacy preset` buttons, a dropdown per engine (Organ source,
B3 tail when B3 wheels is selected, Synth chain, Synth filter, Piano level, FX amp), a Measured laws
checkbox, and an `Engine: ...` status. The same summary is in the window title. Changes are sent as
`AudioCommand::SetFirmwareOptions` and take effect on the next audio block while you play. B3 tail
rebuilds the wheel tables, so it can click once.

## Headless check (no window, no audio device)

```bash
nord-stage4 --firmware-all --self-test out.wav   # renders organ+piano+synth chord for 1 s
nord-stage4 --measured-laws --dump-options       # prints the active set
```

The self-test prints peak, RMS and the RMS difference to the default engine (0 for defaults) and
exits non-zero on non-finite samples.

## Screenshots, focus view and the legibility check

```bash
# One PNG per section at 2x (builds once, writes target/shots/<section>.png)
python tools/gui_shots.py [--out DIR] [--scale 2] [--sections organ,synth] [--no-build]
# One shot by hand; sections: organ piano program synth effects master keybed (default: whole panel)
nord-stage4 --screenshot out.png --screenshot-section synth --screenshot-scale 2
```

`--screenshot-scale N` is physical px per logical panel px. The window opens off-screen without
taking focus or a taskbar entry, is resized to the region, the frame is cropped to it, and the
process exits 0 after saving (`std::process::exit`; the normal close path ends with
`0xcfffffff` on Windows). A 20 s watchdog exits 3 when no PNG arrives; `gui_shots.py` also kills
a child that runs over 30 s. A whole-panel shot is limited by the OS window size (the keybed
is capped at 1x for that reason). F12 still saves the live window to `screenshot.png`. Code:
`crates/nord-gui/src/screenshot.rs`.

Focus view (B12): click a section header or press `Z` (section under the pointer, else the last
one) to zoom that section to fill the window (up to 4x, crisp text: the egui zoom factor, not a
bitmap scale); `Esc` returns, `1`-`5` jump to Organ, Piano, Program, Synth, Layer Effects. In the
overview the window scale-to-fits the panel. Code: `crates/nord-gui/src/panel/view.rs`.

Scale: the panel is drawn at `LAYOUT_SCALE` = 2 (3120 x 840 logical px, `panel/layout_px.rs`) so
that 7.5 px labels fit between neighbours. A 1900 px window therefore shows the overview at about
0.6x: read labels in the focus view or in `gui_shots.py` output.

`panel/legibility_tests.rs` renders the real panel headless and fails on: any text under
`MIN_FONT_PX` (7.5), two overlapping labels, or a label overlapping a control rectangle it
does not belong to. `legibility_allow.txt` lists tolerated collisions (empty now; the test also
fails on stale entries). Nudges for tight neighbours live in `panel/layout_tweaks.rs`, never in
the `ASSUMED` photo tables. Before/after: `docs/img/gui/`.

| State | Labels >= 7.5 px | Collisions |
|---|---|---|
| Before (original fonts 4.5-7 px, 1560 px panel) | no (smallest 4.5) | 33 |
| 7.5 px minimum at 1560 px | yes | 98 (with the stricter 1 px-gap rule: more) |
| 7.5 px minimum at 3120 px plus 6 nudges | yes | 0 |

## What each switch changes

| Switch (flag / dropdown) | Changes | Status |
|---|---|---|
| `--tone-source firmware` (Organ: Pipe) | PNO core 1 oscillator bank for the Pipe organ models | VERIFIED vs SHARC oracle (role: Pipe) |
| `--tone-source b3wheels` | B3 generator pitch ladder and wheel tables | ladder VERIFIED 2.5e-11; per-key weights not modelled |
| `--tone-source vox`, `farfisa` | Vox note bank, Farfisa dividers | opt-in, partial (processors not modelled) |
| `--tone-source legacy` | Pre-bank float waveform | ASSUMED |
| `--b3-tail on\|off` | B3 processor-tail response | EMPIRICAL (`docs/research/b3-vox-video-gap.md`) |
| `--filter-engine firmware` | Firmware synth filter | VERIFIED against the oracle (`synth-engine.md`) |
| `--synth-chain strip` | Firmware per-voice strip chain | partly VERIFIED, ADR/R laws open (OQ22) |
| `--firmware-fx` (FX amp: Firmware) | Every `FirmwareFx` port on every FX chain | per-port, see `effects-and-routing.md` |
| `--firmware-piano-level` (Piano level: Firmware) | Firmware velocity level law | see `piano-engine.md` |
| `--measured-laws` | 16-voice Super Saw, envelope depth, decay, pitch, vibrato | EMPIRICAL (`docs/research/audible-genius-synth-lessons.md`) |
| `--firmware-all` / `--legacy-all` | Presets: everything verified above (not measured laws, not Vox/Farfisa/B3) / all defaults | - |

Defaults are unchanged: with no flag the engine and the `render_hash` stay as before.

## Not yet wired

`--amp-path` (engine/firmware cabinet) and `--rotary-drive` (engine/unity) are on PR #163, and
`--legacy-piano-level` follows PR #159 (firmware level becomes the default; the flag is already
accepted and turns the firmware level off). Add them as fields of `FirmwareOptions` once merged;
the GUI bar then needs one dropdown each.

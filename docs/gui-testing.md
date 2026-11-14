# Running the GUI and testing firmware paths

The standalone `nord-stage4` (crate `nord-gui`) plays through cpal and midir. Every opt-in firmware
engine path of `ns4 render-patch` can be chosen live from dropdowns in the bar above the panel, or
at start-up from the same CLI flags. Code: `crates/nord-gui/src/firmware_panel/`, shared option
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

Large files live in `.reference/` of the **main checkout** (`C:\dev\nord-stage-4-emulator\.reference\`),
not in a worktree: pass the absolute path from a worktree, e.g.
`"C:\dev\nord-stage-4-emulator\.reference\Nord Stage 4 Factory Restore RevK v1.66.ns4b"`.

## The bar

Presets `Firmware preset` (`--firmware-all`), `Legacy preset` (`--legacy-all` plus the engine amp)
and `Defaults`, then one group per engine, each switch with a tooltip (`[STATUS] flag` and a
one-line meaning) and an `Engine: ...` status line with the active set (also in the window title):

| Group | Controls |
|---|---|
| Organ | Source, B3 tail (enabled with B3 wheels), Rotary drive, Level |
| Synth | Chain, Filter, Measured laws, Shape (checkbox + slider) |
| Piano | Level, SMP bus scale |
| FX | Amp path, Firmware FX, Firmware reverb |
| Global | Firmware routing |

Changes are sent as `AudioCommand::SetFirmwareOptions` and `FirmwareOptions::apply_exact` runs on
the audio thread between blocks (plain field writes, no allocation; rotary scaling is a flag, the
program's drive is never rewritten). Only a real B3 tail change rebuilds the wheel tables
(allocates once, resets the wheel phases and can click).

## Headless check (no window, no audio device)

```bash
nord-stage4 --firmware-all --self-test out.wav   # renders organ+piano+synth chord for 1 s
nord-stage4 --measured-laws --dump-options       # prints the active set
```

The self-test prints peak, RMS and the RMS difference to the default engine (0 for defaults) and
exits non-zero on non-finite samples. The window itself needs a display and an audio device.

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

`panel/legibility_tests.rs` (collector, control rectangles) and `panel/legibility_rules.rs`
(geometry rules) render the real panel headless and record every painted text galley with its
clip rectangle, plus the group frames and knob extents the widgets report through the test-only
`widgets/probe.rs`. The audit fails on:

| Rule | Meaning |
|---|---|
| min font | any text under `MIN_FONT_PX` (7.5 px) |
| `L` | two labels overlap or are closer than 0.9 px |
| `C` | a label overlaps (bezel +1.5 px) a control rectangle whose body it is not printed on |
| `K` | a label is clipped by the clip rectangle it was painted with (OLED soft labels) |
| `S` | a label crosses the edge of a section rectangle (text cut by the section) |
| `G` | a label crosses a group frame (Rotary Speaker box, `group_frame`) other than its own title |
| `N` | a label touches a knob body or its tick ring (the ring is open below the knob) |

`legibility_allow.txt` lists tolerated violations (empty now; the test also fails on stale
entries). Run with `NS4_WRITE_COLLISIONS=<file> cargo test -p nord-gui collisions_match` to dump
the current list. Fixes go into the shared helpers first: `widgets::label_fit` / `fit_size`
shrink a text to the room it has (never below 7.5 px), `ctl::tag_fit` stacks a two-word name,
`ctl::shift_legend` puts its LED after the measured text, `bay_effects::ctx::CAPTION_DROP` /
`CAPTION_LINE` keep captions clear of knobs and bezels. Per-control nudges for tight neighbours
live in `panel/layout_tweaks.rs`, never in the `ASSUMED` photo tables (one exception: the
`rotary.speed_leds` rectangle was widened so the row holds its two names). Before/after:
`docs/img/gui/`.

| State | Labels >= 7.5 px | Collisions |
|---|---|---|
| Before (original fonts 4.5-7 px, 1560 px panel) | no (smallest 4.5) | 33 |
| 7.5 px minimum at 1560 px | yes | 98 (with the stricter 1 px-gap rule: more) |
| 7.5 px minimum at 3120 px plus 6 nudges (first audit: L and C only) | yes | 0 |
| Same panel, audit extended with K, S, G, N, 0.9 px gap, bezel margin (B8 label legibility) | yes | 30 |
| After the legibility fixes (Rotary box widened, captions, OLED soft labels, SOLO, nudges) | yes | 0 |

## What each switch changes

| Switch (flag / bar control) | Changes | Status |
|---|---|---|
| `--tone-source` (Organ: Source) | `firmware` pipe bank VERIFIED vs SHARC oracle; `b3wheels` ladder VERIFIED 2.5e-11, per-key weights not modelled; `vox`, `farfisa` partial; `legacy` ASSUMED | MIXED per choice |
| `--b3-tail on\|off` (Organ: B3 tail) | B3 processor-tail response (`docs/research/b3-vox-video-gap.md`) | EMPIRICAL |
| `--rotary-drive engine\|unity\|scaled` (Organ: Rotary drive) | `tanh(g x)`, `tanh(g x)/g`, or drive/10 (`docs/research/b3-rotary.md`) | EMPIRICAL |
| `--organ-level default\|legacy\|firmware` (Organ: Level) | Organ output level law: legacy bus scale, or firmware swell word and squared layer level (`docs/research/organ-output-law.md`) | MIXED per choice |
| `--synth-chain legacy\|strip` (Synth: Chain) | Per-voice chain: firmware strip (default since D22, `docs/research/synth-validation.md`) or the classic engine chain; `--legacy-all` selects the classic chain | MIXED (partly VERIFIED) |
| `--synth-stages` / `--no-synth-stages` (Synth: Stage oscillators) | Bit-exact firmware oscillator stages on the strip chain (noise, Shape Square, SUPER, MULTI, SHAPE SINE); on by default | MIXED (stages VERIFIED, voice weights ASSUMED) |
| `--synth-p table\|fitted\|off\|g,w2,r2,shape,cutoff` (Synth: P table) | Per-wave oscillator level, cutoff factor and SHAPE word law fitted to hardware clips (`synth/p_table.rs`) | EMPIRICAL |
| `--filter-engine zdf\|firmware` (Synth: Filter) | Firmware synth filter (`synth-engine.md`) | VERIFIED vs oracle |
| `--measured-laws` (Synth: Measured laws) | 16-voice Super Saw, envelope depth, decay, pitch, vibrato (`audible-genius-synth-lessons.md`) | EMPIRICAL |
| `--synth-shape <0..1>` (Synth: Shape) | Shape word of the firmware SHAPE oscillator (strip chain); off keeps the saw placeholder | ASSUMED |
| `--firmware-piano-level` / `--legacy-piano-level` (Piano: Level) | Firmware velocity level law for `.npno` (default on); legacy is the touch curve (`piano-engine.md`) | VERIFIED |
| `--firmware-piano-bus` (Piano: SMP bus scale) | Sample path scaled 0.5 instead of 0.55 (Q31 receiver convention) | ASSUMED |
| `--amp-path engine\|firmware` (FX: Amp path) | Firmware drive stage (engine default) or engine tanh drive and RBJ cabinet; 4 frames latency | VERIFIED (`tests_amp_golden.rs`) |
| `--firmware-fx` (FX: Firmware FX) | Every `FirmwareFx` port (delay laws, delay time, reverb mix, compressor), see `effects-and-routing.md` | MIXED per port |
| `--firmware-reverb` (FX: Firmware reverb) | Firmware Spring reverb routine on every chain, Spring types only, 4 frames latency | MIXED (see `fx-reverb-leslie.md`) |
| `--firmware-routing` (Global: Firmware routing) | Section/layer enables and per-layer KB zones as stored (D22); zone codes open | ASSUMED |
| `--firmware-all` / `--legacy-all` | Presets: verified paths on (not measured laws, routing, Vox/Farfisa/B3) / all defaults | - |

Defaults are unchanged: with no flag the engine and the `render_hash` stay as before. The same flags
work for `ns4 render-patch`; both parse them with `FirmwareOptions::from_args`, so a switch added to
that struct needs a field, a parser, `apply_to`/`apply_exact`, a `summary` entry, a `SwitchInfo` in
`firmware_panel/info.rs`, a bar control and a row in this table (a unit test checks the table).

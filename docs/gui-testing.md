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

Whole-engine presets `Firmware preset` (`--firmware-all`), `Classic preset` (`--legacy-all` plus
the engine amp) and `Defaults`; each group (Organ, Piano, Synth, FX) also has small `C` (Classic)
and `F` (Firmware) buttons that set only that engine's controls and leave the others as picked
(`firmware_panel/presets.rs`; tests check that the four engine presets compose to the whole
presets). Below, one row per group, each switch with a tooltip (`[STATUS] flag` and a one-line
meaning) and an `Engine: ...` status line with the active set (also in the window title). Every
opt-in field of `FirmwareOptions` has a control:

| Group | Controls |
|---|---|
| Organ | Source, B3 tail (enabled with B3 wheels), B3 Bass processor, Farfisa processor, Rotary drive, Level, Leslie (engine / firmware), Ring and Leslie gain (enabled with the firmware Leslie) |
| Piano | Level, SMP bus scale, Layer law |
| Global | Firmware routing |
| Synth | Chain, Stage oscillators, P table, Filter, Measured laws, Shape (checkbox + slider), Sample kernel, Sample pitch |
| FX | Amp path, All ports, then one checkbox per port: Delay laws, Delay time, Reverb mix, Reverb (Spring and the multi-tap types 2..11), Compressor, Mod 1, Mod 2, Amp EQ |

Changes are sent as `AudioCommand::SetFirmwareOptions` and `FirmwareOptions::apply_exact` runs on
the audio thread between blocks (plain field writes, no allocation; rotary scaling is a flag, the
program's drive is never rewritten). Two switches allocate once: a real B3 tail change rebuilds
the wheel tables (resets the wheel phases and can click), and switching the firmware Leslie on
or changing its ring builds a new processor (the rotor restarts); identical repeated commands keep
both. Switching the Leslie back to the engine rotary drops the processor.

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

Panorama (B12): `P` shows a pannable / zoomable part of the panel with a strip of the whole
instrument under it (drag or click the strip, wheel or `+` / `-` zoom, arrows pan, `Esc` leaves);
`L` toggles the library drawer. `--screenshot` never enters the panorama and never draws the
drawer, so `gui_shots.py` output is unchanged. Code: `panel/panorama.rs`, tests
`panel/panorama_tests.rs`.

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

Round 2 (`panel/legibility_caps.rs`, same collector plus `Kind::CapTravel` / `Kind::Led` records
from the widgets) adds:

| Rule | Meaning |
|---|---|
| `F` | a label overlaps the travel of a fader cap (drawbar numbers hidden by the pushed-out cap) |
| `E` | an LED bezel is closer than 4 px to a group frame edge, or on it |
| `T` | a label comes closer than 0.6 px to an LED bezel |
| `V` | two stacked labels (sharing a third of their width) have less than 2 px between their ink; text inside OLEDs is exempt |
| `M` | a knob's name is more than 12 px from its tick ring (or missing); a caption beside a trimmer counts |

The bezel margin of rule `C` grew from 1.5 to 2.5 px (the button bezel is 2 px). Stacked-line
pitches (`ctl::TAG_LINE`, `bay_effects::ctx::CAPTION_LINE`, Synth headings, Piano TIMBRE rows)
are 9.5-10 px so that rule `V` holds; state LEDs of Shift legends sit before the text, as printed
on the instrument.

Round 3 adds frames (`panel/frames.rs`, `widgets/scale.rs`) and needs no new rule: boxes record themselves with the widget probe, so rule `G` keeps every label inside or clear of a box, rule `E` keeps LEDs 4 px from its edge, rule `C` keeps scale numerals off control rectangles and rule `N` off tick rings. Unit tests cover the frame table (`nord-panel`: members exist, frames inside sections; `nord-gui`: boxes do not overlap after nudges), the numerals, the type and timbre names, the keybed LED letters and the contrast floor (`style::dim_legend`, 4.5:1 against slate and red).

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
| Round 2 rules F, E, T, V, M and bezel 2.5 px added (first run: 9 F, 8 T, 22 V = 39; M and the wider bezel added 1 + 6 on later runs) | yes | 39 |
| After round 2 (drawbar numbers below the cap travel, LEDs before legend text, 9.5 px line pitch, nudges) | yes | 0 |
| Round 3 content added (29 frames, type / timbre names first run: 28; then scale numerals, dial names, drawbar names and numerals: 64 and 43 on the next runs; rules unchanged) | yes | 28 |
| After round 3 (frame pads, group nudges, scale placement, title positions) | yes | 0 |

## What each switch changes

| Switch (flag / bar control) | Changes | Status |
|---|---|---|
| `--tone-source` (Organ: Source) | `firmware` pipe bank VERIFIED vs SHARC oracle; `b3wheels` ladder VERIFIED 2.5e-11, per-key weights not modelled; `vox`, `farfisa` partial; `legacy` ASSUMED | MIXED per choice |
| `--b3-tail on\|off` (Organ: B3 tail) | B3 processor-tail response (`docs/research/b3-vox-video-gap.md`) | EMPIRICAL |
| `--rotary-drive engine\|unity\|scaled` (Organ: Rotary drive) | `tanh(g x)`, `tanh(g x)/g`, or drive/10 (`docs/research/b3-rotary.md`) | EMPIRICAL |
| `--b3-bass-processor on\|off` (Organ: B3 Bass processor) | B3 Bass key stage (generator weights, window words, line gain of `0x122296`); replaces the B3 tail | MIXED |
| `--farfisa-processor on\|off` (Organ: Farfisa processor) | Key-down weights and line gain of the Farfisa processor `0x122B85` (Farfisa wheels source; partial) | ASSUMED |
| `--leslie firmware\|engine` (Organ: Leslie) | Firmware Leslie processor instead of the engine rotary (`docs/research/fx-leslie.md`) | MIXED (routine VERIFIED, ring EMPIRICAL) |
| `--leslie-ring firmware\|advance\|frozen` (Organ: Ring) | History ring pointers of the firmware Leslie: advance (default), booted pass routine, first oracle state | EMPIRICAL |
| `--leslie-gain-db <dB>` (Organ: Leslie gain) | Gain after the firmware Leslie, -24..+24 dB in the bar | ASSUMED |
| `--organ-level default\|legacy\|firmware` (Organ: Level) | Organ output level law: legacy bus scale, or firmware swell word and squared layer level (`docs/research/organ-output-law.md`) | MIXED per choice |
| `--synth-chain legacy\|strip` (Synth: Chain) | Per-voice chain: firmware strip (default since D22, `docs/research/synth-validation.md`) or the classic engine chain; `--legacy-all` selects the classic chain | MIXED (partly VERIFIED) |
| `--synth-stages` / `--no-synth-stages` (Synth: Stage oscillators) | Bit-exact firmware oscillator stages on the strip chain (noise, Shape Square, SUPER, MULTI, SHAPE SINE); on by default | MIXED (stages VERIFIED, voice weights ASSUMED) |
| `--synth-p table\|fitted\|off\|g,w2,r2,shape,cutoff` (Synth: P table) | Per-wave oscillator level, cutoff factor and SHAPE word law fitted to hardware clips (`synth/p_table.rs`) | EMPIRICAL |
| `--sample-kernel key\|ratio` (Synth: Sample kernel) | `SAMPLES` oscillator 12-tap kernel pick: firmware key rule (default, D46) or the ratio rule | MIXED (run VERIFIED, key field ASSUMED) |
| `--sample-pitch fw\|equal` (Synth: Sample pitch) | `SAMPLES` step from the ARM exponent tables or exact `2^(n/12)` | MIXED (law VERIFIED, root and rate ASSUMED) |
| `--filter-engine zdf\|firmware` (Synth: Filter) | Firmware synth filter (`synth-engine.md`) | VERIFIED vs oracle |
| `--measured-laws` (Synth: Measured laws) | 16-voice Super Saw, envelope depth, decay, pitch, vibrato (`audible-genius-synth-lessons.md`) | EMPIRICAL |
| `--synth-shape <0..1>` (Synth: Shape) | Shape word of the firmware SHAPE oscillator (strip chain); off keeps the saw placeholder | ASSUMED |
| `--firmware-piano-level` / `--legacy-piano-level` (Piano: Level) | Firmware velocity level law for `.npno` (default on); legacy is the touch curve (`piano-engine.md`) | VERIFIED |
| `--firmware-piano-layer-law` (Piano: Layer law) | Piano layer fader follows `(v / 127)^2` | ASSUMED |
| `--firmware-piano-bus` (Piano: SMP bus scale) | Sample path scaled 0.5 instead of 0.55 (Q31 receiver convention) | ASSUMED |
| `--amp-path engine\|firmware` (FX: Amp path) | Firmware drive stage (engine default) or engine tanh drive and RBJ cabinet; 4 frames latency | VERIFIED (`tests_amp_golden.rs`) |
| `--firmware-fx` (FX: All ports) | Every `FirmwareFx` port at once, see `effects-and-routing.md`; the bar shows them as the eight checkboxes below | MIXED per port |
| `--firmware-fx-ports delay-laws` (FX: Delay laws) | Delay send and dry gains follow `kDlyMix` | MIXED |
| `--firmware-fx-ports delay-time` (FX: Delay time) | Free-running delay time follows `kDlyRateClkDiv` | VERIFIED |
| `--firmware-fx-ports reverb-mix` (FX: Reverb mix) | Reverb send and dry gains follow `kRvbAmt` | MIXED |
| `--firmware-reverb` (FX: Reverb) | Firmware reverb routines on every chain: Spring (types 0, 1) and multi-tap (types 2..11), 4 frames latency | MIXED (see `fx-reverb-leslie.md`) |
| `--firmware-fx-ports compressor` (FX: Compressor) | Firmware gain computer with the `kCmpAmt` words, 4 frames latency | MIXED |
| `--firmware-fx-ports mod1` (FX: Mod 1) | A-PAN and TREM firmware routines, free running only | MIXED |
| `--firmware-fx-ports mod2` (FX: Mod 2) | Firmware CHOR, PHAS, FLANG, ENS and SPIN stages | MIXED |
| `--firmware-fx-ports eq` (FX: Amp EQ) | Firmware tone stack `0x1243F4` instead of the engine's three biquads, 2 frames latency | MIXED |
| `--firmware-routing` (Global: Firmware routing) | Section/layer enables and per-layer KB zones as stored (D22); zone codes open | ASSUMED |
| `--firmware-all` / `--legacy-all` | Presets: verified paths on (not measured laws, routing, Vox/Farfisa/B3) / all defaults | - |

Defaults are unchanged: with no flag the engine and the `render_hash` stay as before. The same flags
work for `ns4 render-patch`; both parse them with `FirmwareOptions::from_args`, so a switch added to
that struct needs a field, a parser, an `apply_exact` line (the GUI path), a per-engine preset entry in `firmware_panel/presets.rs`, `apply_to`/`apply_exact`, a `summary` entry, a `SwitchInfo` in
`firmware_panel/info.rs`, a bar control and a row in this table (a unit test checks the table).

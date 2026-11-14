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
| Organ | Source, B3 tail (enabled with B3 wheels), Rotary drive |
| Synth | Chain, Filter, Measured laws, Shape (checkbox + slider) |
| Piano | Level, SMP bus scale |
| FX | Amp path, Firmware FX |
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

## What each switch changes

| Switch (flag / bar control) | Changes | Status |
|---|---|---|
| `--tone-source` (Organ: Source) | `firmware` pipe bank VERIFIED vs SHARC oracle; `b3wheels` ladder VERIFIED 2.5e-11, per-key weights not modelled; `vox`, `farfisa` partial; `legacy` ASSUMED | MIXED per choice |
| `--b3-tail on\|off` (Organ: B3 tail) | B3 processor-tail response (`docs/research/b3-vox-video-gap.md`) | EMPIRICAL |
| `--rotary-drive engine\|unity\|scaled` (Organ: Rotary drive) | `tanh(g x)`, `tanh(g x)/g`, or drive/10 (`docs/research/b3-rotary.md`) | EMPIRICAL |
| `--synth-chain legacy\|strip` (Synth: Chain) | Firmware per-voice strip chain, ADR/R laws open (OQ22) | MIXED (partly VERIFIED) |
| `--filter-engine zdf\|firmware` (Synth: Filter) | Firmware synth filter (`synth-engine.md`) | VERIFIED vs oracle |
| `--measured-laws` (Synth: Measured laws) | 16-voice Super Saw, envelope depth, decay, pitch, vibrato (`audible-genius-synth-lessons.md`) | EMPIRICAL |
| `--synth-shape <0..1>` (Synth: Shape) | Shape word of the firmware SHAPE oscillator (strip chain); off keeps the saw placeholder | ASSUMED |
| `--firmware-piano-level` / `--legacy-piano-level` (Piano: Level) | Firmware velocity level law for `.npno` (default on); legacy is the touch curve (`piano-engine.md`) | VERIFIED |
| `--firmware-piano-bus` (Piano: SMP bus scale) | Sample path scaled 0.5 instead of 0.55 (Q31 receiver convention) | ASSUMED |
| `--amp-path engine\|firmware` (FX: Amp path) | Firmware drive stage (engine default) or engine tanh drive and RBJ cabinet; 4 frames latency | VERIFIED (`tests_amp_golden.rs`) |
| `--firmware-fx` (FX: Firmware FX) | Every `FirmwareFx` port (delay laws, delay time, reverb mix, compressor), see `effects-and-routing.md` | MIXED per port |
| `--firmware-routing` (Global: Firmware routing) | Section/layer enables and per-layer KB zones as stored (D22); zone codes open | ASSUMED |
| `--firmware-all` / `--legacy-all` | Presets: verified paths on (not measured laws, routing, Vox/Farfisa/B3) / all defaults | - |

Defaults are unchanged: with no flag the engine and the `render_hash` stay as before. The same flags
work for `ns4 render-patch`; both parse them with `FirmwareOptions::from_args`, so a switch added to
that struct needs a field, a parser, `apply_to`/`apply_exact`, a `summary` entry, a `SwitchInfo` in
`firmware_panel/info.rs`, a bar control and a row in this table (a unit test checks the table).

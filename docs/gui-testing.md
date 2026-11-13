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

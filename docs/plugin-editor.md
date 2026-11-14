# Plugin editor: build and host smoke test

The CLAP/VST3 plugin has an egui editor (`crates/nord-plugin/src/editor/`, spec: `docs/specs/daw-plugin.md`, section Editor). It was verified by unit and headless frame tests only. **It has not been loaded in a DAW.** Run the smoke test below in Reaper and Bitwig before ticking the host item of B11 (`ns4-hud1`) and B13.

## Build

```bash
cargo xtask bundle nord-plugin --release
```

Bundles land in `target/bundled/` (`nord-plugin.clap`, `nord-plugin.vst3`). Copy them to the host's plugin folder (Windows: `%COMMONPROGRAMFILES%\CLAP`, `%COMMONPROGRAMFILES%\VST3`; macOS and Linux: the usual user folders). Quick check without a host: `cargo build -p nord-plugin --release`.

## Smoke test (Reaper, Bitwig)

1. Rescan plugins; add "Nord Stage 4" to an instrument track. Expect: no crash, the editor opens and shows the red panel with the keybed.
2. Click keys on the on-screen keybed: Organ A sounds. Hold and release a key: no stuck note.
3. Drag the Master Level knob and an Organ drawbar. In the host's parameter list `master_level` and `drawbar_*` move; arm write automation, drag again, play back: the knob follows the lane.
4. Move a parameter from the host (automation lane or generic UI): the panel knob follows within a frame.
5. Hover a dimmed control: tooltip "not automatable in the plugin yet".
6. Drag the window corner: the panel scales and stays centred; close and reopen: the size is kept; save and reload the project: the size is kept.
7. Change the host's DPI scaling (or move the window to a HiDPI display): the panel stays sharp and fully visible.
8. Record the host name, version, OS and the results in `docs/research/` with the date.

## What the editor automates

See the Editor section of `docs/specs/daw-plugin.md`. Of the 138 inventory controls bound to a host id (after merging main at the organ/piano rewire), 122 are live, 13 are Shift/Hold functions that share a rectangle with a live control (not separately automatable), and 3 are dimmed (`organ.zone_leds`, `piano.zone_leds`, `program.layer_scene_ii`).

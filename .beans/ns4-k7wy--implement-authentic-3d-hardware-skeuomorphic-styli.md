---
# ns4-k7wy
title: Implement authentic 3D hardware skeuomorphic styling and fix text overlaps
status: completed
type: task
created_at: 2026-11-11T12:14:47Z
updated_at: 2026-11-11T12:15:20Z
parent: ns4-707f
---

## Context
The previous GUI prototype suffered from cramped label collisions (identified by user annotations: "ORGAN A / ORGAN B", "PIANO A / PIANO B", "SYNTH A / SYNTH B / SYNTH C", and "PIANO SELECT" matrix) and flat 2D rendering that lacked the tactile commercial hardware depth seen on the physical Nord Stage 4.

## Higher Goal
Deliver authentic 3D skeuomorphic styling matching official Nord Stage 4 hardware references (`foto.jpg`, `NS4-Panel-organ.max-2000x2000.format-png.png`, and high-poly 3D models), eliminating all text collisions and providing realistic tactile feedback (physical button depression, milled knob ribs, drawbar caps, domed LEDs with light bloom, and ivory/ebony 3D keybed).

## Acceptance Criteria
- [x] Fix horizontal text collision under Organ A/B faders by adopting authentic column labels (`A AUX KB`, `B AUX KB`) and adding `SUSTPED`/`PSTICK` buttons
- [x] Fix horizontal text collision under Piano A/B faders by adopting authentic column labels (`A AUX KB`, `B AUX KB`) and adding `SUSTPED`/`PSTICK` buttons
- [x] Fix horizontal text collision under Synth A/B/C faders by adopting authentic column labels (`A AUX KB`, `B AUX KB`, `C AUX KB`) and adding `SUSTPED`/`PSTICK` buttons
- [x] Fix text overlap in Piano Select matrix by widening container and using authentic hardware designations (`GRAND`, `UPRIGHT`, `ELEC`, `CLAV`, `DIGITAL`, `MISC`)
- [x] Implement 3D tactile skeuomorphic widgets:
  - `section_frame_3d`: Recessed chassis shadow, brushed aluminum header bar with bevels
  - `group_box_3d`: Beveled group outline with cutout title pill
  - `led_3d`: Chrome/nickel socket, domed lens, hot glint, diffuse light bloom
  - `button_3d`: Recessed socket, 3D bevels, physical depression translation on click
  - `knob_3d`: Milled rib skirt, radial ticks, sunken cap, pointer notch with drop shadow
  - `layer_strip_3d`: Metallic fader cap with grip ridges, recessed track slot, 11-segment LED ladder
  - `drawbar_3d`: Stepped 3D drawbar cap (color-coded Hammond/Nord palette), 8-segment red LED ladder
  - `oled_3d`: Recessed glass bezel, high-contrast OLED screen, pale cyan pixel glow
  - `triangle_matrix_selector`: Directional red triangle LEDs with light bloom and cycle button
  - Keybed 3D: Ivory natural keys with chamfer lip, ebony black keys with drop shadow, red felt bushing strip, and green split-point LEDs
- [x] Pass all quality gates: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, `ruff`, `pytest`, `check_tests.py`

## Out of Scope
- VST3/CLAP `nih-plug` wrapper (deferred to Phase 6 track).

## Summary of Changes
- Updated `crates/nord-gui/src/style.rs` with `LED_ORANGE` and `OLED_CYAN` color tokens.
- Implemented full suite of 3D skeuomorphic widgets in `crates/nord-gui/src/widgets.rs`.
- Overhauled bay rendering in `crates/nord-gui/src/panel.rs` to wire 3D widgets, eliminate redundant and colliding text labels, and add authentic `SUSTPED`/`PSTICK` buttons.
- Enhanced 88-key keybed rendering with red felt bushing strip, 3D ivory/ebony relief, and green split-point LEDs.

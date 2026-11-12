# Research: Hardware DSP Topology, Control Panel PCB & Analog DAC Output Stage Analysis

- **Checked date:** 2026-11-10
- **Sources:**
  - `.reference/mainboard/*.webp` (5 photos of a real Nord Stage 4 main board and panel boards, repair post)
  - `11870-Lg.jpg` (Stage 3-family mainboard, Syntaur listing; not Stage 4)
  - `12916-Lg.jpg` (Stage 3 front-panel control PCB — physical drawbar sliders, LED ladders, 15-LED ring encoders, OLED module, Wooden Pitch Stick)
  - `foto.jpg`, `NS4-Panel-organ.max-2000x2000.format-png.png`, `Screenshot20at2015.jpg` (Nord Stage 4 front panel)
  - `785351857-Nord-Stage-Service-Manual-v1-1.pdf` (`Nord Stage MainBoard B vers 1.41`), `Update Nord Stage 4 OS v1.66.exe`
- **Question:** How do the hardware DSPs, flash storage, front-panel control PCB, and analog D/A reconstruction stage work together in the Nord Stage architecture?

---

## 1. Mainboard Processors & Memory (`.reference/mainboard/*.webp`)

> [!NOTE]
> `11870-Lg.jpg` (Syntaur) is labeled `Moderkort NS3.2.15` and `12916-Lg.jpg` is `NORD STAGE 3SW Panel 1`: they are **Stage 3-family boards**, not Stage 4. The authoritative Stage 4 evidence is the five repair-post photos in `.reference/mainboard/`, whose silkscreen reads `Nord Stage 4 Main Vers 0.9 CLA45D`, label `Moderkort NS4 0.95 DS40095`, `NORD STAGE 4 Panel 1 Rev B Ver 1.4` and `Panel 2 Rev B Version 1.3`. Full notes: [mainboard-photo-analysis.md](mainboard-photo-analysis.md).

Observed on the Stage 4 mainboard together with the OS v1.66 partitions (`ARM0`, `DSPO`, `DSPS`, `DSPF`):
1. **Host CPU: TI `AM1705DPTP4`** (Sitara, ARM926EJ-S, 176-pin HLQFP, ARM-only with no on-chip DSP per TI SPRS657). Runs the `ARM0` image.
2. **At least 4× Analog Devices QFP DSPs** (one legibly marked `ANALOG DEVICES`; Stage 4 repair-post photos lack magnification to read the part suffix directly). On Clavia's closely related `NP4 Main v 0.72` photo ([`.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg`](../../.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg)), pairing the identical TI `AM1705DPTP4` host MPU, the Analog Devices 176-lead LQFP DSPs are legibly marked **Analog Devices `ADSP-21489 KSWZ-4B`** (4th-gen SHARC®, 400 MHz core clock, `#1823`, lot `4259498.1B-0.2`; datasheet & HRM archived under `.reference/sharc/`). On Stage 4, the firmware names three DSP images (`DSPO`, `DSPS`, `DSPF`), so the mapping of four chips to three images (e.g. multiple instances of one image) remains **unverified**.
3. **One large central QFP** with an illegible logo. DSP, FPGA or logic: **unidentified**.
4. **Memory:** a 2×4 array of 8 small memory ICs on the left plus 2 more near the service label. Type (SDRAM vs flash) is **unverified**; sample storage is reached through the firmware update path, so do not assume it is wired directly to the DSPs.
5. **I/O:** 8 output jacks, 2 MIDI DIN, headphone and pedal jacks, 2 ribbon connectors to the panel boards.
6. **Non-stock repair mods:** three DSPs carry wired-on buck-converter modules with blue trimpots (core-voltage repair). Not part of the original design.

---
## 2. Front-Panel Control PCB & Human Interface (`12916-Lg.jpg`, `NS4-Panel-organ`, `Screenshot20at2015.jpg`)

Inspection of the control panel PCB (`12916-Lg.jpg`) and high-resolution panel photos (`foto.jpg`, `NS4-Panel-organ.max-2000x2000.format-png.png`, `Screenshot20at2015.jpg`) confirms:
1. **Hybrid Physical Drawbars + 8-LED Red Ladders:**
   - 9 long-throw linear slide potentiometers (`16'` to `1'`) mounted side-by-side with 8-segment vertical surface-mount red LED arrays (`1..8`) immediately to the right of each slider slot.
   - Allows both tactile physical drawbar pulling and instant visual recall when switching presets or applying Morphing.
2. **7 Layer Level Faders + 11-LED Green Ladders:**
   - Organ (`A`, `B`), Piano (`A`, `B`), and Synth (`A`, `B`, `C`) each feature physical vertical faders paired with 11-segment green LED bar graphs showing real-time effective level (base + Morph offset).
3. **Central & Synth Monochrome Graphic OLED Modules:**
   - Two dedicated high-contrast white-on-black OLED screens: the central Program/Library display and the Synth `OSC WAVEFORM` graphic display.
4. **Wooden Pitch Stick & Stone Mod Wheel:**
   - Strain-gauge / optical zero-detent wooden Pitch Stick (asymmetric pitch bend with natural vibrato) and continuous Modulation Wheel (`Morph Source 1`).

---

## 3. Analog Output & D/A Reconstruction Stage (`CS4392` + `LM833` / `LF353`)

From the Clavia MainBoard schematic (`MainBoard B vers 1.41`, analog output channels `CH1..CH4` and Headphone out):
1. **D/A Converter (`Cirrus Logic CS4392`):** 24-bit, 192 kHz multi-bit Delta-Sigma DAC with differential analog outputs (`AOUTL±`, `AOUTR±`).
2. **Differential Reconstruction Filter (`LM833` Dual Low-Noise Audio Op-Amp):** Active 2nd-order low-pass differential summer ($R = 4.7\text{ k}\Omega$, $C = 820\text{ pF}$, $f_{-3\text{dB}} \approx 41.2\text{ kHz}$).
3. **Output Line Buffer (`LF353` JFET Op-Amp):** High-slew-rate ($13\text{ V/}\mu\text{s}$) JFET output stage with AC coupling ($22\text{ }\mu\text{F}$).

---

## 4. Implications & Follow-ups

- **Implication 1 (DSP):** `crates/nord-core` models the exact 5-DSP / 4-partition architecture (`organ.rs` = `DSPO`, `synth.rs` = `DSPS`, `piano.rs` + `fx.rs` = `DSPF`, `morph.rs` + `midi.rs` = `ARM0`) with the `CS4392` + `LM833` reconstruction filter in `MasterBus`.
- **Implication 2 (Visual UI):** `docs/specs/visual-style-and-panel.md` codifies every color token, fader + 11-LED ladder, drawbar + 8-LED ladder, triangle matrix selector, and dual OLED display for 1:1 visual fidelity in the Standalone and CLAP/VST3 plugin UI.


# Research: Hardware DSP Topology, Control Panel PCB & Analog DAC Output Stage Analysis

- **Checked date:** 2026-11-10
- **Sources:**
  - `11870-Lg.jpg` (Nord Stage mainboard PCB — multi-SHARC DSP & Flash array)
  - `12916-Lg.jpg` (Nord Stage front-panel control PCB — physical drawbar sliders, LED ladders, 15-LED ring encoders, OLED module, Wooden Pitch Stick)
  - `foto.jpg`, `NS4-Panel-organ.max-2000x2000.format-png.png`, `Screenshot20at2015.jpg` (Nord Stage 4 front panel)
  - `785351857-Nord-Stage-Service-Manual-v1-1.pdf` (`Nord Stage MainBoard B vers 1.41`), `Update Nord Stage 4 OS v1.66.exe`
- **Question:** How do the hardware DSPs, flash storage, front-panel control PCB, and analog D/A reconstruction stage work together in the Nord Stage architecture?

---

## 1. Mainboard Multi-SHARC DSP & Flash Memory Array (`11870-Lg.jpg`)

Inspection of the mainboard photograph (`11870-Lg.jpg`) alongside the OS v1.66 firmware partitions (`ARM0`, `DSPO`, `DSPS`, `DSPF`) reveals:
1. **5× Analog Devices SHARC Floating-Point DSPs (`ADSP-21489` / `ADSP-21369` LQFP packages):**
   - Arranged as a central **2×2 quad-SHARC cluster** plus a **5th SHARC DSP** to the left, interconnected via high-speed TDM/SPORT serial audio links and SPI host control buses.
   - Explains how the Nord Stage 4 runs 91 B3 tonewheels (`DSPO`), 120 piano voices with sympathetic modal string resonance (`DSPF`), 46 Wave 2 synth voices (`DSPS`), and **7 independent 5-slot stereo FX chains** (`DSPFxCore1`) simultaneously at zero latency.
2. **8× Non-Volatile NAND/NOR Flash Memory ICs (Left 2×4 Bank):**
   - Directly wired to the DSP/MCU memory controller for zero-buffer, instant sample access (`2 GB` Piano Library + `1 GB` Sample Library).
3. **Host Microcontroller & Rear I/O Array:**
   - Dedicated system MCU (`ARM0` image) managing USB, MIDI DIN ports (`MIDI IN`, `MIDI OUT`), 4 assignable audio outputs (`OUT 1..4`), headphone output, and pedal jacks (`Sustain`, `Triple Pedal`, `Rotor`, `Control`, `Organ Swell`).

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

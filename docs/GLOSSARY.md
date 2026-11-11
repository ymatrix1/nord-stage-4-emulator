# Glossary

| Term | Definition |
|---|---|
| **`ARM0`** | Primary ARM microcontroller partition in the Nord Stage 4 firmware responsible for keybed scanning, OLED UI, USB/MIDI, program storage, and Morphing Matrix control. |
| **`CBIN`** | *Clavia Binary* (`43 42 49 4E`), the 48-byte header container format used by all Nord Stage 4 preset and sample files (`.ns4p`, `.ns4y`, `.ns4o`, `.ns4n`, `.ns4l`, `.ns4t`, `.npno`, `.nsmp4`). |
| **`DSPF`** | SHARC DSP firmware partition dedicated to per-layer Effects processing (`DSPFxCore1`) and Piano String Resonance. |
| **`DSPO`** | SHARC DSP firmware partition (`DSPOrg`) dedicated to the 2-layer Organ physical modeling engine and Rotary Speaker simulation. |
| **`DSPS`** | SHARC DSP firmware partition (`DSPSyn`) dedicated to the 3-layer Synth engine (derived from the Nord Wave 2). |
| **Foldback** | Acoustic Hammond B3 wiring behavior where notes below the lowest tonewheel (`C1`) or above the highest tonewheel (#91) wrap back an octave rather than going silent or aliasing. |
| **Layer Scene II** | Instant dual-scene toggle on the Nord Stage 4 that switches which combination of the 7 layers (`Organ A/B`, `Piano A/B`, `Synth A/B/C`) is active with a single button press. |
| **Modal String Resonance** | Real-time DSP bank of tuned resonant filters excited by played notes whenever the sustain pedal is depressed or other keys are held undamped. |
| **Morphing Matrix** | Nord's 3-source continuous modulation system allowing `Mod Wheel` (CC1), `Control Pedal` (CC11), and `Aftertouch` to sweep any knob from a base value to a target offset across all 7 layers simultaneously. |
| **`.npno`** | Nord Piano Library compressed multisample container (`CBIN` tag `npno`). |
| **`.ns4b`** | Nord Stage 4 Backup archive (standard ZIP file containing `meta.xml`, Programs, Presets, and Sample/Piano banks). |
| **`.ns4l`** | Nord Stage 4 Live slot file (868 bytes, `CBIN` tag `ns4l`, schema `0x0139` — structurally identical to `.ns4p`). |
| **`.ns4n`** | Nord Stage 4 Piano Preset file (195 bytes, `CBIN` tag `ns4n`, schema `0x00CB`). |
| **`.ns4o`** | Nord Stage 4 Organ Preset file (183 bytes, `CBIN` tag `ns4o`, schema `0x00CD`). |
| **`.ns4p`** | Nord Stage 4 Program file (868 bytes, `CBIN` tag `ns4p`, schema `0x0139`), storing all 7 layers, per-layer FX, splits, Scenes I/II, and Morph assignments. |
| **`.ns4t`** | Nord Stage 4 System/Settings table file (124 bytes, `CBIN` tag `ns4t`, schema `0x006C`). |
| **`.ns4y`** | Nord Stage 4 Synth Preset file (541 bytes, `CBIN` tag `ns4y`, schema `0x00D0`). |
| **`.nsmp4`** | Nord Sample Library 4 container (`CBIN` tag `nsmp`). |
| **PolyBLEP** | *Polynomial Band-Limited Step*, an efficient time-domain anti-aliasing correction applied around waveform discontinuities (sawtooth resets and pulse edges). |
| **Split Crossfade** | Smooth volume blending across any of the 3 keybed split points: `Off` (hard split), `Small` (±3 semitones / 6 st window), or `Large` (±6 semitones / 12 st window). |
| **ZDF** | *Zero-Delay Feedback* (Topology-Preserving Transform), a filter discretization technique that resolves the implicit feedback loop algebraically without a 1-sample delay. |

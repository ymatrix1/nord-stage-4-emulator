# Documentation Index

Canonical documentation for the **Nord Stage 4 Emulator** project (interoperability, research, preservation, and compatibility).

| Document | Purpose |
|---|---|
| [PRD.md](PRD.md) | Product goals, target users, scope, latency/polyphony budgets, non-goals |
| [ROADMAP.md](ROADMAP.md) | Phased delivery plan with tracked checkboxes (`- [ ]` / `- [x]`) and parallel lanes |
| [ARCHITECTURE.md](ARCHITECTURE.md) | System architecture, 3-engine / 7-layer DSP pipeline, crate layout, threading |
| [DECISIONS.md](DECISIONS.md) | Architectural Decision Records (`D1`–`D10`) |
| [OPEN-QUESTIONS.md](OPEN-QUESTIONS.md) | Open technical and design questions (`OQ1`–`OQ6`) |
| [GLOSSARY.md](GLOSSARY.md) | Hardware, DSP, and Clavia binary format terminology |

## Technical Specifications (`docs/specs/`)

| Spec | Subsystem |
|---|---|
| [specs/README.md](specs/README.md) | Overview of the technical specification suite |
| [specs/cbin-and-presets.md](specs/cbin-and-presets.md) | Clavia Binary (`CBIN`) container, `.ns4b` backup, `.ns4p`/`.ns4y`/`.ns4o`/`.ns4n`/`.ns4l`/`.ns4t` & firmware container layout |
| [specs/organ-engine.md](specs/organ-engine.md) | 2-layer Organ Engine: B3 91-tonewheel physical model, crosstalk, keyclick, percussion, vibrato scanner, Vox/Farfisa/Pipe, Rotary 122 |
| [specs/synth-engine.md](specs/synth-engine.md) | 3-layer Synth Engine (Nord Wave 2 derived): PolyBLEP VA, Wavetable, FM, SFZ/WAV samples, ZDF Ladder filters, ADR, LFO, Arpeggiator |
| [specs/piano-engine.md](specs/piano-engine.md) | 2-layer Piano Engine: 120-voice multisample playback, Sympathetic String Resonance modal DSP, Dynamic Pedal Noise, timbre curves |
| [specs/effects-and-routing.md](specs/effects-and-routing.md) | 7 independent per-layer FX pipelines (Mod 1, Mod 2, Amp/EQ, Delay, Reverb), Master Compressor, Master Clock |
| [specs/morph-and-zones.md](specs/morph-and-zones.md) | 4-zone keybed router, Small/Large split crossfades, Layer Scene II toggle, Triple-Sensor re-trigger, 3-source Morphing Matrix |
| [specs/midi-and-control.md](specs/midi-and-control.md) | Complete MIDI CC map, Bank Select / Program Change, NRPN/SysEx, and panel control surface mapping |
| [specs/visual-style-and-panel.md](specs/visual-style-and-panel.md) | 1:1 Front-Panel visual layout, color tokens, physical faders + 11-LED ladders, drawbars + 8-LED ladders, dual OLED displays |
| [specs/verification.md](specs/verification.md) | CI pipeline, unit-test coverage ratchet (`tools/check_tests.py`), deterministic offline audio verification, round-trip CBIN tests |

## Research & Reverse Engineering (`docs/research/`)

| Research Note | Focus |
|---|---|
| [research/ns4-backup-and-cbin-teardown.md](research/ns4-backup-and-cbin-teardown.md) | Empirical teardown of `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (`product_id=46`, build `2934`, CBIN header layout, embedded bitstreams) |
| [research/ns4-firmware-teardown.md](research/ns4-firmware-teardown.md) | Empirical teardown of `Update Nord Stage 4 OS v1.66.exe` (`RCDATA 101`, `ARM0` host MCU vs `DSPO`, `DSPS`, `DSPF` SHARC DSP images) |
| [research/mainboard-photo-analysis.md](research/mainboard-photo-analysis.md) | Stage 4 mainboard/panel photo identification (AM1705 host, ADI DSPs, board revisions) |
| [research/hardware-and-dac-analysis.md](research/hardware-and-dac-analysis.md) | Mainboard topology (AM1705 host, ADI DSPs), memory, CS4392 DACs, and LM833/LF353 analog reconstruction filter analysis |



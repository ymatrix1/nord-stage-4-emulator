# Product Requirements Document (PRD)

**Status:** v1.0 specification · Synced with `main` · **Project:** Nord Stage 4 Emulator (`nord-stage-4-emulator`)

## 1. Mission & Purpose

The **Nord Stage 4 Emulator** is an open-source software synthesizer, patch/backup interoperability toolkit, and high-level DSP emulator built for **interoperability, research, preservation, and compatibility** with the Clavia Nord Stage 4 hardware keyboard (`product_id=46`).

Unlike generic virtual instruments that only mimic the visual front panel (such as freeware ROMplers), this project models the real internal architecture documented and verified from hardware/firmware analysis:
1. **Bit-level format interoperability** with Clavia Binary (`CBIN`) preset files (`.ns4p`, `.ns4y`, `.ns4o`, `.ns4n`, `.ns4l`, `.ns4t`) and `.ns4b` backup archives so musicians can inspect, convert, back up, diff, and load real Nord Stage 4 programs.
2. **Three specialized DSP sound engines across 7 multitimbral layers**:
   - **Organ Engine (2 Layers: A/B):** 100% mathematical physical modeling of 91 spinning B3 tonewheels, harmonic leakage/crosstalk, key click, 2nd/3rd harmonic percussion, C1–C3/V1–V3 scanner vibrato, Vox Continental & Farfisa transistor organs, two Pipe organ models, and the Model 122 Vintage Rotary Speaker simulation.
   - **Piano Engine (2 Layers: A/B, 120-voice polyphony):** High-speed RAM multisample playback (SFZ/WAV open libraries) coupled with real-time modal DSP for **Advanced String Resonance** (sympathetic undamped string vibrations) and **Dynamic Pedal Noise**.
   - **Synth Engine (3 Layers: A/B/C, 46-voice polyphony):** Nord Wave 2 derived architecture combining **Virtual Analog** (anti-aliased PolyBLEP oscillators, sync, detune, sub), **Wavetable**, **2/3-operator FM**, and **Sample playback**, feeding selectable **Zero-Delay Feedback (ZDF)** filters (12/24 dB Classic Transistor Ladder, LP/BP/HP), ADR envelopes, LFOs, and an arpeggiator.
3. **7 Independent Per-Layer Effects Chains:** Every one of the 7 layers (`Organ A`, `Organ B`, `Piano A`, `Piano B`, `Synth A`, `Synth B`, `Synth C`) runs its own dedicated effects pipeline (`Mod 1` → `Mod 2` → `Amp/Drive/EQ` → `Delay` → `Reverb`), summed into a downstream **Master Compressor** and locked to a **Master Clock**.
4. **Hardware Control & Morphing Architecture:** 4 keybed split zones with selectable DSP crossfades (`Off`, `Small`, `Large`), **Layer Scene II** instant toggle, and a **3-Source Morphing Matrix** (`Mod Wheel`, `Control Pedal`, `Aftertouch`) capable of sweeping any continuous parameter across all 7 layers simultaneously.

---

## 2. Target Users & Use Cases

| Persona | Need |
|---|---|
| **Stage & studio keyboardists** | Inspect, audition, rehearse, and edit `.ns4p` / `.ns4b` programs on a laptop without hauling the 88-key hardware rig. |
| **Audio DSP researchers & developers** | Study multi-SHARC real-time DSP routing (`DSPO`, `DSPS`, `DSPF`), tonewheel physical modeling, modal piano resonance, and zero-delay feedback filters in clean, tested Rust code. |
| **Preservation & archival engineers** | Document the Clavia `CBIN` v1 (`ns4*`) binary formats and OS v1.66 container structures so user patches remain readable decades after hardware end-of-life. |
| **Live gigging musicians** | Run a low-latency standalone or CLAP/VST3/WebAssembly stage instrument with full 7-layer splits, crossfades, Layer Scene II, and 3-source MIDI Morphing. |

---

## 3. Core Goals

- **G1. Format Interoperability (`CBIN` & `.ns4b`):** Parse, validate, inspect, and round-trip serialize Nord Stage 4 program (`.ns4p`), synth (`.ns4y`), organ (`.ns4o`), piano (`.ns4n`), live (`.ns4l`), settings (`.ns4t`), and backup (`.ns4b`) files.
- **G2. Authentic 3-Engine / 7-Layer DSP Pipeline:** Implement the Organ, Piano, and Synth engines with 7 independent per-layer FX racks and a global Master Compressor / Master Clock in a deterministic, real-time safe Rust core (`crates/nord-core`).
- **G3. Zero-Allocation Real-Time Audio Path:** Audio callback renders 7 layers + 7 FX chains with zero heap allocations, zero locks, and sub-3 ms processing time for a 128-sample buffer at 48 kHz on a standard desktop CPU.
- **G4. Complete Morphing & Split Zone Matrix:** Support up to 3 split points (4 zones) with `Off`, `Small` (6 semitones), and `Large` (12 semitones) linear/equal-power crossfades, Layer Scene II switching, and simultaneous 3-source Morphing (`Mod Wheel` CC1, `Control Pedal` CC11, `Aftertouch`).
- **G5. Full MIDI CC parity:** Implement the complete Nord Stage 4 MIDI CC map (see [specs/midi-and-control.md](specs/midi-and-control.md)) for hardware/software bi-directional control.
- **G6. 100% Tested Codebase:** Every Rust and Python source module ships unit tests enforced in CI via `tools/check_tests.py`.

---

## 4. Non-Goals

- **NG1. Low-Level SHARC / ARM CPU Instruction Emulation:** The hardware splits DSP across multiple Analog Devices SHARC processors (`DSPO`, `DSPS`, `DSPF`) and an `ARM0` host MCU communicating over custom SPI/SPORT buses without a dumped bootrom. Emulating SHARC opcodes at cycle level is unnecessary and impractical; high-level DSP emulation (HLE) in native Rust achieves superior audio performance and portability.
- **NG2. Breaking Proprietary Sample Encryption (`.npno` / `.nsmp4` Audio Streams):** While we parse `CBIN` headers and metadata of `.npno` and `.nsmp4` files for catalog/program compatibility, the Piano and Synth sample engines play standard open formats (WAV/SFZ/FLAC) mapped to the same program slot IDs.

---

## 5. Performance & Quality Budgets

| Metric | Target | Verification |
|---|---|---|
| Audio sample rate | 44.1 kHz / 48 kHz / 96 kHz (default 48 kHz) | Unit & offline render tests |
| Block render budget (128 samples @ 48 kHz = 2.67 ms) | ≤ 0.8 ms CPU time with all 7 layers + 7 FX chains active | Criterion / benchmark harness |
| Organ polyphony | Full 61/88-key polyphony (all 91 tonewheels active continuously) | `organ.rs` unit tests |
| Piano polyphony | Up to 120 voices with priority voice stealing + modal string resonance | `piano.rs` unit tests |
| Synth polyphony | Up to 46 voices across 3 layers (`Synth A/B/C`) | `synth.rs` unit tests |
| Format round-trip fidelity | Byte-exact header & bitstream preservation on unedited fields | `cbin.rs` round-trip tests |
| Unit test coverage gate | 100% of source files covered (`tools/check_tests.py` baseline = 0) | GitHub Actions CI |

# Technical Debt & Known Limitations

Technical debt register for the **Nord Stage 4 Emulator** project.
This document tracks deliberate trade-offs, structural compromises, temporary workarounds, and items slated for future refactoring.

---

## 1. Summary Matrix

| ID | Component | Severity | Description | Target Phase / Milestone | Status |
|---|---|---|---|---|---|
| **TD-01** | `crates/nord-gui` | Medium | Standalone GUI uses `eframe`/`egui` 0.31 rather than embedded `nih_plug_egui` vector UI inside `crates/nord-plugin` | Phase 6.1 (`crates/nord-plugin` editor) | Open |
| **TD-02** | `crates/nord-gui` | Medium | Single default MIDI input port bound on startup; no hotplug menu | Phase 6.1 (Settings Dialog) | Open |
| **TD-03** | `crates/nord-core::piano` | Low | Synthetic multi-sample fallback when no user `.npno` or SFZ bank is loaded | Phase 4 (`ns4-pno4`) | Resolved (SFZ + `.npno` loader in core & `nord-gui`) |
| **TD-04** | `crates/nord-core::organ` | Low | Simplified key-click transient model instead of physical contact bounce network | Phase 2.2 (`ns4-orgn`) | Open |
| **TD-05** | `crates/nord-core::engine` | Low | Boxed heap allocations for delay buffers occur during `Stage4Engine::new()` | Maintenance | Accepted (Zero RT allocs) |
| **TD-06** | `docs/research` | High | `CNSP` & LZO1X firmware unpacked/decoded; internal `ARM0`/`DSPO`/`DSPS`/`DSPF` SHARC symbol mapping remaining | Phase 7 (`ns4-trkb`, [D12](DECISIONS.md)) | In Progress |
| **TD-07** | `crates/nord-core::synth` | Medium | Arpeggiator & LFO are not yet tempo-synced across 7 layers | Phase 3.2 (`ns4-synt`) | Resolved (`crates/nord-core/src/synth.rs`) |

---

## 2. Detailed Technical Debt Registry

### TD-01: GUI Architecture (`egui` vs `nih-plug` VIZIA)
- **Context:** [D11](DECISIONS.md) envisioned unified VST3/CLAP DAW plugin export alongside standalone via `nih-plug`. `crates/nord-gui` was developed with `eframe` and `egui` to deliver rapid cross-platform GUI interaction, crisp OLED graphics, and responsive control surface widgets.
- **Debt:** Currently, the standalone runner is decoupled from DAW plugin host automation interfaces.
- **Remediation Plan:** When implementing `crates/nord-plugin`, either wrap the `egui` canvas using `nih_plug_egui` (unifying standalone and VST3/CLAP UI) or provide standard parameter automation bridges between `nih-plug` parameters and `Stage4PanelState`.

### TD-02: MIDI & Audio Device Selection
- **Context:** `audio.rs` auto-selects the system default CPAL output device and the first available MIDIR input port.
- **Debt:** Users with multiple MIDI controllers (e.g. secondary keyboard or pedalboard) or dedicated ASIO/CoreAudio interfaces cannot switch devices from a GUI settings menu.
- **Remediation Plan:** Introduce a modal settings dialogue or menu bar to list available CPAL audio devices, buffer sizes, and MIDIR input ports.

### TD-03: Piano Engine SFZ Multi-Sample Loader
- **Context:** The Piano engine (`crates/nord-core/src/piano.rs`) provides 120-voice polyphonic velocity tracking, touch curves, and undamped sympathetic resonance, but uses deterministic in-memory wavetable synthesis.
- **Debt:** Clavia's proprietary `.npno` / `CNSP` sample binaries are opaque and compressed without public decoders. Full acoustic realism requires real multi-velocity samples.
- **Remediation Plan:** Implement an open SFZ multi-sample parser and streaming voice reader (e.g. using Salamander Grand or VSCO-2 CE Upright).

### TD-04: Tonewheel Contact Bounce & Gear Ratios
- **Context:** Tonewheel frequencies in `organ.rs` currently use idealized 12-TET frequencies rather than the exact mechanical 196-tooth Hammond integer gear-wheel ratios.
- **Debt:** Missing the subtle micro-cent mechanical detunings and multi-contact bus bar bounce characteristic of vintage B3 consoles.
- **Remediation Plan:** Replace frequency table with exact tooth count integer divisions (`frequency = motor_rpm * teeth / 60`) and add contact bounce bandpass impulse filtering.

### TD-05: Pre-Allocated Boxed Buffers in `Stage4Engine`
- **Context:** Windows thread stacks are limited to 1 MB by default. Initializing 7 layer delay lines, FDN reverb matrices, and scanner buffers on the stack caused stack overflow (`0xc00000fd`).
- **Debt / Trade-off:** Buffer memory is allocated on the heap inside `Box::new()` during initialization (`Stage4Engine::new()`).
- **Status:** **Accepted Architecture Invariant**. Real-time DSP rendering (`render_frame` / `process_block`) remains strictly zero-allocation and lock-free.

### TD-06: Firmware Internal SHARC / ARM0 Symbol & Lookup Table Mapping
- **Context:** Factory backup `.ns4b` `CNSP` decompression (`.npno` / `.nsmp4`) and full-stream Clavia LZO1X firmware decompression (`Image 1` `798,912 B`, `Image 0` `5,499,504 B`) are now solved and implemented cleanly in `crates/nord-core` and integrated into `nord-gui` and `ns4` CLI.
- **Remaining Debt:** While samples and presets load directly from user `.ns4b` archives, the internal filter/envelope/FX lookup tables inside the unpacked `ARM0`, `DSPO`, `DSPS`, and `DSPF` (`ADSP-21489`) binary images are not yet mapped symbol-for-symbol to the Rust DSP models.
- **Remediation Plan (Track B — [D12](DECISIONS.md)):**
  1. Map internal `ARM0` / `DSPO` / `DSPS` / `DSPF` functions and parameter calibration tables from the unpacked OS v1.66 firmware images.
  2. Calibrate `Stage4Engine` filter, envelope, and FX response curves against Stage 4 hardware recordings.

---

## 3. Maintenance Policy

1. Every newly introduced structural compromise or temporary fallback must be logged here with a **TD-XX** identifier.
2. When a roadmap phase or bean resolves a technical debt item, update its status to `Resolved` and link the corresponding commit/PR.

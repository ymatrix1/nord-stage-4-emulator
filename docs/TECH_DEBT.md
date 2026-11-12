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
| **TD-06** | `docs/research` | Medium | `CNSP` & LZO1X unpacked; `ARM0` MurmurHash2 & OS v1.66 128-step DSP parameter curves mapped (`sharc-dsp-and-preset-hash-teardown.md`); SHARC VISA code found in `bank0_CODE@0x440000..0x52FFFF` ([dsp-image-hunt.md](research/dsp-image-hunt.md) 5.5/5.6), disassembly and DSP mapping open | Phase 7 (`ns4-trkb`, [D12](DECISIONS.md)) | Partial (ARM0/DSP curves calibrated) |
| **TD-07** | `crates/nord-core::synth` | Medium | Arpeggiator & LFO are not yet tempo-synced across 7 layers | Phase 3.2 (`ns4-synt`) | Resolved (`crates/nord-core/src/synth.rs`) |
| **TD-08** | `crates/nord-core::synth` | Low | `SAMPLES` oscillator interpolates with 4-point Catmull-Rom; the hardware uses a 12-tap Kaiser-windowed sinc polyphase kernel (`crates/nord-core/src/sinc.rs`, exact, not wired into voices) | Phase 3.x (`ns4-synt`) | Open (kernel exact; wiring and selector open) |
| **TD-09** | `crates/nord-core` (`piano.rs` 1,848 lines, `fx.rs` 1,771, `synth.rs` 1,236, `engine.rs` 1,214, `firmware.rs` 1,035), `tools/video_ref.py` (1,556) | Low | Files far above the 400-line guideline of `.agents/rules/code-structure.md`; `crates/nord-sharc` already follows it (`core/`, `visa/`) | Split opportunistically when touched, behaviour-preserving with tests per module | Open |

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
- **Context:** Factory backup `.ns4b` `CNSP` decompression (`.npno` / `.nsmp4`), full-stream Clavia LZO1X firmware decompression (`Image 1` `798,912 B`, `Image 0` `5,499,504 B`), `ARM0` MurmurHash2 sound-bank resolver (`0x0a5a44`), and the 5×128 OS v1.66 DSP parameter tables (`0x53abe4..0x53b5e4`, 800 Hz rotary crossover, and rotary mic balance steps) are now mapped and wired into `crates/nord-core` and `nord-gui` (`docs/research/sharc-dsp-and-preset-hash-teardown.md`).
- **Remaining Debt:** Executable SHARC code for `DSPO` / `DSPS` / `DSPF` is present as VISA inside `bank0_CODE@0x440000..0x52FFFF` (about 270 KB, 8 blocks, same layout in v1.44 to v1.66). Operand decoding is partial (`tools/sharc_visa.py`). The five DSP images (FX, PNO, ORG, SY1SY2, SMP; 916,780 bytes, `0x44667C..0x5263A8`) and the routine that hands them to the DSP driver (`bank0_CODE@0x241C58`) are identified in [dsp-image-hunt.md](research/dsp-image-hunt.md) 5.12; the driver method behind the call is not. `bank1_CODE.bin@0x08cb7c..0x09fe8` is a region/test table, not a loader table.
- **Remediation Plan (Track B — [D12](DECISIONS.md)):**
  1. Finish the VISA decoder (multiplier/shifter/multifunction fields), map the five images to the Core1/Core2 names and decode the driver method behind the download call (`bank0_CODE@0x241C58`), then verify reverb/amp FX stages against the code. A SPI flash dump is no longer required to start.

### TD-08: Sample Interpolation Kernel
- **Context:** The firmware's synth DSP image holds six 12-tap x 256-phase Kaiser-windowed sinc kernels (`docs/research/sy-dsp-map.md` section 2; `beta = 8.5`, half width 6, cutoffs 1.10, 1.05, 1.00, 0.96, 0.925, 0.895; max error 5.9e-8 = one float32 ulp, VERIFIED by `tools/ns4_sinc_check.py`). `crates/nord-core/src/sinc.rs` now implements this exact closed form (resolved: the earlier 3-term cosine window was a fit with rms 3.6e-5) and measures at least 5x lower error than the current 4-point Catmull-Rom on a 0.3 fs sine. The only DSP reader is the SY1SY2 resampler at `0x1C10EE`; no DSP instruction writes the voice's kernel pointer, so kernel selection is ARM-side.
- **Debt:** `synth/sample_osc.rs::cubic_pcm` and the piano sample path still use cubic interpolation. The rule that picks a kernel from the playback ratio is not decoded: `SincKernelSet::select` is ASSUMED (nearest `fc` to `min(1, 1 / ratio)`); confirm by tracing the ARM literal `bank0_CODE@0xC0001BAC`. The hardware sampler and piano path (SMP image) uses a different 6-tap x 256-phase Q31 kernel ([pno-dsp-map.md](research/pno-dsp-map.md) section 7; role ASSUMED, window not identified, not implemented), so the 12-tap float sinc is the synth path only.
- **Remediation Plan:** Switch `sample_with_loop`/`render_sample_zone_stereo` to `SincKernelSet::for_ratio(..).interpolate`, compare CPU cost per voice (12 MACs against 4) and re-run the sample-mode tests; identify the SMP 6-tap kernel for the piano path.

---

## 3. Maintenance Policy

1. Every newly introduced structural compromise or temporary fallback must be logged here with a **TD-XX** identifier.
2. When a roadmap phase or bean resolves a technical debt item, update its status to `Resolved` and link the corresponding commit/PR.

### TD-09: Oversized Source Files
- **Context:** `.agents/rules/code-structure.md` asks for focused modules under about 400 lines. `crates/nord-sharc` was split into `core/` and `visa/` submodules; the older files listed in the summary table were not.
- **Debt:** Large files mix several concerns (voice state, DSP blocks, parameter tables, tests), which slows review and invites duplicated helpers.
- **Remediation Plan:** When a change touches one of these files, first split it into a directory module with unchanged public API and tests per submodule, proving identical output (golden render hashes) before and after.

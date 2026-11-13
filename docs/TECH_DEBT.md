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
| **TD-04** | `crates/nord-core::organ` | Low | Simplified key-click transient model instead of physical contact bounce network; tonewheel waveform/phase arithmetic now follow the firmware oscillator bank, pitches stay the Hammond gear ratios (ASSUMED) | Phase 2.2 (`ns4-orgn`) | Partly resolved |
| **TD-05** | `crates/nord-core::engine` | Low | Boxed heap allocations for delay buffers occur during `Stage4Engine::new()` | Maintenance | Accepted (Zero RT allocs) |
| **TD-06** | `docs/research` | Medium | `CNSP` & LZO1X unpacked; `ARM0` MurmurHash2 & OS v1.66 128-step DSP parameter curves mapped (`sharc-dsp-and-preset-hash-teardown.md`); SHARC VISA code found in `bank0_CODE@0x440000..0x52FFFF` ([dsp-image-hunt.md](research/dsp-image-hunt.md) 5.5/5.6), disassembly and DSP mapping open | Phase 7 (`ns4-trkb`, [D12](DECISIONS.md)) | Partial (ARM0/DSP curves calibrated) |
| **TD-07** | `crates/nord-core::synth` | Medium | Arpeggiator & LFO are not yet tempo-synced across 7 layers | Phase 3.2 (`ns4-synt`) | Resolved (`crates/nord-core/src/synth.rs`) |
| **TD-08** | `crates/nord-core::synth` | Low | `SAMPLES` oscillator interpolates with 4-point Catmull-Rom; the hardware uses a 12-tap Kaiser-windowed sinc polyphase kernel (`crates/nord-core/src/sinc.rs`, exact) | Phase 3.x (`ns4-synt`) | Partly resolved: synth `SAMPLES` path wired to the sinc kernels (selector ASSUMED); piano 6-tap Q31 path resolved (SMP kernel bit-exact, `piano/q31_kernel.rs`) |
| **TD-09** | `crates/nord-core` (`piano.rs` 1,848 lines open; `fx.rs` 1,771, `synth.rs` 1,236, `engine.rs` 1,214, `firmware.rs` 1,035 split), `tools/video_ref.py` (1,556) | Low | Files far above the 400-line guideline of `.agents/rules/code-structure.md`; `crates/nord-sharc` already follows it (`core/`, `visa/`) | Split opportunistically when touched, behaviour-preserving with tests per module | Partly resolved: `synth/`, `engine/`, `fx/`, `firmware/` split; `piano.rs`, `tools/video_ref.py` open |
| **TD-10** | `crates/nord-core/src/fx.rs`, `cbin/fx_preset.rs`, `engine.rs`, `organ.rs` | Low | Preset-to-gain conversions are not tied to the firmware DSP tables: EQ gain uses 0.25 dB per count (table: 2^(1/24), 0.25086 dB), layer level is linear, drawbar law differs between spec (-3 dB per notch) and code (`level / 8`) | When the ARM-side preset writer is identified ([preset-dsp-map](research/preset-dsp-map.md)) | Open |

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
- **Context:** `organ/tonewheel.rs` uses the exact Hammond gear-ratio frequencies. **Resolved in this area:** the B3 waveform and phase arithmetic are the firmware oscillator bank (`u32` phase, `>> 20`, 4096-point x table, `organ/firmware_bank.rs`; [organ-tonewheel-bank](research/organ-tonewheel-bank.md)), with an optional detuned-copy pair (`COPY_SPREAD_CENTS`, EMPIRICAL).
- **Debt:** (a) the firmware pitch ladder is a 12-TET ladder with +-1.9 cent copies, not the Hammond gear ratios; which one the Nord uses (and the absolute tune/frame rate) is open. (b) The role of the bank (B3 tonewheels vs string resonance) is about 60 % ASSUMED. (c) Contact bounce is still the simple bandpass noise burst. (d) The mix stage (`0x12268F`) and the weights are not reproduced.
- **Remediation Plan:** Decode the ARM weight writers and the bank frame rate; then choose the pitch ladder and replace the key-click model with a contact-bounce network.

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
- **Debt:** the synth sampler is wired (`SAMPLE_INTERPOLATION = Sinc` in `synth/sample_osc.rs`; `cubic_pcm` stays as the switchable legacy path; cost: interpolator 5.3 -> 14.0 ns per sample, whole layer unchanged within noise, see `docs/specs/synth-engine.md` section 7). **Piano part resolved:** the piano sample path uses the exact SMP 6-tap x 256-phase Q31 kernel (`crates/nord-core/src/piano/q31_kernel.rs`, `SampleInterp::SmpQ31` default, `Cubic` kept as a switch): Hamming-windowed sinc with additive DC normalisation, bit-exact (VERIFIED, [smp-kernel-fit.md](research/smp-kernel-fit.md)). Open for the piano part: edge handling at stream start and end (the Rust path repeats edge samples, ASSUMED) and the ARM-side voice record (width, scale). The rule that picks a synth kernel from the playback ratio is not decoded: `SincKernelSet::select` is ASSUMED (nearest `fc` to `min(1, 1 / ratio)`); confirm by tracing the ARM literal `bank0_CODE@0xC0001BAC`. The 12-tap float sinc is the synth path only.
- **Oracle comparison ([oracle-vs-engine.md](research/oracle-vs-engine.md)):** against the Stage B routine run on the SHARC oracle, `SincKernelSet` is within 1 ulp (5.96e-8 abs) per output frame. Both differences found are fixed in `sinc.rs`: `interpolate` floors the 8-bit phase like the hardware (was nearest, 3.4e-3 error on a 0.3-sample start) and the cutoffs are f64 decimals (table max error 1.19e-7 -> 5.96e-8, bit-identical values 4,153 -> 9,355 of 18,432). `SincKernel::interpolate_ext40` (`sinc/ext40.rs`) reproduces the DSP's 40-bit accumulation for tests only. Open: edge handling (the engine repeats edge samples, the hardware ring is circular) and unity gain (engine multiplies by `fc`, firmware applies the voice gain).
- **Remediation Plan:** Done for the synth and piano paths (sample-mode tests re-run, golden diff limited to `SAMPLES`). Remaining: confirm the kernel-per-ratio rule (ARM trace).

---

## 3. Maintenance Policy

1. Every newly introduced structural compromise or temporary fallback must be logged here with a **TD-XX** identifier.
2. When a roadmap phase or bean resolves a technical debt item, update its status to `Resolved` and link the corresponding commit/PR.

### TD-09: Oversized Source Files
- **Context:** `.agents/rules/code-structure.md` asks for focused modules under about 400 lines. `crates/nord-sharc` was split into `core/` and `visa/` submodules; the older files listed in the summary table were not.
- **Debt:** Large files mix several concerns (voice state, DSP blocks, parameter tables, tests), which slows review and invites duplicated helpers.
- **Remediation Plan:** When a change touches one of these files, first split it into a directory module with unchanged public API and tests per submodule, proving identical output (golden render hashes) before and after.
- **Status:** `synth.rs`, `engine.rs`, `fx.rs` and `firmware.rs` are now directory modules (`synth/{mod,dsp,voice,render,...}`, `engine/{mod,midi_dispatch,program,program_fx,...}`, `fx/{mod,biquad,tables,types,modulation,amp_eq,delay,reverb,...}`, `firmware/{mod,parse,lz,lz_compress,...}`) with the public API re-exported unchanged and tests moved next to the code (`support.rs` holds shared fixtures). Largest remaining new file: `synth/render.rs` (about 450 lines, one 390-line `render_frame`; splitting it needs a behaviour-visible refactor of the per-voice loop). Still open: `piano.rs` (another lane), `organ.rs` (752), `bin/ns4.rs` (822), `cbin/*` (up to 1,009), `tools/video_ref.py`.
- **Evidence (behaviour preserved):** `cargo run --release -p nord-core --example render_hash -- out.bin` dumps 2,240,000 bytes (engine timeline, synth modes incl. looped sample at five ratios, organ, piano, a 25-config FX sweep through `MasterBus`). SHA-256 `f96c4757442c5068ec21731bbff93c387f8d6636c0502513f0fcc17a4105c6d4` before the splits (commit `f70f26c`) and after each split. Hot path: 4-note engine chord 2890 to 3050 ns/frame before and after (noise), one sample voice layer 114 to 117 ns/frame. The firmware split has no render output; its 14 moved tests plus an LZ round trip cover it.

### TD-10: Preset Conversion Curves Not Tied to Firmware Tables
- **Context:** `docs/research/preset-dsp-map.md` finds exact closed forms in the DSP images: a tangent prewarp `tan(2.19924e-5 * 2^(k/12))` (`DM 0xB0A63`) and a gain ladder `2^((j-353)/24)` then `16^(k/127)` (`DM 0xB05EB`). The ARM path from a preset field to these tables is not yet decoded.
- **Debt:** EQ gain counts use 0.25 dB per step (firmware ladder: 0.25086 dB), layer levels are linear `raw/127`, drawbars use `level/8` (isolated in `organ/drawbar.rs::drawbar_gain`, ASSUMED; the spec no longer claims -3 dB per notch as fact) and the firmware weight writer is not decoded.
- **Remediation Plan:** Identify the ARM writer of the strips and packed words, confirm which table each field indexes, then replace the constants in `fx.rs`, `fx_preset.rs`, `engine.rs` and `organ.rs` with the tables and add golden tests. Until then every mapping stays `ASSUMED`.

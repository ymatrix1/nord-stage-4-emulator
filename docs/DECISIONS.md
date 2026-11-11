# Architectural Decisions

Each decision records **Context, Decision, Why, Alternatives, Impact**, plus **Status** (`proposed`, `accepted`, `superseded`, `rejected`) and **Decided by** once decided.

## Index

| # | Date | Decision | Status |
|---|---|---|---|
| [D1](#d1-rust-core-for-dsp-engines-and-cbin-parsers) | 2026-11-10 | Rust core (`nord-core`) for DSP engines and `CBIN` binary parsers | accepted |
| [D2](#d2-high-level-dsp-emulation-over-low-level-sharc-instruction-emulation) | 2026-11-10 | High-Level DSP Emulation (HLE) over low-level SHARC instruction emulation | accepted |
| [D3](#d3-pure-physical-modeling-for-the-organ-engine-91-tonewheels--rotary-122) | 2026-11-10 | Pure mathematical physical modeling for the Organ Engine | accepted |
| [D4](#d4-hybrid-open-multisamples--modal-dsp-for-the-piano-engine) | 2026-11-10 | Hybrid open multisamples (SFZ/WAV) + modal DSP for the Piano Engine | accepted |
| [D5](#d5-polyblep-virtual-analog-and-zero-delay-feedback-ladder-filters-for-the-synth-engine) | 2026-11-10 | PolyBLEP VA oscillators and ZDF Ladder filters for the Synth Engine | accepted |
| [D6](#d6-seven-independent-per-layer-effects-chains-plus-global-master-compressor) | 2026-11-10 | Seven independent per-layer effects chains plus global Master Compressor | accepted |
| [D7](#d7-bit-exact-cbin-v1-header-and-bitstream-preservation) | 2026-11-10 | Bit-exact `CBIN` v1 header and bitstream preservation for round-trip compatibility | accepted |
| [D8](#d8-python--uv-for-reverse-engineering-and-ci-test-ratchet-tools) | 2026-11-10 | Python + `uv` for reverse-engineering utilities and CI test ratchet | accepted |
| [D9](#d9-beans-for-in-repo-task-tracking) | 2026-11-10 | `beans` for in-repo task tracking | accepted |
| [D10](#d10-every-source-file-ships-unit-tests-enforced-in-ci) | 2026-11-10 | Every source file ships unit tests enforced by `tools/check_tests.py` | accepted |
| [D11](#d11-dual-target-delivery-daw-plugin-clap--vst3-and-standalone-application) | 2026-11-10 | Dual-target delivery: DAW Plugin (`CLAP` + `VST3`) AND Standalone application | accepted |

---

## Accepted Decisions

### D1. Rust core for DSP engines and CBIN parsers

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** The emulator needs zero-allocation real-time audio synthesis, safe binary parsing of `.ns4p`/`.ns4b`/firmware files, cross-platform compilation (Windows, Linux, macOS, WebAssembly), and deterministic offline testing.
- **Decision:** Implement the sound engines, routing matrix, FX pipelines, and `CBIN` parsers in a pure Rust workspace (`crates/nord-core`).
- **Why:** Memory safety when parsing untrusted binary files, zero-cost abstractions for SIMD/DSP loops, native WebAssembly compilation for browser demos, and built-in `cargo test` / `clippy` tooling.
- **Alternatives:** C++/JUCE (heavy build dependencies, GPL/commercial licensing friction, harder memory safety in binary parsers); pure Python (too slow for real-time 91-tonewheel + 7-FX synthesis).
- **Impact:** Fast, dependency-light `cargo test` in CI; clean C/FFI/WASM boundary for future GUI/plugin hosts.

### D2. High-Level DSP Emulation over low-level SHARC instruction emulation

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** Firmware teardown (`Update Nord Stage 4 OS v1.66.exe`) revealed an `ARM0` host image alongside separate SHARC DSP images (`DSPO`, `DSPS`, `DSPF`), without factory mask bootroms or undocumented FPGA/CPLD bus routing.
- **Decision:** Build a High-Level Emulator (HLE) that implements the exact DSP signal topology, parameter curves, and preset formats natively, using static analysis of the firmware and `.ns4b` files as the ground-truth reference.
- **Why:** Emulating three parallel SHARC DSPs + an ARM MCU at the instruction level without bootroms or ASIC schematics is infeasible in real time, whereas native mathematical modeling of B3 tonewheels, ZDF filters, and modal resonators achieves 95%+ sonic parity at a fraction of the CPU load.
- **Alternatives:** Low-level SHARC/ARM instruction emulator (blocked by missing bootrom and custom inter-DSP buses).
- **Impact:** Firmware parser (`firmware.rs`) is used for static inspection and table extraction; audio synthesis runs natively in `nord-core`.

### D3. Pure physical modeling for the Organ Engine (91 tonewheels + Rotary 122)

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** The Nord Stage 4 Organ Engine (`DSPO`) uses zero samples; it models 91 spinning tonewheels, harmonic leakage, key click, percussion, scanner vibrato, and a Model 122 rotary speaker.
- **Decision:** Model all 91 phase-continuous tonewheel oscillators with authentic 196-tooth gear ratios, 9 drawbar harmonic buses, fast/slow 2nd/3rd harmonic percussion, and an 800 Hz crossover horn/drum Doppler rotary simulator.
- **Why:** Replicates true phase cancellation, foldback on the top/bottom octaves, and organic drawbar sweeps that static samples cannot capture.
- **Alternatives:** Sampled organ notes (fails foldback and phase-interaction behavior).
- **Impact:** Constant, predictable CPU cost regardless of how many keys are held (`organ.rs`).

### D4. Hybrid open multisamples + modal DSP for the Piano Engine

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** Nord Piano Library (`.npno`) audio streams use proprietary compression, while the Stage 4's realism comes heavily from its DSP string resonance, pedal noise, velocity curves, and timbre shaping.
- **Decision:** Parse `.ns4n` / `.npno` metadata for program compatibility while pairing open multisamples (SFZ/WAV, e.g., Salamander Grand) with a real-time sympathetic string resonance modal filter bank and dynamic pedal noise generator.
- **Why:** Preserves full interoperability with `.ns4p` programs and delivers authentic sympathetic resonance without redistributing proprietary `.npno` audio data.
- **Alternatives:** Attempting to decrypt proprietary `.npno` audio streams (fragile, non-redistributable).
- **Impact:** Clean open-source repo that works out of the box and supports user-supplied SFZ libraries.

### D5. PolyBLEP Virtual Analog and Zero-Delay Feedback Ladder filters for the Synth Engine

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** The 3-layer Synth Engine (`DSPS`, derived from Nord Wave 2) requires alias-free oscillators and nonlinear self-oscillating ladder filters.
- **Decision:** Use Polynomial Band-Limited Step (`PolyBLEP`) Virtual Analog oscillators paired with topology-preserving Zero-Delay Feedback (`ZDF` / TPT) 12/24 dB Transistor Ladder and State-Variable filters.
- **Why:** Eliminates aliasing discontinuities without costly 8x oversampling and preserves accurate resonance behavior up to Nyquist.
- **Alternatives:** Naive waveforms (severe aliasing); Chamberlin SVF (unstable at high cutoff/resonance).
- **Impact:** Crisp high-register synth leads and stable filter sweeps across all 3 synth layers (`synth.rs`).

### D6. Seven independent per-layer effects chains plus global Master Compressor

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** The biggest architectural leap in the Nord Stage 4 over the Stage 3 is that all 7 layers (`Organ A/B`, `Piano A/B`, `Synth A/B/C`) have their own dedicated effects chain (`Mod 1`, `Mod 2`, `Amp/Drive/EQ`, `Delay`, `Reverb`).
- **Decision:** Instantiate an independent `LayerFxChain` inside each of the 7 layers before summing into the global `MasterCompressor`.
- **Why:** Matches the real Stage 4 signal flow 1:1 and allows `.ns4p` presets with per-layer effects to sound as intended.
- **Alternatives:** Shared send buses (Stage 3 style; breaks Stage 4 preset compatibility).
- **Impact:** 7 lightweight FX racks run in parallel (`fx.rs`, `engine.rs`).

### D7. Bit-exact CBIN v1 header and bitstream preservation

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** `.ns4p` (868 B), `.ns4y` (541 B), `.ns4o` (183 B), `.ns4n` (195 B), `.ns4l` (868 B), and `.ns4t` (124 B) use a 48-byte `CBIN` header followed by a bit-packed payload where `.ns4p` embeds the engine preset bitstreams directly.
- **Decision:** Store the raw bitstream alongside decoded fields in `CbinFile` so round-trip serialization preserves unknown/reserved bits and CRC32 headers intact.
- **Why:** Guarantees zero data loss when reading, inspecting, modifying known parameters, and writing `.ns4*` files back for hardware use.
- **Alternatives:** Re-synthesizing payloads from scratch (corrupts unmapped bits).
- **Impact:** 100% lossless round-trip on all `.ns4*` files (`cbin.rs`).

### D8. Python + uv for reverse-engineering and CI test ratchet tools

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** Fast binary diffing, `.ns4b` ZIP inspection, and repository quality gates benefit from scripting alongside the Rust core.
- **Decision:** Use Python 3.13 managed by `uv`, linted with `ruff`, and tested with `pytest` under `tools/`.
- **Why:** Matches the workflow proven in `easy-piano`, `prosper`, and `PortPS5`.
- **Alternatives:** Shell scripts (non-portable on Windows).
- **Impact:** Cross-platform Python tools (`tools/ns4_inspect.py`, `tools/check_tests.py`) verified in CI.

### D9. beans for in-repo task tracking

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** AI agents and human contributors need structured, version-controlled task tracking with dependencies and acceptance criteria.
- **Decision:** Track epics and tasks in `.beans/` using `beans` (`prefix: ns4-` in `.beans.yml`).
- **Why:** Keeps task context, acceptance criteria, and completion summaries inside git commits.
- **Alternatives:** Ad-hoc TODO comments or external-only trackers.
- **Impact:** Every unit of work maps to a bean in `.beans/`.

### D10. Every source file ships unit tests enforced in CI

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive
- **Context:** Unverified DSP math or binary parsers regress easily.
- **Decision:** Enforce via `tools/check_tests.py` that every Rust (`#[test]`) and Python (`tools/tests/test_*.py`) source file has unit tests, keeping `tools/test-baseline.txt` at 0 uncovered files.
- **Why:** Prevents untested code from landing on `main`.
- **Alternatives:** Percentage line-coverage threshold only (allows whole new files to land untested).
- **Impact:** CI fails immediately if any new `.rs` or `.py` file lacks unit tests.

### D11. Dual-target delivery: DAW Plugin (CLAP + VST3) and Standalone application

- **Status:** accepted 2026-11-10
- **Decided by:** Yuri Olive ("i think we should build a plugin and also standalone")
- **Context:** Musicians use stage keyboards both inside a DAW (Ableton Live, Reaper, Logic/Bitwig, MainStage, Gig Performer) and as a direct low-latency Standalone desktop application.
- **Decision:** Ship both a **DAW Plugin (`CLAP` + `VST3`)** and a **Standalone desktop application** powered by the shared `crates/nord-core` `Stage4Engine` (via `nih-plug` / `nih_export_clap!` + `nih_export_vst3!` + `nih_plug::wrapper::standalone`).
- **Why:** One DSP core (`nord-core`) and one GUI/parameter definition compile to both native DAW plugins and a standalone executable with audio/MIDI device selection at zero duplication cost.
- **Alternatives:** Standalone only (cannot host inside a DAW project); plugin only (requires a DAW host just to play live).
- **Impact:** Phase 6 delivers both the standalone binary and the `.clap` / `.vst3` bundles in CI/CD releases.


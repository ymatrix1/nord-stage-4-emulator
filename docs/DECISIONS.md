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
| [D12](#d12-pursue-track-b-reverse-engineering-for-cnsp-sample-codec-and-firmware-decompression) | 2026-11-11 | Pursue Track B reverse engineering for `CNSP` sample codec and full LZ firmware decompression | accepted |
| [D13](#d13-adopt-the-third-party-ns4decode-bit-map-for-synth-0x32--0x2f-fields) | 2026-11-11 | Adopt the third-party ns4decode bit map for Synth `0x32` / `0x2F` fields | accepted |
| [D14](#d14-nih-plug-framework-for-clap-vst3-and-au-daw-plugin-architecture) | 2026-11-11 | NIH-plug framework for CLAP, VST3, and AU DAW plugin architecture | accepted |
| [D15](#d15-hybrid-blender-baked-sprite--egui-painter-25d-rendering) | 2026-11-12 | Hybrid Blender-baked sprite + egui painter 2.5D rendering on `nih_plug_egui` | accepted |
| [D16](#d16-phase-6b-scope-original-art-functional-or-stub-controls-ha88-first-additive-host-ids) | 2026-11-12 | Phase 6b scope: original art, functional-or-stub controls, HA88 first, additive host IDs | accepted |
| [D17](#d17-chip-identity-fx-pno-and-org-run-on-adsp-2157x-sharc-sy1sy2-and-smp-on-adsp-2148x) | 2026-11-12 | Chip identity: FX, PNO and ORG on ADSP-2157x (SHARC+), SY1SY2 and SMP on ADSP-2148x | accepted |
| [D18](#d18-rust-sharc-oracle-validated-against-the-adi-cycle-accurate-simulator) | 2026-11-12 | Rust SHARC oracle validated against the ADI cycle-accurate simulator (headless `TpsdkServer` runner) | accepted |
| [D19](#d19-arm-firmware-emulation-harness-unicorn-as-the-parameter-stream-source) | 2026-11-12 | ARM firmware emulation harness (`unicorn`) as the parameter-stream source | accepted |
| [D20](#d20-derived-values-only-no-firmware-bytes-in-the-repository) | 2026-11-12 | Derived values only: no firmware bytes in the repository | accepted |
| [D21](#d21-organ-model-to-dsp-path-map) | 2026-11-13 | Organ model to DSP path map (B3, Vox, Pipe) | accepted |
| [D22](#d22-opt-in-firmware-paths-in-nord-core-defaults-change-only-when-verified-end-to-end) | 2026-11-13 | Opt-in firmware paths in `nord-core`; defaults change only when verified end to end | accepted |
| [D25](#d25-the-organ-output-level-law-pipe-takes-it-by-default-b3wheels-vox-and-routing-stay-opt-in) | 2026-11-13 | The organ output level law; Pipe takes it by default, B3Wheels, Vox and routing stay opt-in | accepted |
| [D29](#d29-the-piano-layer-level-law-is-opt-in-the-bus-convention-stays-assumed) | 2026-11-14 | The piano layer level law `(v / 127)^2` is opt-in; the bus convention stays ASSUMED | accepted |
| [D26](#d26-fx-mod-1-a-pan-and-trem-use-the-firmware-routines-behind-firmwarefxmod1) | 2026-11-14 | FX Mod 1: A-PAN and TREM use the firmware routines behind `FirmwareFx::mod1` | accepted |
| [D27](#d27-the-synth-waveform-browser-uses-the-firmwares-category-tree-the-program-category-id-stays-unmapped) | 2026-11-14 | The Synth waveform browser uses the firmware's category tree; the program category id stays unmapped | proposed |

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
- **Update 2026-11-13:** the engine stays HLE. The SHARC core model in `crates/nord-sharc` is a test oracle for single routines and passes, not a runtime emulator ([D18](#d18-rust-sharc-oracle-validated-against-the-adi-cycle-accurate-simulator)); the original blocker (no boot ROM) does not apply because the oracle loads the images the way the ARM loads them.

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

### D12. Pursue Track B reverse engineering for CNSP sample codec and firmware decompression

- **Status:** accepted 2026-11-11
- **Decided by:** Yuri Olive ("we should try path B", "lets document that, not implement yet")
- **Context:** While Track A (clean-room mathematical physical modeling + open SFZ sample playback) remains the primary portable runtime architecture, the 233 factory sample files (195 `.nsmp4` and 38 `.npno` files inside the `.ns4b` archive) use the proprietary `CNSP` audio payload format. Additionally, we proved that the firmware `CODE` segment is 100% unencrypted and compressed with a custom 16-bit LZ token scheme (`distance = token >> 3`, `length = (token & 7) + 2`).
- **Decision:** Formally pursue **Track B** as a parallel reverse-engineering and interoperability effort to:
  1. Complete the control-bit/literal state machine for the Clavia custom LZ firmware decompressor to unpack the full `ARM0` host binary and embedded `DSPO`/`DSPS`/`DSPF` SHARC images.
  2. Locate and disassemble the `CNSP` sample decoding routine inside the unpacked `ARM0` / `DSPF` / `DSPS` binaries, or derive the `CNSP` bitstream format via differential analysis across `.nsmp4` and `.npno` headers (`0x30..0x80`).
  3. Enable native user-supplied `.nsmp4` and `.npno` sample loading directly from a user's legally owned `.ns4b` backup archive without redistributing proprietary sample assets in git.
- **Why:** Unlocks true 1:1 sonic compatibility with factory `.ns4p` programs that reference specific `.npno` pianos and `.nsmp4` synth samples, and extracts exact SHARC DSP filter coefficient tables.
- **Alternatives:** Relying strictly on open SFZ approximations (leaves `.npno` and `.nsmp4` payloads unreadable).
- **Impact:** Updates `docs/ROADMAP.md`, `docs/TECH_DEBT.md`, and `docs/research/1to1-emulation-feasibility.md` with structured Track B milestones before implementation begins.

### D13. Adopt the third-party ns4decode bit map for Synth `0x32` / `0x2F` fields

- **Status:** accepted 2026-11-11
- **Decided by:** Yuri Olive ("code is also here .reference/ns4decode_pkg", "incorporate everything we can")
- **Context:** Statistical diffing of the 384 factory `.ns4y` presets located the filter ADR, osc ADR, amp velocity and unison, but could not place LFO shape/target, mono/legato/priority, glide, octave, pitch, osc type/category/wave from names alone. ns4decode (MIT, Randy 2024) publishes a complete `.ns4p` bit map and a regression program with hand-checked expected values.
- **Decision:** Use ns4decode's offsets (converted to block-relative bits) for the Synth `0x32`/`0x2F` fields, tag the absolute offsets `EMPIRICAL` and the field order/widths `VERIFIED` against the firmware name tables (`tools/ns4_fw_order_check.py`), keep the code re-derived rather than copied, and gate regressions with `tools/ns4_crosscheck.py` (75/75 regression fields, 0 violations over 1152 factory layers).
- **Why:** The map agrees with independent data (bitscan found the same offsets; corpus values stay in legal ranges) and unblocks real decoding of LFO, voice mode, glide, octave and oscillator selection (resolves OQ4).
- **Alternatives:** Keep guessing from preset names (ambiguous); disassemble the `ARM0` preset parser (large effort, tracked in OQ7).
- **Impact:** `SynthLayerDecoded` gains the decoded fields; `pack()` rewrites only mapped fields and preserves all other bits; scales that ns4decode does not define stay `ASSUMED`.

### D14. NIH-plug framework for CLAP, VST3, and AU DAW plugin architecture

- **Status:** accepted 2026-11-11
- **Decided by:** Yuri Olive
- **Context:** To host the Nord Stage 4 Emulator across all major DAWs (Reaper, Bitwig, Ableton Live, Logic Pro, Studio One, FL Studio, Cubase), the plugin must provide CLAP and VST3 binaries natively, support AUv2 via wrapper on Apple platforms, preserve real-time audio safety, support multitimbral multi-channel routing, expose all 7-layer parameters with stable automation IDs, and allow backwards-compatible state serialization.
- **Decision:** Use `nih-plug` as the unified plugin framework (`crates/nord-plugin`), leveraging its native CLAP and VST3 wrappers, `clap-wrapper` for Audio Unit, sample-accurate transport and event handlers, and parameter management macros.
- **Why:** Avoids rewriting DSP logic; supports all required formats from a single trait implementation; guarantees real-time zero-allocation processing; integrates directly with existing `Stage4Engine` and `ProgramDecoded` preset serialization.
- **Alternatives:** `clack-plugin` (CLAP only, lacks native VST3 and AU wrappers); manual C-FFI / COM shims (unmaintainable boilerplate).
- **Impact:** Adds `crates/nord-plugin` workspace crate with targets for CLAP, VST3, and Standalone.

### D15. Hybrid Blender-baked sprite + egui painter 2.5D rendering

- **Status:** accepted 2026-11-12
- **Decided by:** Yuri Olive
- **Context:** Phase 6b ([ROADMAP](ROADMAP.md#phase-6b-11-hardware-parity--25d-panel-ns4-xc5n)) needs a 2.5D panel (depth, shadows, materials) shared by the standalone app and the plugin editor. `nord-gui` is `eframe` 0.31 on the `glow` (OpenGL) backend; the plugin editor uses `nih_plug_egui`, which runs on `baseview` + OpenGL. Checked 2026-11-12: the nih-plug checkout in the cargo cache pins `egui-baseview` (commit `ec70c3f`, 2025-02-04), which depends on `egui` / `egui_glow` 0.31, the same minor as `nord-gui` (`Cargo.lock`: egui 0.31.1); `nih_plug_egui` exposes only the OpenGL backend. Not yet verified by compiling a `nord-panel` crate against both. Drawing is GUI-thread only and must never lock the engine.
- **Decision:**
  - **Toolkit:** `nih_plug_egui` for the plugin editor, `eframe` for the standalone app, one shared `crates/nord-panel`. `nord-panel` pins egui 0.31 and upgrades only together with `nih_plug_egui`. Control state and hit-testing stay free of egui types so only the drawing layer is toolkit-specific.
  - **Baker:** hardware parts (chassis, plates, fader slots, knob caps, fader caps, buttons, keys) are modelled in Blender from our own geometry and rendered offline (scripted, headless) at 1x and 2x: static plates as atlases, knobs as 64-128 frame filmstrips (or one frame rotated for symmetric caps), fader caps and buttons as separate sprites so their shadows stay on their own layer. `xtask bake-panel` packs the committed renders into atlases deterministically. Flat vector layers (silkscreen text, section outlines, lines) are drawn with `tiny-skia` in the same command.
  - **Reproducibility:** Blender (Cycles/EEVEE) output is not bit-identical across GPUs, so the rendered PNGs are the committed source of truth with content hashes; CI verifies the hashes of the committed files and the atlas packing, and does not re-render. The render scripts and `.blend` sources are committed so renders can be regenerated deliberately.
  - **Dynamic layers (egui painter):** LED ladders and LED glow (dark off-image plus lit overlay), OLED pixels, text and Morph LEDs. One top-left light for the whole panel; a per-element height token drives shadow offset and blur for anything drawn live. Textures upload once; repaint at most 60 Hz and only on change.
  - **Reference asset:** `.reference/HIGH_POLY_KEYBOARD_STAGE_PIANO` (a Sketchfab fan model with Nord's panel artwork, licence unchecked) is a private layout and material reference only: control positions and proportions may be measured into data files. No render, trace or texture derived from it is committed or shipped (D16).
- **Why:** Pre-rendered 3D images give soft shadows, real materials and ambient occlusion that a 2D painter cannot reach, which is how commercial instrument UIs get their look. The egui painter keeps dynamic state exact and runs on both OpenGL hosts without custom GPU code, and sharing `nord-panel` with `nord-gui` avoids writing the panel twice. Committing renders with hashes keeps assets testable by the golden-image harness (B6) without requiring Blender in CI.
- **Alternatives:**
  - A. egui painter only: simplest, but flat materials, and per-frame cost rises with gradient/shadow meshes.
  - B. `tiny-skia` as the only baker (the original proposal): deterministic and light, but 2D vector cannot match rendered materials. Kept for the flat vector layers.
  - C. wgpu shader callbacks with real-time lighting: highest ceiling, rejected for now. `nord-gui` is `eframe`/`glow` and `nih_plug_egui` uses `baseview`/OpenGL, so a wgpu path would need a second backend in both hosts. A live lighting pass can be tried later because only the drawing layer would change.
  - D. `nih_plug_vizia`: nicer CSS-style widgets, but it uses a vizia fork tagged 2024-05-06 (upstream vizia 0.4.0 released 2026-04-23) and cannot share code with `nord-gui`.
  - E. Webview UI: heavy and unreliable inside plugin hosts.
- **Impact:** New `xtask bake-panel` command, committed renders, Blender sources and scripts, and atlases with hashes; Blender becomes an optional development dependency (not needed by CI or normal builds). `crates/nord-panel` render modules (`theme`, `material`, `shadow`, `atlas`, per-widget painters); spec [visual-style-and-panel.md](specs/visual-style-and-panel.md) updates in B7/B8. B7 (`ns4-77s9`) first proves one knob filmstrip and one fader cap end to end and measures atlas size before the full set. B6 (golden images) keeps its `tiny-skia` CPU raster. Renderer fragility on macOS OpenGL is to be checked in B11/B13 host testing.

### D16. Phase 6b scope: original art, functional-or-stub controls, HA88 first, additive host IDs

- **Status:** accepted 2026-11-12
- **Decided by:** Yuri Olive
- **Context:** Phase 6b ([ROADMAP](ROADMAP.md#phase-6b-11-hardware-parity--25d-panel-ns4-xc5n)) targets full front-panel parity with a 2.5D look. OQ8, OQ9, OQ10, OQ11 and OQ14 blocked the renderer and parameter lanes.
- **Decision:**
  1. **Art and branding (was OQ8):** all shipped art is original. No Nord logo or wordmark, no photo textures and nothing traced or rendered from Nord's imagery or third-party models of the hardware. Use a neutral wordmark such as "Stage 4 Emulator" and our own typeface and layout art; control labels may name the feature they control. Reference photos and models stay private in `.reference/`. **Photos as reference (clarified 2026-11-12):** photos and models of the hardware may be used to measure layout and proportions, sample colours into our own theme tokens, and judge shapes (knobs, faders, buttons) and how materials respond to light, so we can model and shade our own parts to match. They are never a source of pixels: no cropped or sampled patches, no tracing, no photogrammetry or image-to-3D, and textures are built procedurally or from our own or freely licensed sources. Only the measured numbers (positions, ratios, colour values) go into the repo, as data. This is a project decision, not legal advice.
  2. **Parity scope (was OQ9):** every manual control exists on the panel. A control is functional where the engine supports it (Undo and the Num Pad are included); otherwise it is a visible stub bound as `Unsupported` per [control-inventory.md](specs/control-inventory.md), and stubs are promoted one at a time. Preset Library, System, Organize and Extern menus start as stubs. Rear connectors are not drawn, or drawn as static decoration.
  3. **Keybed (was OQ10):** HA88 first, with the key count and width as layout parameters so the 73-key and Compact variants can be added later.
  4. **Per-layer host IDs (was OQ11):** additive only. Existing shared IDs (for example `synth_filter_cutoff`) keep their meaning as Layer A; `synth_b_*` and `synth_c_*` are added, built from wrapper structs (the pattern of `fx_params.rs`) so IDs come from one definition. B3 (`ns4-xhbf`) enforces additive-only with a snapshot test.
  5. **Phase structure (was OQ14):** reopen the Phase 6 bay ticks and add Phase 6b, as in the roadmap.
  6. **Egui pinning (was OQ12):** see D15.
- **Why:** Original art removes the trademark and copyright risk at no cost in quality. Functional-or-stub keeps the panel honest about what works. Host parameter IDs are a stable contract for saved DAW projects, so only additions are safe.
- **Alternatives:** Photo-derived art (rejected: rights); full menus before release (delays the visual goal); renaming per-layer IDs (breaks saved projects); all keybed variants up front (more layout work for no early benefit).
- **Impact:** Unblocks B7 art direction, B8d/B9d scope, B8g layout and B9a-c parameter work. The remaining Phase 6b unknown is OQ13 (hardware behaviour).


### D17. Chip identity: FX, PNO and ORG run on ADSP-2157x (SHARC+), SY1SY2 and SMP on ADSP-2148x

- **Status:** accepted 2026-11-12
- **Decided by:** Yuri Olive (accepted by merging PR #63; evidence in [sharc-chip-identity](research/sharc-chip-identity.md))
- **Context:** The oracle assumed ADSP-21489 for all five DSP images. The FX, PNO and ORG images address `0x31xxxxxx` registers and `0x28xxxxxx` core aliases that the 2148x map does not have.
- **Decision:** Model FX, PNO and ORG (two cores each) as **ADSP-2157x** (SC57x family, dual SHARC+; most likely ADSP-21571; the A5 core is unused) and SY1SY2 and SMP as **ADSP-2148x** (most likely ADSP-21489). The ARM host is the TI AM1705. SHARC+ differences (system-register write masks, DO encoding, L2 at `0x20000000`, SEC0, SPI2 peek port, descriptor DMA, TRU) are modelled in `core/periph/` and selected per image (`--sc57x`, `Core::sharc_plus`).
- **Why:** Register census: every `0x2xxxxxxx..0x3xxxxxxx` immediate in the three images maps into one CCES `ADSP_21571_device.h` block (VERIFIED). SY1SY2 and SMP start with the 2148x SPI-slave boot kernel (HWR table 24-10), and the Nord Piano 4 board photo shows two ADSP-21489. The family is **high** confidence, the exact 2157x part (21571 vs 21573) **medium**.
- **Alternatives:** Keep ADSP-21489 for everything (wrong peripheral map, the ORG init never completes); model SC58x (ruled out by TRU numbers and SPU/RCU placement).
- **Impact:** `crates/nord-sharc/src/core/periph/`, per-chip flag masks ([sharc-plus](research/sharc-plus.md)), two simulator targets in `sim_batch.py` (21489, 21571). Open: probes P1..P8 ([ROADMAP](ROADMAP.md) 7b.1) and the exact 2157x part (OQ in [OPEN-QUESTIONS](OPEN-QUESTIONS.md)).

### D18. Rust SHARC oracle validated against the ADI cycle-accurate simulator

- **Status:** accepted 2026-11-12
- **Decided by:** Yuri Olive (accepted by merging PRs #74, #77, #94; spec [sharc-core](specs/sharc-core.md))
- **Context:** D2 keeps the engine HLE, but engine constants and algorithms had no ground truth beyond static reading. The firmware's own DSP routines are available as VISA code in `bank0_CODE@0x440000..0x52FFFF`.
- **Decision:** Build `crates/nord-sharc`, a Rust core model that runs single routines and whole passes of the five DSP images, and treat the CCES 2.12.1 **cycle-accurate ADI simulator** as the reference for the model itself. The simulator is driven headless through CCES's own `TpsdkServer` API (`docs/research/sharc-seq/sim_run.py`, `sim_batch.py`; 32-bit Python 3.6, needs a CCES licence that this repo never touches). Every modelled behaviour has a measured fixture and a Rust test (`*_measured.rs`, `census_regress.rs`). Engine parity is then checked as oracle vs `nord-core` through `sharc-ref`, `sharc-organ`, `sharc-pass` and `sharc-fx` ([oracle-vs-engine](research/oracle-vs-engine.md), [organ-oracle-audio](research/organ-oracle-audio.md)).
- **Why:** Two links, each testable: model vs simulator (every firmware instruction form diffed, 12 gaps fixed, [sharc-census-diff](research/sharc-census-diff.md)), and engine vs oracle (bit exact or within stated ulps). It needs no SHARC hardware and no boot ROM.
- **Alternatives:** Compare only against recordings (no routine-level evidence); run the simulator directly in CI (licensed, 32-bit, slow, not redistributable); a full-system emulator (the D2 reasons still hold).
- **Impact:** Simulator output is committed as measured fixtures; the simulator itself is a local development tool. CI runs the Rust model against the fixtures only. Oracle speed is an open item ([ROADMAP](ROADMAP.md) 7b.1).

### D19. ARM firmware emulation harness (`unicorn`) as the parameter-stream source

- **Status:** accepted 2026-11-12
- **Decided by:** Yuri Olive (accepted by merging PR #80 and the follow-ups #85, #98, #105, #108)
- **Context:** The DSP images are driven by ARM messages (3-word pokes `(0x28000000 + 4 w, 0, value)` and two-word controller messages, [arm-dsp-protocol](research/arm-dsp-protocol.md), [arm-param-messages](research/arm-param-messages.md)). Reading the ARM code statically gave wrong or incomplete maps (for example the idle B3 slot of [organ-model-paths](research/organ-model-paths.md), corrected in [b3-tone-source](research/b3-tone-source.md)).
- **Decision:** Run the real host firmware (`bank0_CODE.bin`, OS v1.66, ARM926EJ-S) under `unicorn` (`tools/arm_emu.py`, `arm_boot.py`, `arm_boot_full.py`, `arm_steady.py`), boot it to the main event loop, drive edit ids through the real layer clients and capture every ARM to DSP message as `(target, address, value)`. The captured scripts replay into the oracle (`--msg-script`, `--script`). The harness stubs only what is not modelled: SPI/flash downloads, UI event queue, keybed link, DSP comm flow control.
- **Why:** The messages are produced by the firmware's own code, so key to wheel maps, weight senders and slot owners are measured, not guessed. A state reached by the real boot also shows which installs are shared (slot allocator de-duplication).
- **Alternatives:** Static xref only (misses state-dependent installs); a full ARM system emulator with peripherals (large effort, no extra evidence for the DSP side).
- **Impact:** `tools/arm_*.py`; `unicorn` is pinned in the `dev` dependency group, `capstone` is added per run (`uv run --with capstone`); nothing of it is linked into `nord-core`. The stubs bound what can be captured: no key-down yet, no sample maps ([ROADMAP](ROADMAP.md) 7b.2).

### D20. Derived values only: no firmware bytes in the repository

- **Status:** accepted 2026-11-12
- **Decided by:** Yuri Olive (rule 6 of [AGENTS.md](../AGENTS.md), applied to every research note since PR #80)
- **Context:** The oracle and the ARM harness read the Nord OS v1.66 images and the factory backup. These are the vendor's copyrighted binaries (`.reference/` and `target/fw_unpacked/` stay git-ignored).
- **Decision:** Only derived values are committed: addresses, routine pcs, table indices, field widths, message words, small integers, closed-form equations, constants that a unit test pins, and measured simulator output of our own probes. No firmware or sample bytes, no disassembly listings of vendor code, no unpacked images. Research notes list their sources as `(local, never committed)` and carry a Reproduce section. Unit tests use synthetic or measured fixtures. A table the code needs is committed as a closed form (for example `round(2^23 * 2^(-n/60))`) with the command that compares it with the local image.
- **Why:** Keeps the project in the interoperability and research scope of the PRD, keeps the repository small, and makes every claim reproducible from a legally owned update file.
- **Alternatives:** Commit trimmed binary fixtures (rights risk, size); keep all findings private (not reviewable).
- **Impact:** Checks that need local files are scripts under `tools/` run by hand, and `VERIFIED` tags name the command ([provenance-in-code](../.agents/rules/provenance-in-code.md)). Data files under `docs/research/data/` are limited to the value classes above.

### D21. Organ model to DSP path map

- **Status:** accepted 2026-11-13
- **Decided by:** Yuri Olive (accepted by merging PRs #116, #122, #126; evidence [organ-model-paths](research/organ-model-paths.md), [b3-tone-source](research/b3-tone-source.md), [b3-generator](research/b3-generator.md))
- **Context:** Several notes attributed the PNO core 1 oscillator bank `0x122620` to the B3 tonewheels, then to string resonance, and listed the B3 tone source as unlocated. The firmware slot allocator de-duplicates shared generators, so the settled-state sweep saw only idle re-installs.
- **Decision:** `Edt_Org_kModel` (string pool order `B3, Vox, Farfisa, Pipe1, Pipe2, B3 Bass`) selects these PNO core 1 routines:

| Model | Generator | Processor / mix stage | Status |
|---|---|---|---|
| B3 (0) | `0x121BBE` (slot 1) | `0x121E2B` | VERIFIED (run): nine Hammond drawbar partials, bin ratios 1 : 2.04 : 3.04 ... 16.26 |
| Vox (1) | `0x12272D` | `0x1229B3` | VERIFIED (run): chord at 12-TET ratios; weights come from the ARM sender `0xC0224468` |
| Farfisa (2) | `0x122A71` (shared) | `0x122B85` | EMPIRICAL (not replayed) |
| Pipe1 (3), Pipe2 (4) | `0x122620` (92-wheel bank) | `0x12268F` (mix stage) | VERIFIED; the two differ in one word, `DM(0x9594E)` |
| B3 Bass (5) | `0x121BBE` | `0x122296` | EMPIRICAL (not replayed) |

  All messages go to PNO core 1 only; no organ id reaches ORG, SY, SMP or FX. The Rust engine follows the map: `ToneWheelSource::ModelDefault` uses the firmware bank for Pipe1 / Pipe2; B3 uses `Legacy` by default and `B3Wheels` (generator ladder and wheel tables) as an opt-in; Vox, Farfisa and B3 Bass stay on `Legacy`.
- **Why:** Each row is reproduced by `tools/arm_slot_owners.py` (`boot`, `fresh`, `chips`) and `tools/arm_model_paths.py`, then replayed through `sharc-chain` / `sharc-pass`. The earlier readings "tonewheel bank = B3" and "string resonance" are superseded.
- **Alternatives:** One shared tonewheel model for B3 and Pipe (disproved: Pipe selects the bank, B3 the generator); treating the B3 processor as the oscillator (it is an 80-tap dot product of the generator ring plus three all-pass stages).
- **Impact:** [organ-engine](specs/organ-engine.md) s3.1.1 and s3.1.6; TD-04; the oscillator-bank role question leaves [OPEN-QUESTIONS](OPEN-QUESTIONS.md). Open: Farfisa and B3 Bass replays, B3 per-key weights, Vox weights.

### D22. Opt-in firmware paths in `nord-core`; defaults change only when verified end to end

- **Status:** accepted 2026-11-13
- **Decided by:** Yuri Olive (PRs #87 and #117 reverted two default changes that lacked end-to-end evidence; #127 and #129 follow the rule)
- **Context:** The first firmware-bank port became the B3 default and was later disproved (the bank is the Pipe generator). Golden renders (`render_hash`) and user presets depend on defaults.
- **Decision:** Firmware-derived behaviour enters `nord-core` behind a switch with the old behaviour as default: `ToneWheelSource::{ModelDefault, Firmware, Legacy, B3Wheels}`, `WeightMode::{Legacy, Firmware}`, `VibratoLink`, `DrawbarLaw`, `SawModel::{Classic, Firmware}`, `SynthLayer::gain_dezipper`, `SampleInterp`, the `fx` paths `amp_fw_coeffs`, `master_iir_enabled`, `MasterBus::set_fw_compressor`, and the `fx/fw_*` routine ports. A default changes only when (1) the algorithm is `VERIFIED` against the oracle, (2) the mapping from the preset or ARM message into it is `VERIFIED`, not `ASSUMED`, and (3) the PR re-baselines `render_hash` and lists the changed segments. Constants carry `VERIFIED` / `EMPIRICAL` / `ASSUMED` tags.
- **Why:** A routine that matches the DSP bit for bit proves nothing if the ARM-side mapping into it is a guess; the Pipe/B3 mix-up is the example. Opt-in paths keep every claim testable (golden tests) without moving audio that nobody asked to move.
- **Alternatives:** Switch defaults as soon as a routine matches (rejected after #87); keep firmware paths in research tools only (the engine would never benefit).
- **Impact:** Open defaults are tracked in TD-04, TD-10 and TD-11. The `render_hash` SHA-256 is recorded in each PR that touches it.
- **Update 2026-11-13 (amp):** `FirmwareFx::amp` became the default for Twin, JC and Small (and their variations) after the rule was met: algorithm and words VERIFIED against the oracle (drive stage golden, cabinet impulse response and chain golden), the ARM mapping VERIFIED (`kAmpTypeNew` words captured from the ARM, `kAmpDrive` = `127 drive / 10`), and the end-to-end comparison with the video improves (Vox score 3.98 to 1.30, RMS +0.2 to +0.5 dB). `FirmwareFx::OFF` / `--amp-path engine` keep the old path. `render_hash` `25baa387...` to `c7f06ca7ff6f8663be3bf2af772c6d6854f23593808ff3382d1895cf5f5ddbd5`. Open: JC and Small have no recording, piano and synth cabinets are ASSUMED equal ([fx-type-map](research/fx-type-map.md) s11).
- **Update 2026-11-13 (routing):** `FirmwareRouting::{Off, On}` (`ns4 render-patch --firmware-routing`) makes the engine follow the program's section / layer enables and per-layer KB zones. Default stays `Off`: the zone-enable rule is VERIFIED on the ARM firmware, but the meaning of the 12 `kKbZone` codes is EMPIRICAL (the firmware key-range writer is not found, OQ23), so criterion (2) is not met and `render_hash` is unchanged ([engine-routing](research/engine-routing.md)).
- **Update 2026-11-14 (routing key ranges):** criterion (2) is now met for the key range: the code to range map is read from the firmware tables and equals the ARM zone objects of 384 of 384 factory programs ([zone-key-ranges](research/zone-key-ranges.md)); the split position table of the decode was corrected (36 was missing). The default stays `Off`: the nine factory programs with a section on and no layer (OQ25) sound only through an ASSUMED rule (an enabled section without layers plays layer A) because the loader that fills the section table is not run and no hardware clip exists. `render_hash` unchanged.


**Update 2026-11-14 (synth flip).** The synth strip chain, the stage oscillators and the per-wave P table became the default of `SynthLayer::new` and `FirmwareOptions::default()` ([synth-validation](research/synth-validation.md)): over 15 hardware clips (nine Part A programs, six lesson notes) the default has mean score 4.1 against 6.6 for legacy, 9 clips better, 3 equal, 3 worse; the 9 clips never used for a fit gain 3.8 dB of score on average (7 better); no Part A clip is worse than legacy by more than 3 dB in presence or RMS. This is an EMPIRICAL exception to condition 1 (verified end to end): the oscillator P words are fitted, not read from the firmware, and the validation clips are lossy YouTube audio at an assumed velocity. The classic chain stays one switch away (`--legacy-all`, `--synth-chain legacy`, `SynthLayer::legacy`).

### D23. The firmware piano level is the default for `.npno` sounds

- **Status:** accepted 2026-11-13
- **Decided by:** Yuri Olive (PR for `feat/piano-extra-voice`; evidence [piano-sound-load](research/piano-sound-load.md) sections 9 to 11)
- **Context:** D22 keeps firmware paths opt-in until the algorithm and the mapping into it are VERIFIED. The piano velocity response was the touch curve `VelocityTouch` (ASSUMED powers) plus a shaping rule.
- **Decision:** `PianoLayer::firmware_velocity_level` defaults to `true`. For an `.npno` sound the note-on starts the firmware's voices (`NpnoLevel::voices`) with the gain `level_q23 / 2^23` each; SFZ/WAV and synthetic sounds and damper-down notes keep the legacy path. `ns4 render-patch --legacy-piano-level` restores the old response.
- **Why:** The level law, the stroke selection, the per-key bytes, the extra-voice rule and the gain word equal the real firmware (ARM run with the loaded sound) in 96 of 96 random notes on four sounds and 12 of 12 SMP record gains; the mapping from note and velocity into them is derived from the file, not guessed (D22 conditions 1 and 2).
- **Alternatives:** Stay opt-in (kept the ASSUMED touch curve although the firmware law is known); make the SFZ/WAV path follow the same law (no stroke records, no model).
- **Impact:** `render_hash` `25baa387...` unchanged (synthetic piano only); the `.npno` piano renders change. The emulator-versus-hardware piano gap does not shrink (RMS +11.8 -> +12.2 dB on Velvet Grand): the excess level is downstream of the voice and is tracked in the research note.

### D24. The SMP bus scale of the piano stays opt-in

- **Status:** accepted 2026-11-13
- **Decided by:** Yuri Olive (PR for `feat/piano-downstream-level`; evidence [piano-downstream-level](research/piano-downstream-level.md))
- **Context:** After D23 the Velvet Grand segment reads about +12 dB against the recording. The firmware path after the voice was traced: the SMP output stage `bus += (y << 17) * gain >> 31` (VERIFIED), the layer level, master, string resonance, dyn comp and unison of the engine.
- **Decision:** Add `PianoLayer::firmware_bus_scale` (`ns4 render-patch --firmware-piano-bus`, `piano/downstream.rs`): the sample path uses 0.5 (bus word read as a fraction of `2^31`) instead of the legacy 0.55. Default `false`.
- **Why:** The bus word is VERIFIED but the receiver's full-scale convention is not decoded (ASSUMED), and the video check improves by 0.83 dB only (+12.2 to +11.4 dB), not to within 3 dB (D22). The large gap is the fixed replay velocity 100 and the unknown recording gain: velocity 64 reads +4.8 dB, and against the oracle-verified Pipe path the piano is +2.4 dB.
- **Alternatives:** Make 0.5 the default now (changes every `.npno` render by 0.83 dB on an assumption); attenuate the piano layer by 12 dB to match the video (fits a replay artefact, would be wrong at the real velocities).
- **Impact:** `render_hash` unchanged. No engine stage other than the 0.55 constant differs from the firmware at default settings.
- **Update 2026-11-13 ([piano-note-velocity](research/piano-note-velocity.md)):** the receiver decode found the first reader of the bus (`FLOAT BY -29`, `2^-29` per unit) but not the end-to-end scale, so the convention stays ASSUMED and the default stays 0.55. The video gap is explained by a uniform replay velocity of about 88 (spectrum within 2.7 dB per band) plus a recording offset of +7.3 dB equal to the Pipe yardstick; per-note velocity estimates are not identifiable and are worse than the uniform replay.

### D25. The organ output level law; Pipe takes it by default, B3Wheels, Vox and routing stay opt-in

- **Status:** accepted 2026-11-13
- **Decided by:** Yuri Olive (PR for `feat/organ-output-level-law`; evidence [organ-output-law](research/organ-output-law.md))
- **Context:** Vox needed +14 dB, B3 +9.3 dB (drive fixed) and Pipe +14.4 dB (legacy bus) for the video level; the Vox gain word `0x3F1EB100` was assumed to follow the layer level.
- **Decision:** One law `K * swell_word * level^2` (`organ/output_level.rs`). Swell word `520 v^2 2^-23` (sender `0xC0223B90`, default pedal 100 = 0.6199) and layer level `(v / 127)^2` (handler `0xC011AB8C`) are VERIFIED from the firmware; `K = 2.8` is EMPIRICAL (three clips). `OutputLevel::ModelDefault` applies it to Pipe on the firmware bank; `--organ-level legacy|firmware` overrides. The program rotary drive is imported as `preset / 10` (firmware `v / 127`). B3Wheels, Vox and `FirmwareRouting` stay opt-in.
- **Why:** Pipe meets the bar (oracle lines 0.00 dB, RMS +3.3, crest -0.4, sub +1.3, mid +0.7 dB; score 4.95 to 3.25). B3Wheels fails the oracle criterion (2 start-phase lines) and presence (-19.9 dB); Vox fails mid (+7.6 dB) and the 1.5 to 8 kHz bands; routing has an EMPIRICAL zone table.
- **Alternatives:** Fit one gain per model (rejected: hides the structure); flip B3 and Vox on RMS alone (rejected: D22 criteria).
- **Impact:** `render_hash` `c7f06ca7...` unchanged (the dump has no Pipe layer or program rotary). Program renders change: rotary drive 10 times lower in the engine unit, Pipe level.

### D26. FX Mod 1: A-PAN and TREM use the firmware routines behind `FirmwareFx::mod1`

- **Status:** accepted 2026-11-14
- **Decided by:** Yuri Olive (PR for `feat/fx-delay-mod-types`; evidence [fx-mod-types](research/fx-mod-types.md))
- **Context:** The engine models A-PAN and TREM as `sin` LFOs with an assumed `0.1 .. 15 Hz` rate. The firmware routines are `0x122D93` (A-PAN, `kFx1Type` 0, 1) and `0x122C39` (types 2, 3, earlier mislabelled all-pass); their instance words are in the FX image and the boot log.
- **Decision:** Model `0x122D93` (`tools/fx_model_apan.py`, bit identical to the oracle on 1000 scenarios plus per-instruction traces), port it as `FwAutoPan`, and wire it and the already verified `FwAllpass` (mix 0 = tremolo) through `fx/fw_mod1.rs` behind `FirmwareFx::mod1`. Default stays off.
- **Why:** 70 of 89 Mod 1 programs use these two types. The routines and parameter words are VERIFIED; what is not verified end to end is the rate in hertz (4 frames per call at the DSP frame rate) and the TREM role, so D22 keeps the switch opt-in.
- **Alternatives:** Flip the default (rejected: no hardware recording of the rate); tune the engine LFO range by ear (rejected: the firmware words are available).
- **Impact:** `ns4 render-patch --firmware-fx` and `FirmwareFx::ALL` change A-PAN and TREM programs (4 frame latency, firmware depth and rate); the default `render_hash` is unchanged.

### D27. The Synth waveform browser uses the firmware's category tree; the program category id stays unmapped

- **Status:** proposed 2026-11-14 (accepted when the PR for `research/wave-categories` merges)
- **Context:** The ANALOG, WAVE and FM name tables of the panel were written from the manual: 7 ANALOG categories (no `SYNC`), invented `SUPER` items, eight invented `TABLES n` groups for WAVE and `ALGO A..E` for FM. The firmware builds the real tree in one routine (`0xC01ADBC0`); the STORE `Cat:` value could not be traced to a model class, and the factory header `category_id` values (`1..52`) do not index the 26-name table.
- **Decision:** `nord-gui` `ui_state/synth/waves.rs` holds the firmware tree (8 ANALOG categories with `SYNC` at index 2, 5 WAVE categories of 6/6/14/10/10, FM groups `HARMONIC` / `INHARMONIC A..E`), each entry with its firmware wave id, checked row by row against `docs/research/data/synth-wave-categories.csv`. The OLED composes `Multi ` / `Super ` before the entry name (manual figure). The WAVE browser indexes the engine bank by flat position. The 26 program categories stay as in `bay_program/name.rs` with the link to the STORE value ASSUMED; no header id is written.
- **Why:** Category membership, names and ids are VERIFIED twice (ARM tree and DSP P-table, 45 of 45); the panel category index now equals the `osc_category` field of a loaded program. Writing a header id would need the unknown id map (OQ26).
- **Alternatives:** Keep manual names (wrong category set and an invented `SUPER`); name the entries `<Category> <name>` for every category (the manual figure shows it only for MULTI, the rule for the others is unverified); map the header id by program names (guesswork, kept out of code).
- **Impact:** CAT dial of ANALOG has 8 stops (MULTI is the sixth, SUPER the seventh); the WAVE page shows firmware names instead of `Table n`; the flat wavetable index of a panel pick now equals the one of a loaded program (`cat * 8 + i` before). `render_hash` unchanged.

### D28. The firmware amp stays the default; the Vox presence residual is not assigned to the amp

- **Status:** accepted 2026-11-14
- **Decided by:** Yuri Olive (PR for `feat/fx-amp-twin-port`; evidence [fx-type-map](research/fx-type-map.md) s12)
- **Context:** [organ-presence-gap](research/organ-presence-gap.md) left the Vox presence / brilliance 7.5 / 9.7 dB under the clip and pointed at a missing amp / cabinet port.
- **Decision:** No amp or default change. The drive stage and cabinet (D22) already cover Twin, Twin II, JC, JC II, Small, Small II with oracle words; the residual is attributed to the organ bank and Mod 2, not the amp.
- **Why:** With the amp on, presence / brilliance are -7.5 / -9.7 dB; amp input level up to +24 dB, every other amp type and all `FirmwareFx` ports leave 5.5 / 6.4 dB at best; B3 is bit identical for both amp paths (amp off in its program).
- **Alternatives:** Fit a presence shelf after the cabinet (rejected: the cabinet words are oracle exact, and the fit would hide the organ bank gap); flip `vox` on the clip score (rejected: D22 and D25 bars).
- **Impact:** None on renders or `render_hash`; the cabinet taps gain a fingerprint and response test.

### D29. FX Mod 2: CHOR uses the firmware routines behind `FirmwareFx::mod2`

- **Status:** accepted 2026-11-14
- **Decided by:** Yuri Olive (PR for `feat/fx-mod2-types`; evidence [fx-mod2-types](research/fx-mod2-types.md))
- **Context:** The engine renders Mod 2 `CHOR` as an assumed `9 ms` modulated delay. The firmware routines are `0x122E24` (`kFx2Type` 4) and `0x122F77` (5), a circular stereo delay line with an LFO driven fractional tap; CHOR is used by 93 of 156 Mod 2 programs.
- **Decision:** Model both routines instruction by instruction (`tools/fx_model_chorus.py`, 500/500 scenarios per type and the per-instruction trace against the oracle), port them with a pair-register machine (`fx/fw_pe.rs`, `fw_chorus4.rs`, `fw_chorus5.rs`) and wire them through `fx/fw_mod2.rs` behind `FirmwareFx::mod2` (in `ALL`, not `DEFAULT`).
- **Why:** The routines and the data flow are VERIFIED; the amount laws are VERIFIED at 11 samples and interpolated between them, the delay is in DSP frames, the rate in hertz has no hardware recording and the layer specific words use the piano row, so D22 keeps the switch opt-in.
- **Alternatives:** Tune the engine chorus by ear (rejected: the firmware words are available); wait for all six Mod 2 types (rejected: CHOR alone covers 60 % of Mod 2 programs).
- **Impact:** `ns4 render-patch --firmware-fx` and `FirmwareFx::ALL` change CHOR programs (4 frame latency, firmware delay times and rates); the default `render_hash` is unchanged.

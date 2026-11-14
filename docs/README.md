# Documentation Index

Canonical documentation for the **Nord Stage 4 Emulator** project (interoperability, research, preservation, and compatibility).

| Document | Purpose |
|---|---|
| [PRD.md](PRD.md) | Product goals, target users, scope, latency/polyphony budgets, non-goals |
| [ROADMAP.md](ROADMAP.md) | Phased delivery plan with tracked checkboxes (`- [ ]` / `- [x]`) and parallel lanes |
| [ARCHITECTURE.md](ARCHITECTURE.md) | System architecture, 3-engine / 7-layer DSP pipeline, crate layout, SHARC oracle and ARM harness data flow |
| [DECISIONS.md](DECISIONS.md) | Architectural Decision Records (`D1`–`D22`) |
| [OPEN-QUESTIONS.md](OPEN-QUESTIONS.md) | Open technical and design questions (`OQ1`–`OQ22`, answered ones removed) |
| [TECH_DEBT.md](TECH_DEBT.md) | Technical debt register (`TD-01`..`TD-11`) |
| [HOST_COMPATIBILITY.md](HOST_COMPATIBILITY.md) | DAW host compatibility status |
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
| [specs/control-inventory.md](specs/control-inventory.md) | Phase 6b control inventory (`ControlId`, binding, gap list) |
| [specs/daw-plugin.md](specs/daw-plugin.md) | Host parameter IDs, engine sync, MIDI routing, program import |
| [specs/sharc-core.md](specs/sharc-core.md) | Track B: SHARC core model as a test oracle (scope, milestones, risks) |
| [specs/verification.md](specs/verification.md) | CI pipeline, unit-test coverage ratchet (`tools/check_tests.py`), deterministic offline audio verification, round-trip CBIN tests |

## Research & Reverse Engineering (`docs/research/`)

| Research Note | Focus |
|---|---|
| [research/ns4-backup-and-cbin-teardown.md](research/ns4-backup-and-cbin-teardown.md) | Empirical teardown of `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (`product_id=46`, build `2934`, CBIN header layout, embedded bitstreams) |
| [research/ns4-firmware-teardown.md](research/ns4-firmware-teardown.md) | Empirical teardown of `Update Nord Stage 4 OS v1.66.exe` (`RCDATA 101`, `ARM0` host MCU vs `DSPO`, `DSPS`, `DSPF` SHARC DSP images) |
| [research/1to1-emulation-feasibility.md](research/1to1-emulation-feasibility.md) | Low-level (1:1) emulation blockers: firmware encryption vs compression, boot ROMs, chip IDs, sample codec |
| [research/backup-and-firmware-readability.md](research/backup-and-firmware-readability.md) | .ns4b contents, sample-library format, firmware entropy and readability checks |
| [research/mainboard-photo-analysis.md](research/mainboard-photo-analysis.md) | Stage 4 mainboard/panel photo identification (AM1705 host, ADI DSPs, board revisions) |
| [research/hardware-and-dac-analysis.md](research/hardware-and-dac-analysis.md) | Mainboard topology (AM1705 host, ADI DSPs), memory, CS4392 DACs, and LM833/LF353 analog reconstruction filter analysis |

**Firmware, formats and hardware**

| Research Note | Focus |
|---|---|
| [research/clavia-lz-reconstruction.md](research/clavia-lz-reconstruction.md) | Clavia LZO1X stream format and bank-relative extraction |
| [research/nsmp4-and-npno-cnsp-codec.md](research/nsmp4-and-npno-cnsp-codec.md) | `.nsmp4` / `.npno` `CNSP` fixed-LPC codecs and `.ns4b` archive layout |
| [research/nord-piano-npno-and-codec.md](research/nord-piano-npno-and-codec.md) | `.npno` format and the 4-bit DPCM codec |
| [research/hardware-architecture.md](research/hardware-architecture.md) | Mainboard architecture and component identification |
| [research/sharc-dsp-and-preset-hash-teardown.md](research/sharc-dsp-and-preset-hash-teardown.md) | `ARM0` preset-name hash and SHARC parameter-curve teardown |
| [research/daw-plugin-architecture.md](research/daw-plugin-architecture.md) | DAW plugin architecture evaluation (`nih-plug`) |
| [research/panel-blender-proof.md](research/panel-blender-proof.md) | Blender proof: knob filmstrip, fader cap, atlas size |

**SHARC oracle: images, chips and core model**

| Research Note | Focus |
|---|---|
| [research/dsp-image-hunt.md](research/dsp-image-hunt.md) | Finding and cutting the five DSP images, instruction census, loader streams |
| [research/sharc-chip-identity.md](research/sharc-chip-identity.md) | FX / PNO / ORG on ADSP-2157x, SY1SY2 / SMP on ADSP-2148x; probes P1..P8 |
| [research/sharc-plus.md](research/sharc-plus.md) | SHARC+ differences measured on the ADSP-21571 simulator |
| [research/sharc-census-diff.md](research/sharc-census-diff.md) | Every firmware instruction, model against simulator |
| [research/sharc-routine-diff.md](research/sharc-routine-diff.md) | Routine-level diff of PNO `0x122620` and `0x12268F` on the simulator |
| [research/sharc-float.md](research/sharc-float.md) | Float ALU and multiplier fidelity |
| [research/sharc-fixed.md](research/sharc-fixed.md) | Fixed-point ALU, multiplier/MR, shifter flags, CACC |
| [research/sharc-simd.md](research/sharc-simd.md) | SIMD (PEYEN) compute |
| [research/sharc-type2c.md](research/sharc-type2c.md) | Type 2c short multiply and the exhaustive compute sweep |
| [research/sharc-bitfifo.md](research/sharc-bitfifo.md) | Shifter bit FIFO (`BITDEP`, `BITEXT`, `BFFWRP`) |
| [research/sharc-loops.md](research/sharc-loops.md) | Arithmetic loops and the probe harness |
| [research/sharc-stacks.md](research/sharc-stacks.md) | PC, loop and status stacks |
| [research/sharc-sequencer.md](research/sharc-sequencer.md) | Sequencer, interrupts, timer, system registers |
| [research/sharc-leftovers.md](research/sharc-leftovers.md) | `DO UNTIL`, stack instructions, jump modifiers, Type 25 |
| [research/sharc-sy1sy2-smp.md](research/sharc-sy1sy2-smp.md) | SY1SY2 and SMP images: boot kernel and start-up |
| [research/sharc-seq/README.md](research/sharc-seq/README.md) | Headless `TpsdkServer` runner (`sim_run.py`, `sim_batch.py`) and probe generators |

**ARM host firmware and the ARM to DSP link**

| Research Note | Focus |
|---|---|
| [research/arm-dsp-protocol.md](research/arm-dsp-protocol.md) | Download path, message format, EMIFA port, negative DSP-address result |
| [research/arm-param-messages.md](research/arm-param-messages.md) | Parameter senders, channel tables, targets |
| [research/arm-fw-emulation.md](research/arm-fw-emulation.md) | `unicorn` harness capturing real ARM to DSP messages |
| [research/arm-organ-messages.md](research/arm-organ-messages.md) | Edit registry, organ layer, partial boot and the steady state (s10) |
| [research/dsp-rx-protocol.md](research/dsp-rx-protocol.md) | How the ORG image receives data (peek/poke port, strips) |
| [research/preset-dsp-map.md](research/preset-dsp-map.md) | Preset fields against DSP strips, packed words and conversion tables |

**Organ**

| Research Note | Focus |
|---|---|
| [research/program-coverage.md](research/program-coverage.md) | Factory program sweep: engine paths used by the 384 programs, verified vs assumed, render health |
| [research/organ-model-paths.md](research/organ-model-paths.md) | Which PNO core 1 routines each `Edt_Org_kModel` installs |
| [research/b3-tone-source.md](research/b3-tone-source.md) | B3 tone source: generator `0x121BBE` + processor `0x121E2B` |
| [research/b3-generator.md](research/b3-generator.md) | B3 generator frequency law, wheel tables, silent tail |
| [research/organ-farfisa-bass.md](research/organ-farfisa-bass.md) | Farfisa generator decoded and ported, Farfisa / B3 Bass passes in the oracle, event-2 chain owners |
| [research/organ-weights.md](research/organ-weights.md) | Per-wheel weights from the ARM bank engines |
| [research/organ-oracle-audio.md](research/organ-oracle-audio.md) | Bank + mix stage audio from the oracle against the Rust engine |
| [research/organ-tonewheel-bank.md](research/organ-tonewheel-bank.md) | PNO core 1 bank `0x122620` (Pipe generator), oracle run and ARM tables |
| [research/organ-post-stage.md](research/organ-post-stage.md) | Vox post stage `0x12272D` and output stage `0x1229B3` |
| [research/pno-full-pass.md](research/pno-full-pass.md) | Complete PNO core 1 pass in the oracle (`sharc-pass`) |
| [research/pno-module-chain.md](research/pno-module-chain.md) | ARM installers and module map of the PNO chain |
| [research/pno-dsp-map.md](research/pno-dsp-map.md) | PNO image map (core 1, entry `0x121007`) |
| [research/org-dsp-map.md](research/org-dsp-map.md) | ORG processing pass, parameter map, `sharc-org` |
| [research/org-stage-map.md](research/org-stage-map.md) | ORG stage table, strip fields, module palette |
| [research/organ-tone-source.md](research/organ-tone-source.md) | Superseded sweep of candidate tone sources (see organ-model-paths) |
| [research/organ-tonegen.md](research/organ-tonegen.md) | Superseded tonewheel bank hunt (see organ-model-paths) |

**Synth**

| Research Note | Focus |
|---|---|
| [research/sy-dsp-map.md](research/sy-dsp-map.md) | SY1SY2 image map and the sinc kernel set |
| [research/sy-smp-audio.md](research/sy-smp-audio.md) | SY1SY2 and SMP voices and audio through the oracle |
| [research/oracle-vs-engine.md](research/oracle-vs-engine.md) | SY1SY2 resampler: oracle against the Rust engine |
| [research/synth-voice-control.md](research/synth-voice-control.md) | Where oscillators, filters and envelopes run; saw and gain smoother |
| [research/synth-arp-patterns.md](research/synth-arp-patterns.md) | Synth arp / gate pattern storage in `.ns4y`, the 29 firmware presets (table recovered, [arp-preset-table.csv](research/arp-preset-table.csv)) |
| [research/synth-stage-equations.md](research/synth-stage-equations.md) | Core2 filter stage `0x12566E` |
| [research/synth-0x2f-ns4decode-crosscheck.md](research/synth-0x2f-ns4decode-crosscheck.md) | `0x32` / `0x2F` bit map cross-check |
| [research/synth-0x2f-filter-env-adr.md](research/synth-0x2f-filter-env-adr.md) | Filter envelope ADR location |
| [research/synth-hardware-sweep-plan.md](research/synth-hardware-sweep-plan.md) | Hardware sweep plan for the ASSUMED scales |
| [research/synth-scale-source-hunt.md](research/synth-scale-source-hunt.md) | Videos, manuals and forums as scale sources |

**Piano**

| Research Note | Focus |
|---|---|
| [research/smp-voice.md](research/smp-voice.md) | SMP compressed voice record and real `.npno` pages on the oracle |
| [research/smp-kernel-fit.md](research/smp-kernel-fit.md) | SMP interpolation kernel closed form and unpacker |
| [research/piano-dsp-path.md](research/piano-dsp-path.md) | ARM key handler to SMP voice record; pitch and level laws |

**Effects**

| Research Note | Focus |
|---|---|
| [research/fx-dsp-map.md](research/fx-dsp-map.md) | FX image map and a verified biquad |
| [research/fx-chain-install.md](research/fx-chain-install.md) | ARM routine directory, stage rows, Amp type switch |
| [research/fx-type-map.md](research/fx-type-map.md) | Module objects, routine ownership, what a type switch sends |
| [research/fx-contexts.md](research/fx-contexts.md) | FX clients and context words, wiring plan |
| [research/fx-routine-models.md](research/fx-routine-models.md) | Delay, table LFO, all-pass, resonant filter models |
| [research/hardware-audio-reference-fx.md](research/hardware-audio-reference-fx.md) | Hardware audio reference (Bank A demo video) for FX validation |

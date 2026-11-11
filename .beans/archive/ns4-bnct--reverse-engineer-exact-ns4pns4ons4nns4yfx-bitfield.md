---
# ns4-bnct
title: Reverse-engineer exact .ns4p/.ns4o/.ns4n/.ns4y/FX bitfields from OS v1.66 and factory .ns4b
status: completed
type: task
priority: normal
created_at: 2026-11-11T17:42:35Z
updated_at: 2026-11-11T18:29:11Z
parent: ns4-707f
---

## Context
Inspecting real Nord Stage 4 OS v1.66 factory .ns4p files with crates/nord-core/src/cbin/presets.rs revealed that the existing bitfield offsets and section slices (Global 45B, Organ 135B, Piano 147B, Synth 493B) are misaligned against real hardware binaries, causing nearly every factory program to decode to identical bogus defaults (Pipe2, tempo 64.87, scene_1_mask=0, etc.) so importing a program leaves sound wrong/unchanged.

## Higher Goal
Extract the exact hardware parameter bit layout for Global (.ns4p), Organ (.ns4o), Piano (.ns4n), Synth (.ns4y), and per-layer FX (0x36) blocks from OS v1.66 firmware/factory patches so importing any .ns4p produces faithful DSP synthesis and panel state.

## Acceptance Criteria
- [x] Map exact section boundaries and header/signature bytes of .ns4p, .ns4o, .ns4n, .ns4y, and 0x36 FX blocks across the 384 factory programs in .ns4b and bank0_CODE.bin
- [x] Map exact bit offsets for Global (scenes I/II active masks, split enabled/points/crossfades, transpose, master clock, compressor)
- [x] Map exact bit offsets for Organ (model, drawbars A/B, percussion, vibrato/chorus, rotary, layer enable/level/octave)
- [x] Map exact bit offsets for Piano (layer enable/level/octave, category/model_id, timbre, touch, string res, pedal noise, soft release)
- [x] Map exact bit offsets for Synth (3 layers: enable, level, octave, osc mode/wave/config, filter model/cutoff/res/drive/env/kb track, amp ADR, LFO, arp, unison)
- [x] Map per-layer FX (0x36) block (Mod1, Mod2, Delay, Amp/Comp/Drive, EQ, Reverb + enable/dry-wet/parameters) and wire into FX engine & Stage4Engine::apply_decoded_program
- [x] Update crates/nord-core/src/cbin/presets.rs, engine.rs, crates/nord-gui/src/panel.rs, tools/ns4_inspect.py, docs/specs/cbin-and-presets.md, and docs/ROADMAP.md (container.rs 48-byte CBIN framing unchanged)
- [x] Verify decoded factory .ns4p round-tripping and distinctive audio/DSP state across Synth, Organ, Piano, and Split factory programs, passing all 7 quality gates

## Out of Scope
- .npno/.nsmp4 compressed audio sample decoding (tracked in separate Track B bean)

## Summary of Changes
- Pinned OS v1.66 `bank0_CODE.bin` `Buffer_*` bit offsets, UI enum tables (`Mod1Type`, `Mod2Type`, `RvbType`, `VibratoTypeB3`, `SplitPos`), and `4W+3` MSB-first Morph encoding (`zero = max_val`: `8` for drawbars `W=4`, `127` for `W=7`, `16383` for `W=14`).
- Rewrote `crates/nord-core/src/cbin/presets.rs` with lossless `raw_payload`-backed decoders/encoders (`FxDecoded`, `OrganPresetDecoded`, `PianoPresetDecoded`, `SynthPresetDecoded`, `GlobalProgramDecoded`, `ProgramDecoded`) and synthetic round-trip unit tests.
- Wired decoded synthesis, section/scene masks, split points, Morph deltas (`WHEEL`/`A.T.`/`CTRLPED`), and all 7 `0x36` FX chains into `Stage4Engine::apply_program` (`crates/nord-core/src/engine.rs`) and `Stage4PanelState::apply_program` (`crates/nord-gui/src/panel.rs`), removing prior fallback hacks.
- Updated `tools/ns4_inspect.py` and `tools/tests/test_ns4_inspect.py` with matching MSB-first decoders and synthetic hardware tests, plus updated `docs/specs/cbin-and-presets.md` and `docs/ROADMAP.md`.

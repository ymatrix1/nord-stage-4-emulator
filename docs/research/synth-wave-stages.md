# Synth Wave to strip stage: the type index of each ANALOG wave

- **Checked:** 2026-11-13
- **Sources (local, never committed):** OS v1.66 `bank0_CODE.bin` / `bank0_SRAM.bin` in the ARM harness (`tools/arm_edit_capture.py`, settled snapshot), ORG core 2 image (`tools/ns4_dsp_ldr.py --sram`), `sharc-org`, `sharc-disasm`; `tools/synth_wave_probe.py`. Builds on [synth-strip-path](synth-strip-path.md) s6, [pno-module-chain](pno-module-chain.md) s5, [synth-voice-control](synth-voice-control.md).
- **Question:** Which strip word 0 stage (oscillator routine) does each ANALOG category / wave of the Synth install, and which routine is the SHAPE wave 0 of the Smooth Lead program?

Tags follow `.agents/rules/provenance-in-code.md`. Only addresses, indices and derived values are recorded.

## 1. Short answer

| Question | Answer | Tag |
|---|---|---|
| Who writes the stage type? | The synth layer apply function (`0xC00171B0`, value `(word + 0x8000) >> 16` stored at `[strip + 0x98]`) writes the **pending type index**; `0xC01CE7D8` copies the 40-byte block `[strip + 0x98 .. 0xC0)` to `[strip + 0xC8 ..)`; the sender `0xC01CC85C` builds strip word 0 = `((byte & 0x7F) << 24) \| table[[strip + 0xC8]]` (table `0xC02FA050`, 74 ids). Earlier note (s6 of synth-strip-path: "the edit ids do not change the type") was a read of the **applied** copy: `0xC01CE7D8` is reached from `0xC0019410` only for the state marker 0x39, so in the harness word 0 stays type 2 while `[strip + 0x98]` follows the edits | VERIFIED (write watch, code reads) |
| Mapping category / wave to type | bank 0 (ANALOG), `kMode = 1`: table of section 2 (`data/synth-wave-types.csv`, 112 harness points); an out-of-range wave falls back to type 2; `kMode = 0` gives the marker 0x39; banks 1 and 2 give types 49 .. 53 (one per category 0..4) | VERIFIED (run) |
| Smooth Lead (ANALOG cat 3, wave 0) | type **7** = stage `0x1212D3`, **not** the stage `0x1215FB` of cat 0 wave 2 and not a triangle | VERIFIED (run), preset category / wave fields equal the edit-id domain: VERIFIED by range ([synth-wave-models](synth-wave-models.md) s7) |
| Stage models | not written: `0x1212D3` is 200 instructions (`0x1212D3..0x12145D`, listing `0x2425A6..0x2428BA`), four calls of the helper `0x1251D4` (not `0x12501F`), reciprocal normalisation (`RECIPS`), a shape-dependent pulse width; its parameter block and the A-slot call frame are not set up | open |

## 2. Table (ANALOG, kOscBank 0, kMode 1)

| Category | Waves | Type index per wave | Stage id per wave |
|---|---|---|---|
| 0 | 7 | 0 1 2 3 4 5 46 | 120CD0 121183 1215FB 121B99 121B99 121B99 121FD8 |
| 1 | 3 | 10 11 12 | 12179A 121867 1219C3 |
| 2 | 9 | 27 28 29 30 31 32 33 34 35 | 12255D 1226DD 122A32 122C5C 122C5C 122C5C 1239BE 123C7C 123FE9 |
| 3 | 5 | 7 8 9 43 44 | 1212D3 12169D 121AB5 120F55 12145E |
| 4 | 7 | 38 39 13 41 42 36 37 | 123427 1235C9 1232A5 12377D 120DAB 122F1F 1230DF |
| 5 | 6 | 16 17 19 20 18 21 | 121C54 (six waves, same stage) |
| 6 | 5 | 22 23 24 25 26 | 121D6D 121E9D 124985 124985 124985 |
| 7 | 3 | 47 48 45 | 122030 122151 122417 |

45 waves reach 34 distinct stage routines. Waves that share a stage (cat 0 waves 3 to 5, cat 2 waves 3 to 5, cat 5, cat 6 waves 2 to 4) differ in the strip parameter words, not in the code. The panel names of the categories are not read from the firmware here; the decoder's names (`cbin/synth_osc.rs`) and the category numbering of the edit ids are assumed to agree.

## 3. Oracle experiment (stage `0x1212D3` in `sharc-org`)

Installing `0x1212D3` in slot 0 of the stage-A table (`--msg-script`: `0 0xC10B8 0x1212D3`) runs for 3 passes without a fault (4616 steps per pass, 441 words changed); the A-stage record `0xC1F96` then holds a slowly decaying level near 0.123 (increment word 0.006 cycles per sample from the idle frame), so the stage is live but the pitch, shape and amplitude words of its frame (`DM(-29, I2)`, parameter block `[DM(-32, I2)]`: `-32`, `-30`, `+2`, `+4`, `+16`) are not yet driven. Bit-exact modelling needs the frame layout of the A call (`I0 = 0xC1E02`, `I2 = 0xC1F96 + 0x36 c`, `I3 = 0xC0B88 + 0x26 c`, [org-stage-map](org-stage-map.md) s2) and the controller words that the pass writes each frame.

## Implications

- The ASSUMED "decoded Triangle of SHAPE wave 0 plays as the firmware saw" is wrong in kind: the hardware runs a dedicated 200-instruction stage (`0x1212D3`) for it. The 3.6 dB second harmonic of the clip is a property of that stage.
- The preset load already gives the full wave to stage table, so the remaining work per stage is the model, not the identification.

## Follow-ups

1. Drive stage `0x1212D3` in `sharc-org` with a frame (pitch increment, shape word) and fit / derive the waveform; write the Python model and golden, then the Rust opt-in module (Smooth Lead first).
2. Repeat for `0x120CD0 / 0x121183 / 0x1215FB / 0x121B99` (SAW, SQUARE, TRIANGLE of category 0) and the sub / sync stages.
3. Done in [synth-wave-models](synth-wave-models.md) s7: the category / wave domain of the preset fields equals the edit ids (sizes 7, 3, 9, 5, 7, 6, 5, 3 and 578 factory layers in range).

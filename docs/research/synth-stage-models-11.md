# Synth stages, eleventh batch: SHAPE 3 `0x120F55`, SHAPE 4 `0x12145E`; SUPER 2..4 `0x124985` blocked

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (cut from `bank0_CODE.bin` with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--msg-script`, `--buf`), `sharc-disasm` (listings `0x2428BC..0x242BF4`, `0x241EAA..0x242304`, helper `0x24A45C..0x24A4E8`, `0x24930A..0x249872`), the boot replay script `org_core2.script` (`tools/arm_bulk_tables.py`, [arm-bulk-tables](arm-bulk-tables.md)). Builds on [synth-stage-models-9](synth-stage-models-9.md) (method) and `tools/synth_wave_oracle.py` (stage-A harness, frame `I2 = 0xC1F96`).
- **Question:** Can the last ANALOG stages (SUPER 2..4 = 8 layers, SHAPE 3 and SHAPE 4 = 1 layer each) be modelled word for word?

Tags follow `.agents/rules/provenance-in-code.md`. Result: **SHAPE 3 and SHAPE 4 are bit exact (320 of 320 output words each over 40 trials with varying `s5` and pitch; `tools/synth_wave_oracle.py --stage 0x120F55 / 0x12145E`)** and ported opt-in. **SUPER 2..4 is not modelled**: it is a wavetable stage whose data the oracle cannot reproduce (s3).

## 1. SHAPE 4 (`0x12145E`, 197 instructions, `tools/synth_stage_shape4.py`)

A sibling of the SHAPE 0 stage `0x1212D3`: same setup, same helper `0x1251D4` (`m.helper`), same mix (`m.mix_lanes`). Differences (VERIFIED, run):

| Item | SHAPE 0 `0x1212D3` | SHAPE 4 `0x12145E` |
|---|---|---|
| shape source | `clip(s17 + s19, 1)` and the fade | the spread word `s = DM(-5, I2)` directly |
| normaliser | `1 / (1 - W^2)` | `2 / D`, `D = 1 + (1 - s)^2 - s^2 / 2` (two Newton steps, numerator 2) |
| second helper call | phase `- 2 (W - 1)`, subtracted, `R6` carried (`16/9`) | phase `- 2 FIX(s, 30)` (cycles `s / 2`), **added**, `R6` reloaded from `DM(-6, I4)` (both calls `4/3`) |
| bias | none | `c0 = (2/3)(a^2 - 1)` (`0x3F2AAAAA`), added to every sample (stored word) |

Per oscillator `osc_k = helper(p + k inc) + helper(p + k inc - off) + c0`; both oscillators of a lane use the setup of `inc_1`; scale `P[16] g inv_d`. Rust: `fw_shape34::shape4_core`.

## 2. SHAPE 3 (`0x120F55`, 264 instructions + helper `0x12522E`, `tools/synth_stage_shape3.py`)

Two oscillators per lane in a `LCNTR = 2` loop that swaps `DM(-25 / -23)` and `DM(-21 / -19, I2)` between the passes; the weights are consumed in the same order as `m.mix_lanes`, so the mix is that function. The scale is `-2 g P[16]` (no `1 / D`), `g` from the pitch word `f` (not `a`). Setup (all VERIFIED, run):

| Word | Law |
|---|---|
| `L` | `14 s^2 + 2`, stored (`s = DM(-5, I2)`) |
| `w` | `min(max(0.1815 / a, 2), L)`, `0.1815 / a` by two Newton steps (`0x3E3A2E7F`), `a = inc_1 2^-31` (register); stored word `DM(20, I4)` |
| `iw`, `c` | `1 / w` (register `R4 = w` feeds the second `RECIPS`), `c = (2/pi) iw` stored (`DM(-12, I4)`) |
| `d` | `-(pi / 4) w`: `R0 = DM(20, I4)` is already the *stored w* when `F0 = F0 * F1` runs, so the product uses `w`, not `iw` (the model follows the listing order; 320 of 320 confirm it) |
| `K6` | `d 0.66666 f` (`0x3F2AAA3B`) |

Per sample `k = 1..4`, `p = phase + k inc`: `x = clip(w p 2^-31, 1)`, the helper `h(x) = u (c12 + q (c14 + q (c16 + c18 q)))`, `u = |x| - 1`, `q = u^2` (image words `DM(0xC1E0E..0xC1E14)` = `1.5708, -0.64589, 0.079431, -0.0043311`, `DM(0xC1E06) = -1.0`: `h = sin(pi u / 2) = -cos(pi x / 2)`, even in `x`). `A = h + c` is stored; then two cubic band-limiting terms at `p +- FIX(iw, 31)` (cycles `+- 1 / (2 w)`): `bump = K6 (1 - min(1, |ps 2^-31| 0.5 / f))^3`, added in sequence (`bump(+) + A`, stored, then `bump(-)` + that). The result is a cosine bump of width `1 / w` cycles around phase 0 on the constant `2 / (pi w)`: a band-limited pulse-like wave. Rust: `fw_shape34::shape3_core`.

## 3. SUPER 2..4 (`0x124985`): what was found, why it stops here

`0x124985` serves the three SUPER waves `Organ`, `Saw Bright`, `Square Bright` (types 24, 25, 26; the node builder passes the table id as `aux`, [synth-wave-categories](synth-wave-categories.md)). Listing `0x24930A..0x2496D2` (stage) and `0x2496DA..0x249872` (the install / reset routine that fills the module record). Structure (reading of the listing, not checked against the oracle):

1. Pitch `f` clipped to `0.5`, the saw-helper fade `g`, `s = DM(-5, I2) + 0.06`, `v = 0.75 (s + 2 s^4)`, four voice increments `2 f (1 + v (P[-32 + 2 j] - 1))` stored at `0xC1F5E + 6 j` (lane y one above): a detuned four-voice unison like SUPER 0 / 1 (so the stage is a stage-A wave stage, not a B filter).
2. The octave band: `idx = max(0, -(8 EXP + top3 mantissa bits))` of `2 f` selects a row of the PM table `PM(DM(-16, I1) + 4 idx)` (module record `I1 = 0xC15B6`, `DM(0xC15A6) = 0x9631F`): two words, an offset and a count.
3. Per voice and sample the phase times the count (`MRF = R10 * R13`) indexes a wavetable in external memory (`DM(-17, I1) = DM(0xC15A5) = 0x08030000`), two neighbouring words are copied into the buffers `0xB6F80 / 0xB6F88`, a 4-tap polyphase FIR (`MRF` / `MRB` loops, PM coefficients) interpolates, then a three-section lattice (`I9 = 0x95000`) filters and the P weights mix the lanes.

**Blocker (VERIFIED, run):** with the image and a random frame the stage outputs 0 because (a) the PM table at `0x9631F..` (3072 words) is filled by the ARM boot log (the replay script holds it, e.g. `0x9631F = 0x7A4`, `0x96320 = 4`), not by the image, and (b) the wavetable at `0x08030000..` is **not in the boot log** (0 of 13,509 script lines touch `0x803xxxx`): it is SDRAM data the ARM side writes some other way, and the update image holds no such table. The oracle can poke random table words, so the *algorithm* could still be verified word for word, but the Rust port would play silence or an invented table; the sound of the three waves needs the real table (where the ARM gets it from is open).

## 4. Oracle, tests, Rust

`tools/synth_wave_oracle.py` gained `STAGE_SHAPE3 / STAGE_SHAPE4` (models, `S5_STAGES`, `PITCH_STAGES`: `s5` and the note term are varied per trial). 40 trials each: **320 of 320** words equal. Recorded cases: `data/synth-stage-oracle-cases-rest.json` (six cases per stage, replayed by `tools/tests/test_synth_stage_rest.py`). Rust (`synth/fw_shape34.rs`, `FwStage::Shape3 / Shape4`, SHAPE category waves 3 and 4 in `from_category_wave`, behind `--synth-stages`): goldens `tools/synth_stage_golden9.py` -> `tests_fw_stage_golden9.rs` (160 samples each), deviation below `2e-5` absolute from the Python words. The Rust cores are closed forms in `f32` of one oscillator (own weight 1); the band limit of SHAPE 4 follows the render increment, that of SHAPE 3 the SY rate (as in `fw_sine3.rs`): ASSUMED for the render rate. The spread word follows Osc Ctrl, the P block (ratios, weights) is ASSUMED, as for every other stage (OQ22).

## 5. Coverage

ANALOG layers with a bit-exact stage model: 302 + 2 = **304 of 312 (97.4 %)**. Open: SUPER 2..4, 8 layers (2 + 5 + 1 by wave). Filter layers unchanged (442 of 442).

## Implications

- The last closed-form oscillators are done; the remaining ANALOG layers need ARM-provided table data, not more DSP reading.
- SHAPE 3 and 4 change sound only with `--synth-stages` (D53).

## Follow-ups

1. Find how the ARM fills the SUPER wavetable at `0x08030000` (the id `aux` of `0xC01ACBF4`; search the ARM code for stores into the SDRAM window `0x0803xxxx`, or a DMA descriptor), then model `0x124985` with the replay script's PM table; the structure in s3 is the starting point.
2. A hardware clip of SHAPE waves 3 and 4 to fit the P block (ratios, weights) and the spread word scale.

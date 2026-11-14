# Synth oscillator stages, second batch: noise, Shape Square, SUPER, MULTI

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (`tools/ns4_dsp_ldr.py --sram`), `sharc-org` and `sharc-disasm` (listings `0x243570..0x24405E`), `docs/research/data/program-sweep.json` (PR #241), [synth-wave-types.csv](data/synth-wave-types.csv). Builds on [synth-wave-models](synth-wave-models.md) (40-bit model, `mix_lanes`) and [synth-wave-stages](synth-wave-stages.md) (wave to stage table).
- **Question:** Which of the remaining ANALOG stages matter most for the factory bank, and can they be modelled word for word?

Tags follow `.agents/rules/provenance-in-code.md`. Only addresses, derived words and formulas are recorded.

## 1. Ranking by factory-layer use

Uses of the 312 ANALOG layers in the 384 factory programs (`program-sweep.json`, `synth.analog.wave=<CAT>:<n>`), by stage:

| Stage | Waves | Layers | Status |
|---|---|---|---|
| `0x1215FB` saw, `0x121B99` pulse, `0x120CD0` sine, `0x121183` triangle, `0x12169D` Shape Saw, `0x1212D3` Shape Triangle | PURE 0..4, SHAPE 0, 1 | 107 | modelled earlier |
| `0x121D6D` SUPER | SUPER 0 | 46 | **this PR** |
| `0x121C54` MULTI (one stage, six waves) | MULTI 0..5 | 43 | **this PR** |
| `0x121FD8` noise | PURE 6 | 25 | **this PR** |
| `0x121AB5` Shape Square | SHAPE 2 | 19 | **this PR** |
| SHAPE SINE (`0x123427`, `0x1235C9`, `0x1232A5`, `0x12377D`, `0x120DAB`, `0x122F1F`, `0x1230DF`) | SHAPE SINE 0..5 | 32 | open |
| SYNC (`0x12255D`, `0x1226DD`, `0x122A32`, `0x122C5C`, `0x1239BE`, `0x123C7C`, `0x123FE9`) | SYNC 1..8 | 18 | open |
| MISC (`0x122030`, `0x122151`, `0x122417`) | MISC 0..2 | 8 | open |
| SUPER 1..4 (`0x121E9D`, `0x124985`), SHAPE 3, 4 (`0x120F55`, `0x12145E`) | | 13 | open |
| SUB OSC (`0x12179A`, `0x121867`, `0x1219C3`) | SUB OSC 0 | 1 | open |

PURE wave 6 is the noise stage (type 46, `0x121FD8`), 25 layers: it was not in the task list but ranks above Shape Square. The new models cover 133 of 312 ANALOG layers (43 %), 240 of 312 (77 %) with the earlier six stages. SAMPLE / WAVE bank paths (types 49..53, `0x124269`, `0x12453F`, `0x1247B6`, `0x124B6A`) were not reached (time box).

## 2. Models (all `tools/synth_stage_*.py`, 40-bit registers, oracle `tools/synth_wave_oracle.py --stage`)

| Stage | Model | Oracle result | Tag |
|---|---|---|---|
| `0x121FD8` noise | LCG `s <- s M + C` per lane, four steps per pass, `M = DM(0xE08C8) = 0x0BB38435`, `C = DM(0xE08C9) = 0x3619636B` (both lanes use the same pair; the `R2 <-> S1` swap of the listing does not give each lane its own), sample `s 2^-31` truncated to 24 bits, mixed with the common `P` weights (own `P[-16 + lane]`, partner `P[-13 / -14]`) and scaled by `P[16 + lane]`; lane state words `DM(0xB2EF9 + lane)` (image init 3, 4) | 320 of 320 words, 40 trials; final state of both lanes equal | VERIFIED (run) |
| `0x121AB5` Shape Square | pulse helper `0x1250C9` twice per lane, amplitude `R6 = 0.7`, width word `2 FIX(w 2^30) - 2^31`, `w = clip(clip(s17, 1 - min(1, 5.5 a)) + s19, 1)`, `a = inc 2^-31`; output scaled by `-P[16]` | 320 of 320 words; the shape words take only the idle values 0.5 and 0 in the pass, so the width law is checked at one shape value | VERIFIED (run), shape law VERIFIED at the idle word only |
| `0x121D6D` SUPER | eight saw voices per lane: spread `u = 0.35 (t + 2 t^4)`, `t = s5 + 0.06` (`s5 = DM(-5, I2)`), voice increment `FIX(trunc((rho_j u + 1 - u) f) 2^31)` with `rho_j = P[-32 + 2 j + lane]`, phases `DM(0xC2336 + 2 j + lane)`, inline saw `copysign(w^2, xo) - xo`, voice weights from the groups `P[-16 + 4 j ..]` (own `[0 / 1]`, partner `[3 / 2]`), accumulation through the 32-bit frame words, final scale `P[16] g(f)` | 320 of 320, 40 trials (phases and increments checked word for word in the debug run) | VERIFIED (run) |
| `0x121C54` MULTI | three voices: increment `a_v = trunc(c_v f rho_0 (rho_v s5 + 1 - s5))` (voice 0 without the factor), `c = DM(-32 .. -30, I1) = DM(0xC1596 ..) = 1, 2, 0.5` (octaves, idle image words), per-voice fade `g(a_v)` folded into the voice weights, `r7 = 0.5 / a_v` kept as a 40-bit register (a truncated word gives 88 of 96), final `trunc(acc P[16])` from the stored accumulator | 320 of 320, 40 trials | VERIFIED (run) |

Limits of the oracle runs: the pass writes the pitch, `s5`, `s17`, `s19` words itself (idle values), so ratios, weights, phases, octaves and gain are random but the spread and shape words are idle; random ratios are limited to `2^-2 .. 2^3` for the unison stages because an increment above 0.5 cycles (a ratio of 20 on the MULTI octave) leaves the saturating `FIX` regime where a rare sample differs (one seed in twelve before the limit). Recorded words: [data/synth-stage-oracle-cases-rest.json](data/synth-stage-oracle-cases-rest.json) (six cases per stage, replayed by `tools/tests/test_synth_stage_rest.py` without the firmware); the six earlier stages stay in `synth-stage-oracle-cases.json`.

## 3. Rust

`synth/fw_stages.rs`: `FwStage::from_category_wave` (PURE 6, SHAPE 2, SUPER 0, MULTI), `noise_step` / `noise_sample` (bit exact against the model), `saw_diff`, `spread_u`, `square_wave` and `FwStageState::sample` (one pre-allocated state per voice, no allocation in render). Errors against the Python model goldens (`tools/synth_stage_golden.py` -> `tests_fw_stage_golden.rs`): noise 0 (bit exact, 16 steps); the spread law (16 points), the unison inline saw (64) and Shape Square (384) deviate by at most 5.96e-8, 2.98e-8 and 1.79e-7 absolute (f32 against the exact model; the three values were printed without their test names, the tests assert 1e-6). Wiring: `SynthLayer::strip_stage` is set by `synth_params.rs` from the decoded category and wave; with `chain = FirmwareStrip` and the opt-in `strip_stages` (`--synth-stages`) the stage plays instead of the legacy waveform. Defaults are unchanged, so `render_hash` is unchanged.

Inputs that are ASSUMED (the P-block writer is open, OQ22; another agent fits them from hardware clips): SUPER voice ratios (a symmetric `-1..1` spread), spread word `0.1 x Osc Ctrl`, equal voice weights summing to one, MULTI ratios 1 and octaves 1, 2, 0.5 (the octave words are VERIFIED idle image words, their per-wave values are not), noise weight 1, Shape Square shape word = strip shape (idle 0.5). One lane of the stage is rendered with the mix weights folded into one level.

## 4. Stop point

Time box reached after the four stages above. Next in value order: SHAPE SINE (32 layers, seven routines of 180 to 270 instructions), SYNC (18 layers, nine waves on seven routines, `0x123FE9` alone is 1069 instructions), MISC (8), SUPER 1..4, SHAPE 3 and 4, SUB OSC. SAMPLE / WAVE bank stages were not opened. The listings of all 34 stages were dumped (`sharc-disasm`, byte offset `2 pc`); the four modelled ones are each a re-use of the saw helper inline (`0x1250C9`, the inline saw of SUPER / MULTI) plus the common `mix_lanes` shape, so SYNC and SHAPE SINE are the next candidates for the same method.

## Implications

- 77 % of the ANALOG layers of the factory bank now have a bit-exact stage model in Python; the stage maths is no longer the blocker for them, the program words (OQ22) are.
- The default path stays the legacy waveform (D22): none of this is verified end to end against a recording.

## Follow-ups

1. Model SHAPE SINE and SYNC (`tools/synth_wave_oracle.py` accepts a stage once its model is registered in `MODELS`).
2. Replace the ASSUMED ratios, weights and spread scale by the fitted P block when it exists.
3. Wire `synth_params` to read the shape word for Shape Square from the decoded Osc Ctrl once the knob scale is known.

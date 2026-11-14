# Synth stages, seventh batch: SYNC 3..5 and the SYNC 2 spread probe

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (cut from `bank0_CODE.bin` with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--watch`, `--trace`, `--dump`), `sharc-disasm` (listings `0x2458B8..0x245E3C`, `0x24D148..0x24D2DC`), `docs/research/data/synth-wave-types.csv`, `docs/research/data/synth-sync-table.json`. Builds on [synth-stage-models-5](synth-stage-models-5.md).
- **Question:** Which of the 31 remaining ANALOG layers can be modelled word for word next, and can the SYNC 2 interpolation path be checked at other spread words?

Tags follow `.agents/rules/provenance-in-code.md`. The harness frame is the one of the earlier stages (`tools/synth_wave_oracle.py`): stage in slot 0, `I2 = 0xC1F96`, two oscillator passes (`LCNTR = 2`, the loop swaps the inc / phase slots at its end), the common mix and the scale.

## 1. The writer of `DM(-5, I2)` and the SYNC 2 probe: VERIFIED (run)

`--watch 0xC1F91:2` on a pass names one store site, `pc 0x126960` (`DM(-5, I6) = R5`). The listing around it (`0x24D248..0x24D2D6`) computes

`s5 = clip(F5, 0.5) + 0.5`, `F5 = (DM(-24, I4) + DM(-28, I0)) + (DM(-22, I4) + DM(-26, I0)) DM(-26, I5) + DM(-12, I4) DM(10, I2)`

(pointer words `DM(0xC0B7C ..)`, channel 0: `I0 = 0xC2736`, `I4 = 0xC1E4E`, `I5 = 0xC15B6`, `I2 = 0xC1188`). A poke probe (`DM(addr) = v`, then `--dump 0xC1F91:2`) on the seven words shows that only **`DM(0xC271A)`** survives the pass: poking `0.3` gives `s5 = 0x3F4CCCCD` (0.8), poking `-0.2` gives `0x3E999999` (0.3); pokes of the other terms (`0xC1E36`, `0xC271C`, `0xC1E38`, `0xC1E42`, `0xC159C`, `0xC1192`, `0xC1184`) left `s5` at `0.5` (rewritten by the pass or multiplied by zero at the idle state). The harness now pokes `DM(0xC271A + lane)` with a random `v` in `[-0.55, 0.55]` for SYNC 2 and 3..5 (`tools/synth_wave_oracle.py`, `random_s5`, an own generator so the other trial draws are unchanged), so `s5` spans `0..1` (table index 0..128, every fraction).

Result: SYNC 2 still matches **960 of 960** words over 120 trials with the varying `s5` (the six recorded cases in `data/synth-stage-oracle-cases-rest.json` were re-recorded with it). The interpolation `T[i] + (T[i + 1] - T[i]) frac` is therefore VERIFIED at arbitrary `s5`, no longer only at node 64. Both lanes received the same `s5` in every trial although the two source words differ (the listing swaps the lanes before the store: `R1 <-> S5`), so the lane-dependent `T[i + 1]` select of the SIMD register swap `R5 <-> S4` is never exercised with two different indices: the firmware never has two indices either (VERIFIED by the 120 trials, not by a unit test). The pitch word is still the one value `0.00603` in every trial (its writer was not searched), so the `trunc(h) = 4` branch of SYNC 2 and `h < 16` stay the only exercised range of `lim`.

## 2. SYNC 3..5, `0x122C5C` (5 layers): VERIFIED (run)

`tools/synth_stage_sync3.py`, oracle `python tools/synth_wave_oracle.py --stage 0x122C5C`: **320 of 320** words (40 trials), 960 of 960 over 120 trials; six recorded cases. Waves 3, 4 and 5 of category 2 (`data/synth-wave-types.csv`) share the stage. Different code from `0x122A32` (a listing diff matches 40 %): the same table lookup at `s5`, but the oscillator is a pair of band-limited square edges and the stage ends in the output high pass of SHAPE SINE 4. With `f` the pitch word, `a = inc1 2^-31` (oscillator 1 only), `h` the table value, `p = DM(-8, I1) 2^-31` (signed fraction; idle word `0x40000000`, `p = 0.5`), `x_k = phase + k inc`:

| Quantity | Law (listing `0x2458B8..0x245E3C`) |
|---|---|
| `h` | table interpolation at `s5` (as `0x122A32`); `a h` multiplies the **register** `h`, the stored word enters later |
| `e`, `r`, `w1` | `e = 1 - clip(4 a h, 1)`, `r = 2 (p - 0.5)`, `w1 = clip(r, e)` (stored) |
| `c1`, `q`, `rt` | `c1 = 0.5 + w1 / 2` (stored), `q = max(0.5 / (a h), 2)` (stored), `rt = (1 - \|r\|) / (1 - \|w1\|)` (stored) |
| `c2`, `q2` | `c2 = 0.5 - e2 / 2` with `e2 = 1 - clip(4 a, 1)`, `q2 = max(0.5 / a, 2)` (stored) |
| `g1`, `g2` | `C (B + clip(A - a h, B))`, `C (B + clip(A - a, B))` with the saw fade constants (stored) |
| `y_k` | `ASHIFT(UUF(x_k, FIX(h, 25)), 7) + pulse` (integer wrap): the phase times `h` plus the pulse offset |
| edge | `E(v, c, q) = sg - copysign(u^2, sg)`, `d = \|v\| - c`, `sg = sgn(d)`, `u = 1 - min(1, \|d q\|)` |
| `tA`, `tB` | `E(y_k 2^-31, c1, q)` (stored), `E(x_k 2^-31, c2, q2)` |
| sample | `((0.5 tB + 0.5)(tA - 1) + 1) + w1` |
| output | the common mix with the scale `g2 P[16] g1 rt`, then the one-pole high pass `y_k = (x_k - x_(k-1)) + c y_(k-1)` over the four words (state and `c` at `DM(-32 / -30 / -28, I7)`, `I7 = DM(-9, I3)`, as `0x120DAB`) |

Two findings of the run. (1) `R8 = R12` in `R8 = R12, F12 = F8 COPYSIGN F12` copies the **sign register** (`R12 = copysign(1, d)` after the earlier `F12 = F11 COPYSIGN F12`), not `d`: the first reading `d - sgn(d) u^2` matched 88 of 320 words and stored `1.0` where the model had `0.82`; with `sg` the same as SUPER 1 (`sg - copysign(u^2, sg)`) 247 of 320 matched. (2) `F2 = F2 * F4` after the table lookup reads the 40-bit register `h` (`RND32`), not the stored word: using the stored word was 1 ulp off in `e`, `w1`, `c1`, `q`, `rt`; with the register 320 of 320.

The stored scratch words (`I4 = 0xC1EEE`) were compared with the model in a single pass (`h`, `w1`, `c1`, `rt`, `q`, `g1`, `c2`, `q2`, `g2`, and the `tA` buffer): equal. `w1` comes out as `-0.0` (`0x80000000`) in the oracle when `e = 0`; the model stores `+0`, which no later operation distinguishes.

Rust: `synth/fw_sync3.rs` (`sync3_core`, `edge`), `FwStage::Sync3` = category 2 waves 3..5 with the shared `OutputHighPass`; the pulse word is the idle `0x40000000` (VERIFIED value, writer open: ASSUMED constant), the spread word is the Osc Ctrl value as in `Sync2` (ASSUMED). Golden `SY3_*` (160 rows over five shape words, `tools/synth_stage_golden6.py`), worst deviation below 2e-5.

## 3. Coverage

ANALOG layers with a bit-exact stage model: 281 + 5 (SYNC 3..5) = **286 of 312 (91.7 %)**. Filter layers unchanged (374 of 442, 84.6 %).

## 4. What remains (26 layers)

| Stage | Layers | Notes from the listing |
|---|---|---|
| `0x1239BE` SYNC 6 | 3 | 305 instructions, 74 % identical to `0x122C5C`: no pulse word, the fixed `R15 = 0.5` width, an extra `DM(-20, I4)` store, a `BTGL 31` half-cycle block, a different output scale (`R1 = -0.5`) and **no** output high pass (diff of the two listings, not decoded) |
| `0x123FE9` SYNC 8 | 1 | 281 instructions, 75 % identical to `0x122C5C` (same family) |
| `0x123C7C` SYNC 7 | 2 | 357 instructions, 54 % identical |
| `0x1226DD` SYNC 1 | 1 | 188 instructions, 48 % identical |
| `0x124985` SUPER 2..4 | 8 | 195 instructions, delay-line / filter module, needs a fuller harness frame ([synth-stage-models-4](synth-stage-models-4.md) s3) |
| `0x122030`, `0x122151`, `0x122417` MISC | 3 + 2 + 3 | 120, 41 and 156 instructions, not read |
| `0x120F55`, `0x12145E` SHAPE 3 / 4 | 1 + 1 | 264 and 198 instructions, not read |
| `0x12179A` SUB OSC | 1 | 81 instructions, not read |

## 5. Method notes

- Environment as in [synth-stage-models-5](synth-stage-models-5.md) s6; `python tools/synth_wave_oracle.py --image SRAM_ORG/core2_sram.bin --stage 0x122C5C --sharc-org target/release/sharc-org`. Debug aids used: one pass with `--buf` on the scratch (`I4 + -32 .. +32`) to compare the stored words, then `--trace pc:pc` for the registers at one instruction.
- A transfer in a multifunction line reads **current** registers: look at which earlier instruction last wrote the source (here a `COPYSIGN` result).
- Float registers keep 40 bits: a product of a looked-up value reads the register, a later `DM` reload reads the truncated word.

## Implications

- 91.7 % of the factory ANALOG layers have a verified stage; `--synth-stages` plays SYNC 3..5 with the same ASSUMED level, ratio and pulse conventions as the earlier stages.
- `s5` can be varied in the harness; the SHAPE SINE stages were verified at the idle value only and can be re-run with `S5_STAGES` extended.

## Follow-ups

1. SYNC 6 and SYNC 8 (the same family as `0x122C5C`, a listing diff is the cheapest next step), then SYNC 7 and SYNC 1.
2. Extend `S5_STAGES` to the SHAPE SINE stages that read `s5` and re-run them.
3. The pitch word writer (`DM(-29, I2)`, always `0.00603`) to exercise `trunc(h) > 4` and the fade branches.
4. MISC (8), SHAPE 3 / 4 (2), SUB OSC (1), SUPER 2..4 (8).

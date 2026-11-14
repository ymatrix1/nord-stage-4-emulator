# Synth stages, sixth batch: SUPER 1, SHAPE SINE 3 and SYNC 2

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (cut from `bank0_CODE.bin` with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--trace`, `--buf`), `sharc-disasm` (listings `0x243D3A..0x243FAE`, `0x246EFA..0x24737A`, `0x245464..0x2458B6`, the head of `0x2458B8`), `docs/research/data/program-sweep.json`, `data/synth-wave-types.csv`. Builds on [synth-stage-models-4](synth-stage-models-4.md).
- **Question:** Which of the 43 remaining ANALOG layers can be modelled word for word next, in order of layer count per effort?

Tags follow `.agents/rules/provenance-in-code.md`. All three stages share the harness frame of the earlier ones (`tools/synth_wave_oracle.py`): stage in slot 0, `I2 = 0xC1F96`, `I4 = scratch`, two oscillator passes (`LCNTR = 2`) or a voice loop, the common mix (`mix_lanes` or its inline copy) and the scale `g P[16]`.

## 1. SUPER wave 1, `0x121E9D` (3 layers): VERIFIED (run)

`tools/synth_stage_super1.py`, oracle `python tools/synth_wave_oracle.py --stage 0x121E9D`: **320 of 320** words (40 trials). Six recorded cases in `data/synth-stage-oracle-cases-rest.json`.

Same lane setup as `0x121D6D` (fade `g`, `r7 = 0.5 / f`, detune law `u`, increments `(rho_j u + 1 - u) f`, mix through `DM(I5, M2)` weight pairs, accumulators through memory), but the loop is `LCNTR = 7` (voices 0..6; the eighth increment is computed and not used) and the per-sample shape is a band-limited square, not a saw. Per voice phase word `x_k = phase + k inc`:

| Quantity | Law (listing `0x243E8E..0x243F20`) |
|---|---|
| `d` | `\|x_k 2^-31\| - 0.5` |
| `w` | `1 - min(1, \|d r7\|)` |
| `sg` | `copysign(1, d)` (the sign comes from `R12 = R0`, a register copy of `d`) |
| `s_k` | `sg - copysign(w^2, sg)` = `sg (1 - w^2)`: `+-1` away from the edges, a parabola dip at `\|x\| = 0.5` |

The first reading (`w^2 - copysign(w^2, d)`) matched zero words: a register copy `R8 = R12` (the sign register) sits in the same instruction as the `COPYSIGN` that reads `F8 = w^2`, so after it `F8` holds `sg`, not `w^2`. A trace of the voice (`--trace 0x121E9D:0x121FD8`, register dump at `pc 0x121F94`: `R0..R3 = 1`) showed the square.

Rust: `synth/fw_super1.rs` (`fold_diff`, `super1_sample`), `FwStage::Super1` = category 6 wave 1, golden `tests_fw_stage_golden5.rs` (`FOLD_*`, 80 rows, `tools/synth_stage_golden5.py`), worst deviation below 1e-5.

## 2. SHAPE SINE wave 3, `0x12377D` (3 layers): VERIFIED (run)

`tools/synth_stage_shape_sine3.py`, oracle `--stage 0x12377D`: **320 of 320** words (40 trials). Six recorded cases. Two oscillator passes (second on `inc2` / `phase2`), no output high pass. With `f` = pitch word, `s5 = DM(-5, I2)`, `a = x 2^-31`:

| Quantity | Law (listing) |
|---|---|
| `g` | saw fade `C (B + clip(A - f, B))` of the pitch word (not of `inc1`), stored |
| `vr` | `(1 - (1 - s5)^2)(1 - min(1, 5.5 f))` (the wave 0 width) |
| `rA`, `rB` | `1 / (1 - vr)`, `1 / (1 + vr)` (two Newton steps each, stored) |
| `P0`, `K` | `P0 = pi (rA - rB)` (`pi = 0x40490FDB`, stored), `K = (P0 2/3) f` |
| `r7`, `c` | `0.5 / f`, `c = (2/pi) vr` (`0x3F22F983`, stored) |
| warp | `y = (a + 1) rA` if that is below 1, else `(a - 1) rB` (wave 0 law, `COMP` / `IF GE`) |
| sine term | word `2 FIX(y, 30) + 0x40000000`, sine helper `0x12522E`, `+ c`, stored |
| cubes | `cube(x) = (1 - min(1, \|x 2^-31\| r7))^3 K` at `x_k - 0x80000000` (added to the sine term, stored) and at `x_k + FIX(vr, 31)` |

The sine term, the two cubes and the mix reuse `synth_stage_shape_sine.sine_of_word` and `mix_lanes`; the first run matched.

Rust: `synth/fw_sine3.rs` (`shape_sine3_core`), `FwStage::ShapeSine3` = category 4 wave 3, golden `SS3_*` (96 rows), worst deviation below 2e-5.

## 3. SYNC wave 2, `0x122A32` (6 layers): VERIFIED (run, one shape word)

`tools/synth_stage_sync.py`, oracle `--stage 0x122A32`: **320 of 320** words (40 trials), six recorded cases. Hard sync with a table-driven ratio; two oscillator passes, the usual mix with the scale **`-g P[16]`** (`F0 = -F0` before the product).

| Quantity | Law (listing `0x24548A..0x245586`) |
|---|---|
| index | `n = FIX(s5, 30)`; `i = FEXT(n, 23:8)` (128 steps per unit of `s5`); fraction `FDEP(n, 8:23) 2^-31` |
| `h` | `T[i] + (T[i + 1] - T[i]) frac`, table `DM(0xB074C ..)` (258 words, [synth-sync-table.json](data/synth-sync-table.json)); stored |
| `g` | `C (B + clip(A - f h, B))` (stored) |
| `ih`, `frac(h)` | `TRUNC h`, `h - ih` (stored) |
| `lim`, `m` | `ih 2^25` (integer `CLIP` limit), `FIX(h, 25)` (`UUF` multiplier) |
| `q`, `r7`, `off` | `0.5 / (f h)`, `0.5 / f` (both stored), `FIX(-1 / h, 31)` (`-0.5 / h` cycles) |
| block A | `w_A = (1 - min(1, \|FLOAT(ASHIFT(CLIP(UUF(x + off, m), lim), 7) ^ 2^31) q\|))^2` |
| block B | `u = FLOAT(ASHIFT(UUF(x, m), 7) ^ 2^31)`, `d = u - copysign(w_A, u)` (stored) |
| output | `-frac copysign((1 - min(1, \|x\| r7))^2, x) - d` |

`UUF` is the unsigned fractional multiply, the upper word of the 64-bit product without a left shift (the oracle agrees: `uuf(0x80000000, 0x80000000) = 0x40000000`). `ASHIFT 7` then wraps the `h`-cycle ramp into `ih` saw teeth; `BTGL 31` adds half a cycle; the integer `CLIP` stops the ramp after `ih` cycles.

**Open (one shape word):** the oracle pass writes `s5 = 0.5` every trial (`0x3F000000` in all six recorded cases), so the table index is 64 with fraction 0: the interpolation (`FEXT` / `FDEP` split, `T[i + 1]`) is VERIFIED only at that node and the other 256 entries are the raw table; the fraction path is ASSUMED to be a plain linear interpolation (the listing `F5 = F5 - F4; F5 = F5 F6; F4 = F5 + F4` says so; the SIMD register swap `R5 <-> S4` that selects `T[i + 1]` was not checked at another index). A probe that varies `s5` needs the writer of `DM(-5, I2)`. Likewise the pitch word is the same in all trials (`0.00603`), so the `trunc(h) = 4` branch of `lim` is the only one exercised; `h` stays below 16.

The table (VERIFIED: `sharc-org --buf 0xb074c:264:f32` after the boot, raw words) rises roughly as `2^(i / 32)` up to entry 127, holds 16.0 at entry 128, restarts at 0.0922 at entry 129 and falls to `0x358637BD` (1e-6) at entry 257. It is a derived 258-word numeric table, committed as JSON and as `synth/fw_sync_table.rs` (generated by `python tools/synth_stage_sync.py`, tested against the JSON); no code or image bytes.

Rust: `synth/fw_sync.rs` (`sync_ratio`, `sync2_core`), `FwStage::Sync2` = category 2 wave 2, golden `SY2_*` (160 rows over five shape words, so the interpolation fraction is exercised in the port against the model), worst deviation 4.2e-7.

## 4. Coverage

ANALOG layers with a bit-exact stage model: 269 + 3 (SUPER 1) + 3 (SHAPE SINE 3) + 6 (SYNC 2) = **281 of 312 (90.1 %)**. Filter layers unchanged (374 of 442, 84.6 %).

## 5. What remains (31 layers)

| Stage | Layers | Notes from the listing |
|---|---|---|
| `0x122C5C` SYNC 3..5 | 5 | **not the same code as `0x122A32`** (a diff of the two listings shows a different setup: pulse word `DM(-8, I1)`, `MAX(F0, F2)` clamps, a second fade `DM(-10, I4)`, no integer `CLIP`); same oscillator frame, 311 instructions |
| `0x1239BE` SYNC 6, `0x123C7C` SYNC 7, `0x123FE9` SYNC 8, `0x1226DD` SYNC 1 | 3 + 2 + 1 + 1 | not read |
| `0x124985` SUPER 2..4 | 8 | delay-line / filter module, see [synth-stage-models-4](synth-stage-models-4.md) s3 |
| `0x122030`, `0x122151`, `0x122417` MISC | 3 + 2 + 3 | not read |
| `0x120F55`, `0x12145E` SHAPE 3 / 4 | 1 + 1 | not read |
| `0x12179A` SUB OSC | 1 | not read |

## 6. Method notes

- Environment: build the images with `python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP`, `python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram SRAM_ORG`, then `python tools/synth_wave_oracle.py --image SRAM_ORG/core2_sram.bin --stage 0x122A32 --sharc-org target/release/sharc-org` (on Windows pass `C:/...` paths to Python, not MSYS paths).
- A multifunction instruction that also contains a register transfer (`R8 = R12, F12 = F8 COPYSIGN F12`) reads all sources before any write: when a model matches zero words, check whether a transfer replaces a register that a later instruction reads as a float.
- A pass-written word (`s5`, pitch) is read back after the pass; it cannot be varied by a poke, so stages whose behaviour depends on `s5` (tables, widths) are verified at the idle value only.

## Implications

- 90 % of the factory ANALOG layers have a verified stage; the opt-in `--synth-stages` path plays SUPER 1, SHAPE SINE 3 and SYNC 2 with the same ASSUMED level and ratio conventions as the earlier stages.

## Follow-ups

1. SYNC 3..5 (`0x122C5C`, 5 layers), then `0x1239BE`, `0x123C7C`, `0x123FE9`, `0x1226DD`.
2. A probe that varies `s5` for the SYNC table stages (find the writer of `DM(-5, I2)` in the pass or poke the strip word that feeds it).
3. MISC (8), SHAPE 3 / 4 (2), SUB OSC (1), SUPER 2..4 (8).

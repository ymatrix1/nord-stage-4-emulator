# Synth stages, seventh batch: SYNC 6, 7, 8 and 1, the pitch writer and varying spread

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (cut from `bank0_CODE.bin` with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--watch`, `--buf`, `--msg-script`), `sharc-disasm` (listings `0x24737C..0x2478F6`, `0x2478F8..0x247FD0`, `0x247FD2..0x2484D0`, `0x244DBA..0x245122`, the helpers `0x24A45C` and `0x24C500..0x24C5D6`), the constant table `DM(0xC1DE2..)`. Builds on [synth-stage-models-6](synth-stage-models-6.md).
- **Question:** Which of the 26 remaining ANALOG layers can be modelled word for word next, where is the pitch word written (the oracle pass used 0.006028 in every trial, so the fade and `trunc(h) > 4` branches of the SYNC stages were unexercised), and do the SHAPE SINE stages hold at varying `s5`?

Tags follow `.agents/rules/provenance-in-code.md`. All stages share the harness frame of `tools/synth_wave_oracle.py` (stage in slot 0, `I2 = 0xC1F96`, `I4 = scratch`, two oscillator passes `LCNTR = 2`, the common `mix_lanes`).

## 1. Reading the listings

The disassembler prints some constant loads as `DM(-12, I0)`: `I0 = 0xC1E02` points at a table of identical word pairs `DM(0xC1DE2..)` (VERIFIED, `--dump 0xc1de2:40`): offset -32..6 in steps of 2 give 0.125, 0.25, 0.375, 0.5, 0.75, 0.99, 0, `0x7EB851EB`, `0xC0000000`, `0x80000000`, 25, 31, -31, -15, -35, 1, 2, 4, -1, 1.5708. `0x123C7C` uses them instead of immediates (`{25}` = `R14 = 0x19`), which is why it looked unlike `0x1239BE`. Each such load is fused with a compute instruction in the same word (`R14 = DM(-12, I0), R3 = R1 + R14`).

## 2. SYNC wave 6, `0x1239BE` (3 layers): VERIFIED (run)

`tools/synth_stage_sync6.py`, oracle `python tools/synth_wave_oracle.py --stage 0x1239BE`: **320 of 320** words (40 trials, varying `s5` and pitch). Six recorded cases in `data/synth-stage-oracle-cases-rest.json`.

Same frame as `0x122C5C` but no pulse word, no output high pass, a fixed first edge width 0.5 and a ramp factor. With `f` the pitch word, `a = inc1 2^-31`, `h = interp(s5)` ([synth-stage-models-5](synth-stage-models-5.md) s3), `x_k = phase + k inc`, `wave(|x|, c, q) = sg - copysign(u^2, sg)` of `0x122C5C`:

| Quantity | Law (listing `0x24737C..0x2478F6`) |
|---|---|
| `g1`, `g2` | `fade(a h)`, `fade(a)` (stored), `fade(x) = C (B + clip(A - x, B))` |
| `q`, `q2` | `max(0.5 / (a h), 2)`, `max(0.5 / a, 2)` (stored) |
| `e2`, `c2` | `e2 = 1 - clip(4 a, 1)` **stored and reloaded** (24-bit truncation), `c2 = 0.5 - e2 / 2` (stored) |
| `k` | `c2 / (1 - c2)` (two Newton steps on the register `c2`, stored) |
| `A_k` | `wave(\|ASHIFT(UUF(x_k, FIX(h, 25)), 7)\|, 0.5, q) g1 + 1` (stored) |
| `P_k` | `A_k (0.5 wave(\|x_k\|, c2, q2) + 0.5)` (stored) |
| out | `P_k (g2 FLOAT(x_k ^ 2^31) - 1) + (1 - 0.5 k)`, scale `-0.5 g2 P[16]` |

The first run missed 5 of 40 words by one ulp: `DM(0, I4) = R2; R5 = DM(0, I4)` sends `e2` through memory, so it is truncated to 24 bits before it feeds `c2` (the other SYNC stages pass `e2` in a register). Rust: `synth/fw_sync6.rs` (`sync6_core`), `FwStage::Sync6` = category 2 wave 6, golden `SY6_*` (160 rows), worst deviation below 2e-5.

## 3. SYNC wave 8, `0x123FE9` (1 layer): VERIFIED (run)

`--stage 0x123FE9`: **320 of 320** words. The `0x1239BE` frame with these differences (listing diff, 82 % identical):

| Change | Listing |
|---|---|
| `s5` is scaled by 0.75 before `FIX BY 30` | `0x248002..0x248008` |
| `h = 2 interp(0.75 s5)` (`F4 = F4 + F4`, stored) | `0x24804A` |
| `R1 = 0` (not -1): `c2 = 0.5`, `k = 1` | `0x2480E2` |
| the plain-phase edge uses `x - 0xC0000000 - 2 inc` (a quarter cycle minus two increments) | `0x2482F2..0x248302` |
| no `BTGL 31` ramp block; out `= A_k (0.5 tB + 0.5) - (1 - 0.5 k)` | `0x2483EA..0x248406` |
| scale `g2 P[16]` (no -0.5) | `0x248496` |

Rust: `sync8_core`, `FwStage::Sync8` = category 2 wave 8, golden `SY8_*`, below 2e-5.

## 4. SYNC wave 7, `0x123C7C` (2 layers): VERIFIED (run)

`--stage 0x123C7C`: **320 of 320** words (the first model matched all 48 words of the first six trials). Frame of `0x1239BE`, `h = 2 interp(s5)` (not scaled by 0.75), three stacked edges:

| Quantity | Law (listing `0x2478F8..0x247FD0`) |
|---|---|
| `ah`, `ah / 4` | `a h` (`DM(4, I4)`), `0.25 a h` (`DM(6, I4)`); `q = max(0.5 / ah, 2)`, `q3 = max(0.5 / (ah / 4), 2)`, `g1 = fade(ah)`, `g3 = fade(ah / 4)` (stored) |
| `h4` | `0.25 h` stored at `DM(-22, I4)`: the second multiplier `FIX(h4, 25)` |
| `A_k` | as `0x1239BE` (stored) |
| `P_k` | `A_k (wave(\|ASHIFT(UUF(x_k, FIX(h4, 25)), 7) + 2^30\|, 0.5, q3) g3 + 1)` (the sum register is not stored, the product is) |
| `Q_k` | `P_k (0.25 wave(\|x_k\|, c2, q2) + 0.25)` (stored) |
| out | `Q_k (g2 FLOAT(x_k ^ 2^31) - 1) + cc`, `cc = 1 - c2 / (2 (1 - c2))` (stored), scale `-0.5 g2 P[16]` |

Rust: `sync7_core` (same file), `FwStage::Sync7` = category 2 wave 7, golden `SY7_*`, below 2e-5.

## 5. SYNC wave 1, `0x1226DD` (1 layer): VERIFIED (run)

`tools/synth_stage_sync1.py`, `--stage 0x1226DD`: **320 of 320** words. The only SYNC stage on the saw-family helpers: the parabola helper `0x1251D4` ([synth-wave-models](synth-wave-models.md), `synth_wave_model.helper`) twice and the sine helper `0x12522E` once (which takes the phase word as is, no `0xC0000000` offset: the first run used the offset of `sine_helper` and matched 0 of 48).

| Quantity | Law (listing `0x244DBA..0x245122`) |
|---|---|
| `h`, `fh`, `r` | `h = interp(s5)` (stored), `fh = h f` (stored), `r = 0.5 / fh` (stored, no floor), `g = fade(h f)` (stored) |
| phase | `y_k = ASHIFT(SSFR(x_k, FIX(h, 25)), 6)` |
| `S1`, `S2` | `helper(y, fh, r)` stored; `helper(y - 2^31, 4/3 fh, r)` (the second call sees `R6` already scaled) |
| `D_k` | `S1 - S2` (stored) |
| out | `D_k (0.5 - 0.5 sine(x_k))`, scale `g P[16]`, output high pass of `0x120DAB` |

`SSFR` is the signed fractional multiply, upper word of `2 a b`, **rounded to nearest with ties to even** (VERIFIED: at `s5 = 1` the ratio is 16 and `FIX(h, 25) = 2^29`, so every fourth product is a tie; round-half-up missed 4 of 320 words, ties-to-even matches all). Rust: `synth/fw_sync1.rs` (`sync1_core`) + `OutputHighPass`, `FwStage::Sync1` = category 2 wave 1, golden `SY1_*`, below 3e-5.

## 6. The pitch word and varying inputs

**Pitch writer (VERIFIED).** `sharc-org --watch 0xc1f79:2` finds the store `DM(0xC1F79) = R8` at `pc 0x1262E8` (listing `0x24C500..0x24C5D6`): the pass sums integer words (`DM(-32, I0)`, `DM(-30, I4)`, a bend term `DM(10, I2)` and a vibrato term), clips at `0x58000000`, splits into a 9-bit index (`FEXT 23:9`) and an 8..23 fraction and interpolates a table at `DM(0xC1476) = 0xB069B`. Poking the note term `DM(0xC2716)` (read through `I0 = 0xC2736`, shared by both lanes) moves the pitch word by one semitone (x 1.0595) per `0x01000000` over -36..72 semitones (0.00075 .. 0.386; 0 gives 0.006028). The ARM writer of `DM(0xC2716)` and the table `0xB069B` are not read (OQ22).

`tools/synth_wave_oracle.py` now pokes it for the SYNC stages (`random_pitch`: -24 .. +48 semitones, capped so that `f max(rho) <= 0.45` cycles per sample, because beyond Nyquist the oracle words are not music; the first uncapped draw (pitch up to 0.1 with ratios up to 39) gave 24 to 49 mismatches of 320 per SYNC stage, all with an increment above 0.4 where the 32-bit phase runs past Nyquist and the oracle returns `-0.0`). With the probe **SYNC 1, 2, 3, 6, 7, 8 all give 320 of 320** (the earlier six-trial SYNC 2 / 3 results hold at varying pitch, the fade branch `a h > 0.2278` and `trunc(h) > 4` are now exercised).

**Varying `s5` for SHAPE SINE (VERIFIED).** The persistent source of `s5` (`DM(0xC271A)`, [synth-stage-models-6](synth-stage-models-6.md) s3) is poked for all six SHAPE SINE stages (`0x123427`, `0x1235C9`, `0x1232A5`, `0x12377D`, `0x120DAB`, `0x122F1F`): **320 of 320** for each, so the `s5` laws of the earlier batches (width `1 - (1 - s5)^2`, resonance `k`, gain ceiling `1 + 15 s5^2`, polynomial `p(s5)`) hold over `s5` in 0..1, not only at the idle 0.5. The recorded cases of these stages and of SYNC 2 / 3 were re-recorded with the new draws.

## 7. Coverage

ANALOG layers with a bit-exact stage model: 286 + 3 (SYNC 6) + 1 (SYNC 8) + 2 (SYNC 7) + 1 (SYNC 1) = **293 of 312 (93.9 %)**. Filter layers unchanged (374 of 442, 84.6 %).

## 8. What remains (19 layers)

| Stage | Layers | Notes |
|---|---|---|
| `0x124985` SUPER 2..4 | 8 | delay-line / filter-like module, PM table reads and an `MRF` / `MRB` multiply-accumulate loop; needs a fuller harness frame ([synth-stage-models-4](synth-stage-models-4.md) s3) |
| `0x122030`, `0x122151`, `0x122417` MISC | 3 + 2 + 3 | not read |
| SHAPE 3 / 4 | 2 | not read |
| SUB OSC | 1 | not read |

## Implications

- The ANALOG synth now has a stage model for every SYNC wave of the catalogue; the only SYNC routine without layers is `0x12255D`.
- The oracle probes cover pitch and spread; ratios, weights and the real note term still come from the unknown P-block writer (OQ22).
- `SSFR` rounds ties to even: other ports that used round-half-up (`organ`, `dsp_frame_rate`) only differ on exact ties.

## Follow-ups

1. SUPER 2..4 (`0x124985`) with a fuller frame; then MISC, SHAPE 3 / 4, SUB OSC.
2. Find the ARM writer of `DM(0xC2716)` and the table `0xB069B` (note to pitch word).
3. The `FwStage` variants are wired behind `--synth-stages` only; the real ratio and weight words stay ASSUMED.

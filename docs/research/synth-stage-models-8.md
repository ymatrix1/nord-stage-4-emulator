# Synth stages, eighth batch: MISC 0, 1, 2 and SUB OSC

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (cut from `bank0_CODE.bin` with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--watch`, `--diff`, `--buf`, `--msg-script`), `sharc-disasm` (listings `0x244060..0x2442A0`, `0x2442A2..0x244370`, `0x24482E..0x244AB8`, `0x242F34..0x2430CC`), the SYNC table `DM(0xB074C..)` (`data/synth-sync-table.json`). Builds on [synth-stage-models-7](synth-stage-models-7.md).
- **Question:** Which of the 19 remaining ANALOG layers (SUPER 2..4 8, MISC 8, SHAPE 3 / 4 2, SUB OSC 1) can be modelled word for word next?

Tags follow `.agents/rules/provenance-in-code.md`. All four stages share the harness frame of `tools/synth_wave_oracle.py` (stage in slot 0, `I2 = 0xC1F96`, two passes or four-sample loops, the common mix). Result: **MISC 0, 1, 2 and SUB OSC are bit exact (320 of 320 words over 40 random trials each)**; SUPER 2..4 and SHAPE 3 / 4 were not reached in the time box.

## 1. The MISC family (`0x122030`, `0x122151`, `0x122417`)

The three stages are the three MISC waves in table order (`synth-wave-stages`: 0 = `0x122030`, 1 = `0x122151`, 2 = `0x122417`; 3 + 2 + 3 layers). The routine between them (`0x1221B9`, listing `0x244372..0x244644`) and the next (`0x122323`) have no ANALOG layer and were not modelled. `tools/synth_stage_misc.py`, oracle `python tools/synth_wave_oracle.py --stage 0x122030` (and `0x122151`, `0x122417`).

### 1.1 MISC 0, pink noise (`0x122030`, 3 layers): VERIFIED (run)

Per lane, four steps `k`; the generator is the LCG of the noise stage ([synth-stage-models](synth-stage-models.md): state `DM(0xB2EF9 + lane)`, `s <- s M + C`), `v = FLOAT(s, -31)`:

| Step | Law (listing `0x244070..0x2441DA`) |
|---|---|
| counter | `DM(13, I2)` (`0xC1FA3 + lane`) is incremented and stored at the start of each step |
| rows | ten rows at `I7 + 2 j + lane`, `I7 = DM(-21, I3) - 32`; `c` = counter, mask `0x3FF`: for `j = 0..9`, `if c & mask == 0: row[j] = trunc(v), c = mask`; `mask >>= 1` (a Voss-McCartney generator: row 9 refreshes every other step, row 0 every 1024th) |
| sum | `F8 = sum of the ten rows` in order, each add rounded; a second LCG step `v2` is added, `sample = trunc(0.2 F8)` (`0x3E4CCCCD`) |
| tail | the one-pole output high pass of `0x120DAB` over the four samples (`HP_IDLE`, states at `DM(0xC22D6..0xC22DA)`), then the common mix `(partner w0 + own w1) P[16]` of the noise stage |

The state area is not the frame: `I3 = 0xC0B88` is the module record, `DM(-21, I3) = DM(0xC0B73) = 0xC2556` (rows at `0xC2536 + 2 j + lane`) and `DM(-9, I3) = DM(0xC0B7F) = 0xC22F6` (high pass). Found by a `--watch` of the row stores (`pc 0x122096..0x1220A8`) and a dump of the idle pass; the first harness run poked the row pointer into the frame word `DM(-21, I2)` and matched 0 of 320 words. The oracle now pokes the rows, counter and high pass words (`random_misc`), 320 of 320.

### 1.2 MISC 1, low-passed noise (`0x122151`, 2 layers): VERIFIED (run)

Per step: `v = FLOAT(LCG, -31)`, `z <- (v - z) 0.0005 + z` (`0x3A03126F`, the state `DM(-3, I2)` is stored, so reloaded truncated), `sample = trunc(64 z)`; no high pass, the common mix. 320 of 320 on the first run.

### 1.3 MISC 2, cubed phase-modulated sine (`0x122417`, 3 layers): VERIFIED (run)

With `a = inc 2^-31`, `inc = FIX(f P[-32], 31)` (stored `DM(-25, I2)`), `g1 = fade(a)` (stored `DM(-22, I4)`, `I4 = 0xC1E8A`), `v = T(s5)` (SYNC table `DM(0xB074C..)` at `FIX(s5, 30)`, [synth-stage-models-5](synth-stage-models-5.md) s3; `R4 = DM(-5, I2)` is `s5`, the first reading took the FIX of `a` and matched 0 words), `u = a v / 2`, `g2 = fade(u)` (stored `DM(-20, I4)`), `inc2 = FIX(clip(u, 0.5), 31)`:

| Quantity | Law (listing `0x244848..0x244A54`) |
|---|---|
| modulator | `s_k = 0.5 sine(phase2 + k inc2) + 0.5`, `w_k = s_k^3 g2 + (0.25 - 0.25 g2)` (stored `DM(-13 + 2 k)`) |
| carrier | `out_k = sine(phase1 + k inc) w_k (1.5 g1)` |
| tail | output high pass, common mix |

`sine` is the polynomial of the helper `0x12522E` on the phase word without the `0xC0000000` offset (`synth_stage_shape_sine.sine_of_word`; with `synth_wave_model.sine_helper` the first run matched 0 of 320). Both phases advance by `4 inc` / `4 inc2`. 320 of 320 at varying pitch and `s5`.

Rust: `synth/fw_misc.rs` (`pink_sample`, `lowpass_sample`, `fm_core`, `fm_ratio`), `FwStage::MiscPink / MiscLowpass / MiscFm` = category 7 waves 0, 1, 2; the pink rows, counter and low-pass state are fields of `FwStageState`, the modulator phase is `phases[0]`. Golden `tests_fw_stage_golden8.rs`: 32-sample sequences of the two noise stages from the default state (< 2e-5) and the FM grid (< 3e-5).

## 2. SUB OSC, `0x12179A` (1 layer): VERIFIED (run)

`tools/synth_stage_sub.py`, `--stage 0x12179A`: **320 of 320** words at varying pitch and `s5`. Two oscillator passes (phases `DM(-21 / -19, I2)`, the loop swaps the two phases and increments through the frame), each the sum of a square one octave down and a saw. The firmware phase `u` advances by **half** the pitch increment (`SSFR(inc, 0x40000000)`, ties to even):

| Quantity | Law (listing `0x242F86..0x24308A`) |
|---|---|
| pulse | `pulse_helper(u, inc/2, offset 0x80000000, amp = s5)` (helper `0x1250C9`), stored |
| saw | `saw_helper(2 u, inc, amp = 1 - s5)` (helper `0x12501F`), `F6 = 1 - s5` |
| osc | `saw_k + stored(pulse_k)`; new phase = the one the pulse helper returns |
| out | `mix_lanes` of the two passes, scale `P[16] (1 - 0.3 s5)` (`0x3E99999A`) |

Rust: `synth/fw_sub.rs` (`sub_core`), `FwStage::SubOsc` = category 1 wave 0, phase state `phases[0]` advancing `dt / 2`. Golden `SUB_*` (160 rows), < 3e-5.

## 3. Recorded cases and checks

Six oracle cases per stage are in [data/synth-stage-oracle-cases-rest.json](data/synth-stage-oracle-cases-rest.json) (`tools/synth_stage_record.py`, now also drawing the MISC states); `tools/tests/test_synth_stage_rest.py` replays them without the firmware. The golden generator is `tools/synth_stage_golden8.py`.

## 4. Coverage

ANALOG layers with a bit-exact stage model: 293 + 3 (MISC 0) + 2 (MISC 1) + 3 (MISC 2) + 1 (SUB OSC) = **302 of 312 (96.8 %)**. Filter layers unchanged (374 of 442, 84.6 %).

## 5. What remains (10 ANALOG layers, 68 Ladder24 filter layers)

| Stage | Layers | Notes |
|---|---|---|
| `0x124985` SUPER 2..4 | 8 | delay-line / filter-like module, PM table reads (`I9 = 0x95000`), `MRF` / `MRB` loop ([synth-stage-models-4](synth-stage-models-4.md) s3); not started this batch |
| `0x120F55`, `0x12145E` SHAPE 3 / 4 | 1 + 1 | 264 and 198 instructions; not started this batch |
| `0x12549D` Ladder24 | 68 filter layers | nonlinear 5-state module with a delay-table lookup; not started (needs the fuller frame planned for SUPER 2..4) |

The ARM writer of `DM(0xC2716)` and the note-to-pitch table `0xB069B` were not searched this batch (OQ22 stays open): the ratio and weight words of the oscillators remain ASSUMED.

## Implications

- Every ANALOG category except SUPER 2..4 and SHAPE 3 / 4 now has a stage model; MISC and SUB OSC are the first stages with persistent per-voice state besides the phases (pink rows, counter, low-pass word): the opt-in Rust keeps them in `FwStageState`.
- The MISC stages take their state from a module record (`I3 = 0xC0B88`), not from the oscillator frame; the real writer of `DM(0xC0B73)` (the row pointer) is part of the unknown installer, so the oracle pokes rows at the idle pointer.

## Follow-ups

1. SHAPE 3 / 4 (`0x120F55`, `0x12145E`), then SUPER 2..4 and `0x12549D` with a fuller frame.
2. Find the ARM writer of `DM(0xC2716)` and the table `0xB069B` (OQ22), and of the P block ratio / weight words.
3. The `FwStage` variants are wired behind `--synth-stages` only; the real ratio and weight words stay ASSUMED.

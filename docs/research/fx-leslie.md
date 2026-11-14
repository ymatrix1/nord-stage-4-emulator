# Leslie / rotary speaker: where it lives in the firmware (first pass, stop point)

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the five DSP images (`tools/ns4_dsp_images.py`, `ns4_dsp_ldr.py --sram`), the full boot log (`tools/arm_bulk_tables.py capture`, final state `pno_core1.script`), the settled ARM snapshot and `unicorn` harness (`tools/fx_clients.py`, `tools/arm_edit_capture.py`), ADI `elfdump` listing of PNO core 1 (`tools/adi_dis.py`), `sharc-fx` oracle, `capstone` listing of `bank0_CODE@0..0x279000`. Builds on [fx-reverb-leslie](fx-reverb-leslie.md) s1, [fx-type-map](fx-type-map.md) s9, [pno-module-chain](pno-module-chain.md), [b3-rotary](b3-rotary.md).
- **Question:** where are the rotor state machine, the Doppler / amplitude modulation and the horn and drum filters of the Leslie (edit ids `Edt_OrgCom_kLeslie*`, `Edt_Sys_kLeslie*`, `Fnc_kRotorSpeed`), and can the engine rotary be replaced by them?

Tags follow `.agents/rules/provenance-in-code.md`. Derived values only.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is the Leslie row really a null stub? | **No.** The image ships `0x12637B` (`NOP; RTS`) in the row `0xB2F29` / `0xB2F2B`, but the **boot log installs `0x1237F8` in both** (final state of `pno_core1.script`, words `0xB2F29`, `0xB2F2B`), with the stage contexts `I1 = 0xE359E` (word `0xB2F2A`) and `0xE31A6` (word `0xB2F2C`). `0x1237F8` is the PNO core 1 copy of the FX routine `0x122B1D` (111 instructions, the "routine B" of the Amp cabinet module `0xC0442A94`, [fx-type-map](fx-type-map.md) s10). The earlier statement that the drive row holds no code was a reading of the image, not of the booted state. Correction of [fx-type-map](fx-type-map.md) s9 and [fx-reverb-leslie](fx-reverb-leslie.md) s1 | VERIFIED (boot log, `fx_leslie_ir.py words`) |
| Where is the rotor speed? | The ARM sends **rotor speed words** into the same row region: `0xB2F25`, `0xB2F26` and `0xB2FA1` = `0x22B00`, `0x26000`, `0x15D00` in the settled state (speed 0), re-sent when `Fnc_kRotorSpeed` is 2 or 5 or `Edt_OrgCom_kEffectsLeslieSpeed` is set to 0 (value cell `0x9115E480` of FX unit 3). The writer is `0xC020D68C (obj, flag, float)`: it multiplies the float by `8388607.0` (`0x4AFFFFFE`), combines it with `11025.0` (`0x462C4400`), converts to an integer and stores `int << 8` through the DSP port (`0xC020D7DC`, `0xC020D9FC`). The words are therefore 23 bit phase increments (`0x22B`, `0x260`, `0x15D` before the shift). The operation between the two constants and the rate they refer to are not resolved (the frame rate of the DSP is 43,402.34 Hz, [dsp-frame-rate](dsp-frame-rate.md), not 44,100 Hz), so no Hz value is stated | words VERIFIED (run), scale open |
| Chain installed with it | The boot also installs two chain elements at `0xB0476` and `0xB047E` in PNO core 1: `[0x1250CF, 0x125371, 0x121BBA]` (words `0xB0479..B`) and `[0x1250CF, 0x1235AA, 0x1236F8]` (words `0xB0481..3`), state pointer `0xB52C7`. `0x1235AA` is the table LFO with the `0.9908` smoothing (the Leslie row context `0xB2F39` holds the same `0.99078`), `0x1236F8` the cabinet routine A (FIR + biquad bank), `0x125371` the nonlinear resonant filter. Whether this pair is the organ unit's Amp chain or the rotary is not decided | VERIFIED (words), role ASSUMED |
| Rotor state machine (acceleration ramps) | **Not found.** The edit ids `Edt_Sys_kLeslieHornSpeed / HornAcc / RotorSpeed / RotorAcc / Balance` (`0x91..0x96`) have value cells (defaults 1, 1, 1, 1, 2) but dispatching them sends nothing in 100 ticks; the only ARM output seen is the 3 speed words above. The speed-to-phase conversion `0xC020D68C` is called from `0xC0112588`, `0xC0112930`, six sites in `0xC02091E0..0xC02097EC` and `0xC020DC14`; the tick function that ramps with `Acc` was not reached | open |
| Static response of routine B with the booted context | **Not usable.** With the real context (`tools/fx_leslie_ir.py ir`: static image words overlaid with the boot words, FX core 2 copy of the routine) a unit impulse gives 4.6e-12 energy (left context) and 7e-7 (right) and a noise input comes out at -116 dB and -64 dB. The coefficient runs are real (402 words each: `0x967FE..0x9698F` left, `0x96634..0x967C5` right, taps of magnitude up to 0.023) but with the booted words the routine outputs nothing: the row is gated (its drive words `0xB2F2D`, `0xB2F31` are 0 at boot) or needs a state the boot does not set | VERIFIED (negative), cause open |
| Presence gap of the B3 clip | **Not measured.** No horn / drum response and no modulation depth was obtained, so the engine rotary was not replaced and `tools/emu_vs_hw.py organ_b3` was not re-run with a firmware Leslie | open |

## 2. How it was found

```bash
uv run --with unicorn python tools/arm_bulk_tables.py capture target/fw_unpacked/bank0_CODE.bin \
    target/fw_unpacked/bank0_SRAM.bin --out target/bulk          # boot log, pno_core1.script = final DM state
python tools/fx_leslie_ir.py words FX_core2_sram.bin PNO_core1_sram.bin target/bulk/pno_core1.script
python tools/fx_leslie_ir.py ir    FX_core2_sram.bin PNO_core1_sram.bin target/bulk/pno_core1.script
uv run pytest tools/tests/test_fx_leslie_ir.py
```

* The final-state words equal to a routine address of `0x120000..0x127000` list every module the ARM installs; `0xB2F29` and `0xB2F2B` are in that list (the other entries: the chain rows `0xB0479..B`, `0xB0481..3`, `0xB343B`, the slot table `0xC01D6..`).
* The context at `0xE359E` / `0xE31A6` is the cabinet context shape of [fx-type-map](fx-type-map.md) s10 (`c[-27]` buffer `0xE3200`, `c[-28]` length `0x32A`, `c[-24]` tap count `0x192`, `c[-23]` second count `0x20`, `c[-22]` / `c[-21]` PM taps `0x967FE` / `0x96970`, `c[-20]` second buffer `0xE359E`, length `0x40`). Boot-written words that differ per side: `c[-30]` = `0x3E4E7C20` (0.2016, left) against `0x3C615180` (0.01375, right) and `c[-2]` = `-726` against `-796`. A gain and a delay that differ per channel fit a two microphone model, but no recording confirms it (ASSUMED).
* A coverage diff of the ARM (block hook) over 100 ticks per edit value, from the settled snapshot, found the speed words: values `Fnc_kRotorSpeed` 2 and 5 and `LeslieSpeed` 0 produced the three PNO core 1 messages above; every other Leslie id (angles, stop mode, horn / rotor speed and acceleration, balance) produced none.

## 3. Not done (stop point)

| Item | State |
|---|---|
| Rotor state machine, speed ramps | not located; start from the callers of `0xC020D68C` and run longer than 100 ticks after a speed change |
| Why routine B is silent with the boot context | check the module chain order (which row feeds `0xE3200` and what the drive words `0xB2F2D / 0xB2F31` do in the pass), run the pass with `sharc-pass` and the boot log instead of a single routine call |
| Horn / drum magnitude responses, modulation depth, Rust port, `--leslie firmware`, `emu_vs_hw.py` re-run | blocked by the two rows above; no engine change, defaults and `render_hash` unchanged |

## Implications

- The Leslie is in a DSP image (PNO core 1), not an ARM-only mixer: the ARM sends rotor speed words and installs routines, the DSP runs a cabinet-type FIR + IIR stage with per channel gain and delay. The "no firmware counterpart" conclusion of [fx-type-map](fx-type-map.md) s9 is withdrawn.
- The presence gap of [b3-rotary](b3-rotary.md) stays unexplained by this pass.

## Follow-ups

1. Locate the ARM rotor tick (acceleration) and the DSP consumer of `0xB2F25 / 0xB2F26 / 0xB2FA1`.
2. Run the PNO core 1 pass with the full boot log and the Leslie row driven, measure the row's response with noise and sine at both speeds.
3. Then the Python model, goldens, Rust opt-in and the `organ_b3` comparison.

## 4. Second pass: the Leslie processor is `0x1256AD` (2026-11-13)

| Question | Result | Tag |
|---|---|---|
| Who calls the row `0xB2F29 / 0xB2F2B`? | **PNO core 1 routine `0x1256AD..0x126366`** (1,440 instructions; listed in [pno-dsp-map](pno-dsp-map.md) s5 and [pno-module-chain](pno-module-chain.md) s4.2 as a "parameter mapper with 65-point table interpolators"). The calls are `I8 = DM(0xB2F29)`, `I1 = DM(0xB2F2A)`, `call (m8,i8)` and the same with `0xB2F2B / 0xB2F2C` at `0x125A99..0x125AAA`. It is the rotary processor, not a mapper | VERIFIED (listing) |
| Rotor motion | Phase accumulator `DM(0xB2F23) += DM(0xB2F25)` wrapped at the modulus `DM(0xB2F21)` (`0x1256B8..0x1256D5`), a second one from `DM(0xB2F27)`; the phase is split with `fext / fdep` into table index and fraction and drives **65-point interpolated tables at stride `0x41` from `0xB4E97`** (about 24 reads per phase, results to `0xB2FCA..`, `0xB4864..`, `0xB4850`) | VERIFIED (code), table meaning EMPIRICAL |
| Signal path | input pair through `I2 = DM(0xB4CCC)` (the entry `I0`), two circular delay lines (`B4 / B6 / L6` from `0xB4858..0xB485C`, tap offset `DM(0xB2FAA)`), fraction interpolation, all-pass pairs, two cabinet routine B calls on the pairs `0xB4850` and `0xB4C7C` (horn and drum branch, ASSUMED); results in place at `I0` and `I4` | EMPIRICAL (reading) |
| State pointer | `I1 = 0xB2F2A`: the only address of `0xB2E00..0xB3500` whose words `+0xB, +0xC, +0xD` are the table pointers `0xB0276`, `0xB02F7`, `0xB0378` that the routine loads into `I4 / I6 / I5` | EMPIRICAL |
| Runs in the oracle | `tools/fx_leslie_run.py` (boot log replayed, pass context `0x1212E7`, one call per 2 frames): 1,885 steps per call, no skipped operation. With seeded noise (rms 0.17) the output rms is 0.0004 to 0.0006 per lane (about -50 dB) for drive words `0xB2F2D / 0xB2F31` of 0, 0.5 and 1.0: the level does not depend on the drive | VERIFIED (run), cause of the -50 dB open |
| Elements `0xB0476` / `0xB047E` | not run in this pass; no decision whether they are the rotary or the Amp chain | open |

Stop point of the second pass: the processor is located and runs in the oracle, but its output is about 50 dB below the input, so a gain, a gate word or the input scaling is missing. No responses, no Python model with goldens, no Rust port and no `organ_b3` run were produced. Next: bisect the level with `--trace` (which store brings the signal down: the cabinet scale `c[-30]`, the `128` output gain, or the word `0xB2F3C` read at `0x12592C`), then measure the static and the modulated response at both speeds (speed words for `Fnc_kRotorSpeed` 3 and 4 are still to be captured).

## 5. Third pass: level, rotor phase, modulation (2026-11-13)

| Question | Result | Tag |
|---|---|---|
| Where does the level go? | Trace of one call (`fx_leslie_run.py --trace`, call 2, noise): the signal pair is `(-0.144, 0.297)` at the load, `(-0.033, 0.070)` after the rotor mix (`0x125A41`), and the cabinet routine B call on `0xB4850` (`0x125A9F..0x125AAC`) returns `1.7e-05` in `R0`: **routine B is the attenuator** (its FIR taps are 0.005 to 0.02 and the per channel gain `c[-30]` is 0.2016 / 0.01375); the final output is `-4.8e-05`. The input buffers are not empty (the pair is read from `I0`), the gate word `0xB2F3C` is 0 and the drive words change nothing; the `x128` is the cabinet's own output scale. So the -50 dB is the booted cabinet data, not a missing gate. Whether the hardware makes the level up later (FX chain, master) is open | VERIFIED (trace), consequence open |
| Rotor phase | `DM(0xB2F23) += DM(0xB2F25)` per call (2 frames), clamped at the limit word `DM(0xB2F21)`. Image and boot log leave `0xB2F21 = 0`, which **freezes the rotor at angle 0** in every run so far. With `0xB2F21 = 0x7FFFFFFF` the phase advances by `0x22B00` per call (checked over 6 calls). `2^31 / 0x22B00` calls is 0.696 Hz at the frame rate 43,402.34 Hz (`0x26000`: 0.76 Hz): plausible slow rotor and horn rates, so the speed words would be `inc = f 2^31 / (fs / 2)`; the real limit word is written by the ARM when the rotor runs (not in the boot log) | VERIFIED (run), limit value and wrap ASSUMED |
| ARM speed words | `Fnc_kRotorSpeed` 0..5 and `kEffectsLeslieSpeed` 0 / 1, 400 ticks each after the dispatch from the settled snapshot: **the same three words `0xB2F25 / 0xB2F26 / 0xB2FA1 = 0x22B00 / 0x26000 / 0x15D00` at tick 140 for every value**. They are a periodic re-send of the slow values, not a function of the speed id; this corrects the attribution in s1. The fast words and the ramp are not produced by the edit dispatch in this harness | VERIFIED (negative) |
| Modulation | `tools/fx_leslie_run.py --sine HZ` with `0xB2F21 = 0x7FFFFFFF`, speed words x10, 3,000 calls, input 0.3: the envelope swings **15.3 / 16.3 dB (200 Hz), 15.3 / 17.3 (1 kHz), 14.7 / 16.6 dB (3 kHz)** (left / right lane, 40 blocks), with 3 to 4 lobes per run: an amplitude modulation of 15 to 17 dB peak to peak that does not depend on frequency. Levels are -55 to -75 dB re full scale | EMPIRICAL (one run per tone) |

Stop point of the third pass: no horn / drum split or Doppler response, no Python model with goldens, no Rust port, no `organ_b3` run. The processor is a 1,885 step routine with about 24 table reads per phase and delay lines, so a numerically verified model needs a transliteration of its 1,440 instructions (the generator of [fx-reverb-leslie](fx-reverb-leslie.md) s6 is the starting point) plus the 65-point tables from the boot log (derived summaries only); that does not fit this pass. Next: find the ARM store that holds the Leslie state machine (id `0x18D` is dispatched to unit 3 but moves nothing), the real limit word, then the transliteration.

## 6. Size of the transliteration (2026-11-13)

`tools/fx_leslie_census.py` (listing of PNO core 1, `0x1256AD..0x126366`): **1,440 instructions in 60 forms**. The five biggest are `R=R*R` (226), `R=R+R` (143), `R=dm(N)` (88), `R=dm(N,R)` (86) and the PEx / PEy swap `R<->S` (82); the rest are multifunction pairs (a multiply or add with a load or store), `clip`, `modify`, `fext` / `fdep` (12), `trunc` / `float` / `fix` (21), two `call (m8,i8)` and the `LW` long-word loads of the row pointers (about 23). That is a bounded set, so a generator of `fx_pe` steps from the listing is feasible (the reverb core of [fx-reverb-leslie](fx-reverb-leslie.md) s6 needed 230 steps of the same kinds), but it also needs the callee `0x1237F8` (111 instructions, two loops) and a trace compare over about 1,885 steps per call; it was not started in this pass.

Gain staging (open): the booted state has `kEffectsLeslieEnable` 1 and the same module words for 0 and 1 ([fx-leslie](fx-leslie.md) s1; the enable sweep of [fx-contexts](fx-contexts.md)), so the ARM data alone does not say whether the organ bypasses this unit when the rotary is off or what makes up the -50 dB; the organ bus gain K of another PR was not related to it here.

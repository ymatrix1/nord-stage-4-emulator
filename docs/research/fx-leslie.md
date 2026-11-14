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

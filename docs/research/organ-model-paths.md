# Research: which PNO core 1 routines each organ model installs (`Edt_Org_kModel` 0..5)

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66), `capstone` listing of `0..0x279000`, `unicorn` runs from the settled steady state of [arm-organ-messages](arm-organ-messages.md) s10 (`tools/arm_steady.py`, `tools/arm_edit_capture.py`), the PNO image `SRAM/core1_sram.bin` and the oracle `sharc-chain` ([organ-post-stage](organ-post-stage.md)). Derived data: [data/organ-model-paths.csv](data/organ-model-paths.csv) (addresses and small integers only).
- **Question:** Does PNO core 1 carry more than one organ generator, and which `Edt_Org_kModel` value installs which routine into which slot?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = fit or reading of data, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Model names and order | The firmware string pool at `0xC02C75F4` is `B3, Vox, Farfisa, Pipe1, Pipe2, B3 Bass` (then `<empty>`): value 0..5 of `Edt_Org_kModel`. The vibrato type ids `0x168..0x16A` (B3, Vox, Farfisa) follow the same order. | VERIFIED (strings), order = value EMPIRICAL |
| How the model reaches the DSP slots | Organ layer handler `0xC011C9BC` (id `0x166`) stores the **model as the `event`** (`+0x90`) of the layer's chain object `[store + 0x98]`, sets `+0xB4 = 1`, state `+0xB0 = 0` and queues the step `0xC022F060`. The step runs three times (state 0, 1 = generator slot, 2 = processor slot, then 3). The `kind` field `+0x8C` of [pno-module-chain](pno-module-chain.md) s3 is the *previous* event (6 at construction), not a separate selector: the slot result does not depend on it (7 kinds x 6 events, both organ layers, `ke` sweep). | VERIFIED (run) |
| **Vox = the oscillator bank with pitch LFO** | Model 1 installs `0x12272D` (generator) and **`0x1229B3`** (processor, record `I1 = 0xE2D42`) and writes the Vox vibrato word `DM(0xE20E3)` and the gain targets `DM(0x95FC6) = 1.0`, `DM(0x95FC7) = 0`. The hypothesis holds for Vox. | VERIFIED (run) |
| Pipe1 / Pipe2 = the 92-wheel bank | Models 3 and 4 install `0x122620` (bank) and `0x12268F` (mix stage) and send the bank increments (exact 12-TET) and the `a/b/wB` weights of set 0. They differ in **one word**: `DM(0x9594E)` = `0xC24E5` (Pipe1) or `0xC2655` (Pipe2), the mix-stage input array (frame 0 / frame 1 half of the bank output, [organ-oracle-audio](organ-oracle-audio.md)). The earlier name "tonewheel bank" of [organ-tonewheel-bank](organ-tonewheel-bank.md) is therefore **not B3**: it is selected by the Pipe models (footage offsets -12, 7, 0, 12, ... are shared with the Hammond ladder). | VERIFIED (selection), role of the bank for the pipes ASSUMED |
| B3 / Farfisa / B3 Bass | Generator slot stays idle (`0x121B33`); processor `0x121E2B` (B3), `0x122B85` (Farfisa), `0x122296` (B3 Bass). Farfisa also writes 30 words at `DM(0xC2C2C..0xC2D79)` (second layer `0xC2C78..0xC2DBD`). No PNO core 1 oscillator is installed for them: their tone source is not in the PNO core 1 slot table (SMP/ORG route not found: no organ id addresses any other chip). | VERIFIED (slots), source open |
| Two organ layers | Layer A uses generator slot 2/3 and processor slot 4 (record `0xB472C`, `0xB473C`, `0xB474C`, `0xE2D42`), layer B processor slot 6 (`+8`). | VERIFIED |
| Output-stage record | Four records live in the image: `i`-th has `I1 = 0xE2D42 + 7 i`, weight array `PM(0x95E46 + 0x60 i)` (96 words, **zero in the image**), parameter block `G = 0x95FC6 + 0xC i`. Layer A Vox = record 0, layer B = record 2. The ARM install and a key-down (second engine family `0xC00BF890`, `0xC00BE348`) **never write the 96 weights**; they write 79-word arrays at `0x951F1..0x9528F`, voice blocks `0x954E6`, `0x9559C`, `0xB4246`. The consumer of those is the PNO core 1 kernel (voice code at pc `0x1900..0x1AA0`), not `0x1229B3`: **who fills `PM(0x95E46..)` is open**. | VERIFIED (negative), mechanism open |
| Real record replay | With the image record (real section coefficients) and only the ARM burst the final lanes are exactly 0. With weight 1 on the oscillators of C4 E4 G4 (ASSUMED map `note - 24`) the lanes reach peak 0.153, rms 0.0613 (depth 0), peak 0.151, rms 0.0611 (Vox 3 depth). The synthetic record of [organ-post-stage](organ-post-stage.md) (identity sections, gain word `1/0.090075`) gave peak 2.78: 18x louder. | VERIFIED (run) |
| Waveshaper `T` | A **band-limited step**: integral of a Blackman-windowed sinc. Closed form in `tools/organ_shaper.py`, rms 1.7e-4 (max 3.7e-4 with fitted constants), 2.7e-4 (max 6.9e-4) with clean ones. No firmware table is needed. | EMPIRICAL (fit) |

## 2. Model to slot map (`data/organ-model-paths.csv`)

```bash
uv run --with unicorn python tools/arm_model_paths.py target/fw_unpacked/bank0_CODE.bin \
    target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --ticks 100000000 \
    --csv docs/research/data/organ-model-paths.csv
```

| Model | Name | Generator (state 1) | Processor (state 2) | Slot buffer `I1` (layer A) | Extra PNO core 1 words vs model 0 (layer A) |
|---|---|---|---|---|---|
| 0 | B3 | idle | `0x121E2B` | `0xB472C` | none |
| 1 | Vox | **`0x12272D`** (slot 2) | **`0x1229B3`** | `0xE2D42` | `0x95FC6/7`, `0xE20E3` |
| 2 | Farfisa | idle | `0x122B85` | `0xB474C` | `0xC2C2C..0xC2D79` (30 words) |
| 3 | Pipe1 | **`0x122620`** (slot 2) | **`0x12268F`** | `0xB473C` | `0x9594E` (+ bank weights) |
| 4 | Pipe2 | `0x122620` | `0x12268F` | `0xB473C` | `0x9594E` = other array |
| 5 | B3 Bass | idle | `0x122296` | `0xB472C` | none |

Generator slot is 2 when a routine is installed, 3 when idle (the setter writes both 2 and 3 with idle first). A run with 100 M instructions of main loop (the +0x120 tick delay of the queue) gives the same slot writes as stepping the chain directly (tested for all six models).

## 3. Corrections and what the sweep shows about `kind`

[pno-module-chain](pno-module-chain.md) s3 tabulated the installer by `(state, kind, event)` with hand-set fields. In the live object `kind` is the previous event and the result depends on the event only. Starting from the construction value 6, the boot-time chain objects (events 0 and 2 on the first four objects) show gens `0x121BBE` and `0x122A71` in slots 1 and 0: those belong to the other two chain objects (not the organ layers) and are the piano-side pair. The mapping above is for the organ layers' objects.

## 4. Output stage record (`0x1229B3`)

Image data (VERIFIED, `sharc-chain SRAM/core1_sram.bin --dump 0xE2D22:30`, `0xE2CC0:64`): record `i` at `DM(0xE2D22 + 7 i)` = `{state, weights ptr, G}`; `G` block of 12 words `{0.99, 0.99, P_2, P_3, 0, 0, 0, 0, P_1, 0, rec + 3, 1.0}`. Section coefficients (a0, a1, b0, b1 = P[4..7]) of the image: section 1 high-pass `0.9891, -0.9944, 0.9944, 0` (a DC blocker: `y = 0.9944 (x - x') + 0.9891 y'`); section 2 low-pass `0.9514, 0.0305, 0.0161, 0`; section 3 low-pass `0.8348, 0.1035, 0.0546, 0` (both DC gain 0.957). The ARM install overwrites only `G+0 = 1.0`, `G+1 = 0` (the model 1 burst; the image default was 0.99 / 0.99), so the output is `out_3` through all three sections.

Not found: the writer of the 96 weights. Evidence: no literal `0x95E46`, `0x95F06`, ... in the ARM code (`lit.py`-style scan of `0..0x279000`), no message to `0x95E00..0x95FFF` other than `0x95FC6/7` in the Vox install, the second-family key-down, its record update and 140 M further instructions. The consumers named above (kernel voice loop) use `PM(i12 = 0x954E6)`, `PM(i13 = 0x9559C)`: the likelier explanation is that the DSP kernel computes the weights from the ARM voice parameters. This needs the kernel pass (`pc 0x1800..0x1B00` and its caller), which `sharc-chain` does not run (it calls single routines).

## 5. Vox replay

```bash
python tools/organ_vox_replay.py SRAM/core1_sram.bin --exe target/release/sharc-chain --notes 60,64,67 --depth 0
```

`organ_vox_replay.py` pokes the ARM burst (`G+0`, `G+1`, depth) and the chord weights, runs `0x12272D` then `0x1229B3` with the real `I1`. Results in section 1. The record index, strides and the `ARM only` silence are unit-tested (`tools/tests/test_organ_vox_replay.py`).

## 6. Waveshaper table `T` (`tools/organ_shaper.py`)

`T[i + 512] = -1 + 2 (sum_(j < i) k_j + k_i / 2) / sum k`, `k_j = sinc(t / a) blackman(t / 512)`, `t = j - 512 + shift`.

| Constants | rms | max |
|---|---|---|
| fitted `a = 170.58`, `shift = -0.4638`, `alpha = 0.16` | 1.7e-4 | 3.7e-4 (the grid fit in the tool: 1.8e-4 / 4.9e-4) |
| clean `a = 512/3`, `shift = -0.5` | 2.7e-4 | 6.9e-4 |

Features it reproduces: `T(+-512) = +-1`, overshoot 1.094 at `+-171` (a third of the range, the first sinc zero spacing), small even part (-0.0053 at 0) from the half-sample shift. A windowed-sinc step is the textbook anti-aliasing of a clipper, so the oscillator is a *band-limited clipped triangle*. The residual (3e-4, symmetric) says the exact window is not Blackman with these constants; a Kaiser window (beta 7.9) is no better (4.3e-4 max). The table needs no raw data in Rust.

## 7. Rust to-do (`crates/nord-core/src/organ/`, not touched here)

1. Model enum `B3, Vox, Farfisa, Pipe1, Pipe2, B3Bass` with this value order; Vox -> `note_bank.rs` (84 oscillators + `post_lfo.rs` + sine bank + `output_stage` with the real image sections: HP 0.9891, LP 0.9514, LP 0.8348, gain `0.090075`).
2. Generate `T` with `blep_table(512/3, -0.5)` at init (no allocation in `process_block`); golden vectors from `organ_post_ref.py`.
3. The 92-wheel bank (`firmware_bank.rs`, `WeightMode::Firmware`) is the **Pipe** path (models 3/4), not B3. B3/Farfisa/B3 Bass have no PNO core 1 generator; keep the existing tonewheel/scanner model for B3 as ASSUMED until the source is found.
4. Output-stage weights: ASSUMED `note - 24`, weight 1 per sounding oscillator.

## Implications

- PNO core 1 does carry two organ generators, but they belong to **Vox** (`0x12272D` + `0x1229B3`) and **Pipe1/2** (`0x122620` + `0x12268F`), not Vox and B3.
- The B3 tone is not produced by the PNO core 1 oscillator slots.

## Follow-ups

1. Find the writer of `PM(0x95E46 + 0x60 i)`: trace the PNO kernel (pc `0x1800..0x1B00`) with a pass harness in `sharc-chain` and the voice blocks `0x954E6`, `0x9559C`.
2. Locate the B3, Farfisa and B3 Bass tone source (other DSP image, ORG strips) from a key-down under models 0, 2, 5 on all comms.
3. Check the model order against a UI test (the string pool order is the only evidence for names; the manual's model order agrees, [organ-farfisa-bass](organ-farfisa-bass.md) s1).
4. Verify the Vox record against audio once the weights are known.

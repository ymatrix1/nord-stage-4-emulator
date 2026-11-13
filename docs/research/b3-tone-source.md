# Research: where the tone of the B3, Farfisa and B3 Bass organ models comes from

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66), the booted-ARM harness (`tools/arm_boot_full.py`, `arm_steady.py`, `arm_edit_capture.py`, `arm_model_paths.py`, `arm_organ_weights.py`), the PNO image `SRAM/core1_sram.bin` and its ADI `elfdump` listing (`tools/adi_dis.py`, listing address 0 = pc `0x120200`), the oracle `sharc-chain`. Builds on [organ-model-paths](organ-model-paths.md) (PR #116), [pno-module-chain](pno-module-chain.md), [organ-weights](organ-weights.md), [arm-organ-messages](arm-organ-messages.md). Derived data: [data/b3-slot-owners-boot.csv](data/b3-slot-owners-boot.csv), [data/b3-slot-owners-fresh.csv](data/b3-slot-owners-fresh.csv) (addresses and small integers only).
- **Question:** `Edt_Org_kModel` 0 (B3), 2 (Farfisa) and 5 (B3 Bass) install an idle generator slot and only a processor (`0x121E2B`, `0x122B85`, `0x122296`) on PNO core 1. Where does their tone come from: the processors themselves, another chip (ORG, SY, SMP, FX), the ARM, or is the Rust tonewheel model as good as it gets?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = reading of data or code, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is the generator slot of B3 / Farfisa / B3 Bass really idle? | **No.** The "idle" writes (`0x121B33` into slots 2 and 3) are the *layer-private* generator slots. The firmware slot allocator deduplicates shared generators: from a fresh boot the B3 event (0) installs `0x121BBE` into **slot 1** and the event-2 chain objects install `0x122A71` into **slot 0**, both global, and later installs of the same event write only idle into slots 2/3. The settled-state sweep of PR #116 only saw the re-install. | VERIFIED (`arm_slot_owners.py boot`, `fresh`) |
| B3 tone source | **PNO core 1, generator `0x121BBE` (slot 1) + processor `0x121E2B` (slot 4 / 6)**. Replaying the pass in the oracle with the ARM's own messages (drawbars 16', 5 1/3', 8' at 8, key 60) gives nine sinusoidal partials in the generator ring at bins 27, 55, 82, 110, 165, 220, 277, 329, 439 of 4,600 calls: ratios 1 : 2.04 : 3.04 : 4.07 : 6.11 : 8.15 : 10.26 : 12.19 : 16.26, the Hammond drawbar series `0.5 1 1.5 2 3 4 5 6 8` times the 8' wheel (bin quantisation of the lowest bin is 1.8 %). Key 72 gives exactly the octave-shifted series (55 ... 879). | VERIFIED (run) |
| What the processor does | `0x121E2B` is an 80-tap dot product of the ring (`DM 0xB3300 + k`) with ARM-written per-key weight arrays `PM 0x951F1` and `PM 0x95241` (nonzero exactly at the ring partials the drawbars select, plus the percussion 2nd harmonic), then three stereo all-pass stages. It is not a self-oscillating filter. | VERIFIED (run, listing) |
| Other chips | **No.** Install, drawbar update and key-down of models 0, 2, 3, 5 address PNO core 1 only (`('PNO', 1, 2)`: 337 / 27 / 276 words for model 0, 526 for Farfisa); no message goes to ORG, SY, SMP or FX. | VERIFIED (`arm_slot_owners.py chips`) |
| B3 Bass | Same generator `0x121BBE`, processor `0x122296` (sibling of `0x121E2B`: same ring state `DM 0xB33BA..0xB33C0`, same constants). Not replayed. | EMPIRICAL |
| Farfisa | Processor `0x122B85` (84-tap FIR over the ring `DM 0xC2B52..` that the shared generator `0x122A71` writes); the Farfisa vibrato type word `DM 0xC27D7` and the fine tune word `DM 0xC27D4` are inputs of `0x122A71`. Owner of the two event-2 chain objects (chan 1, 3) not identified. Not replayed. | EMPIRICAL |

## 2. Candidates

| Candidate | Evidence | Confidence |
|---|---|---|
| (a) The processors are the generators | `0x121E2B` / `0x122296` read and write only the ring bookkeeping `DM 0xB33BA, 0xB33BC, 0xB33BE, 0xB33C0` and a ring at `0xB3300 + index` that **`0x121BBE` writes** (it stores through `0xB32E8 + 0xB33BA` and sets the same index); coefficients come from ARM arrays that the model install zeroes. Feedback gains `0.99` / `0.9977` are all-pass coefficients, not oscillator poles. With zero input they stay silent (oracle: ring and output unchanged over 3 calls with image state). | rejected, 95 % |
| (b1) Tone from ORG / SY / SMP / FX | The ARM sends nothing to those chips for the organ (section 3); boot traffic to them is the generic initial state (SMP 1,516 path 1 words, ORG 912 per core, FX 3,749). The `.ns4b` has no organ audio ([organ-tonegen](organ-tonegen.md)). | rejected, 90 % |
| (b2) ARM synthesis | No sample or PCM stream from the ARM to a DSP; all messages are parameter words. | rejected, 85 % |
| (c) PNO core 1 generator `0x121BBE`, slot 1, fed by the second ARM engine family | Section 4: run gives the nine Hammond partials, pitch tracks the key, ARM weight arrays select the drawbars. | **accepted, 90 % for B3** |
| (d) Rust "tonewheel as good as it gets" | The real signal is per key nine partials, not a 91 or 92 wheel bank: the Rust bank (`WeightMode::Firmware`) belongs to the Pipe models ([organ-model-paths](organ-model-paths.md)). The Rust model is the right idea for B3 (drawbar partials) but its structure differs. | partly |

## 3. ARM traffic of the organ models (VERIFIED)

```bash
uv run --with unicorn python tools/arm_slot_owners.py chips target/fw_unpacked/bank0_CODE.bin \
    target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --models 0,2,5,3
```

| Model | install burst | bank engine key-down | bank engine update | chips addressed |
|---|---|---|---|---|
| 0 B3 | 337 | 27 | 276 | PNO core 1 only |
| 2 Farfisa | 526 | 27 | 276 | PNO core 1 only |
| 5 B3 Bass | 337 | 27 | 276 | PNO core 1 only |
| 3 Pipe1 | 339 | 27 | 276 | PNO core 1 only |

The install burst of model 0 is the tear-down (zeroing `PM 0x951F1`, `0x95241` (79 words each), `DM 0xB42A6`, `0xB42EE` (8 each)) plus slot words; the 276-word update is the wheel weight set of [organ-weights](organ-weights.md) (`a`, `b`, `wB` for the 92-wheel bank), which is identical for all models and is consumed by the bank mix stage only when a Pipe model installs it.

## 4. Slot ownership and the replay

### 4.1 Slot owners (`arm_slot_owners.py boot`, `fresh`)

```bash
uv run --with unicorn python tools/arm_slot_owners.py boot  CODE SRAM --csv docs/research/data/b3-slot-owners-boot.csv
uv run --with unicorn python tools/arm_slot_owners.py fresh CODE SRAM --models 0,1,2,3,5 --csv docs/research/data/b3-slot-owners-fresh.csv
```

Boot (default model B3 on both organ layers): `0:0x122A71 1:0x121BBE 2:0x121B33 3:0x121B33 4:0x121E2B 5:0x122B85 6:0x121E2B 7:0x122B85`. Slot 1 is written by the organ A chain (event 0); organ B (chan 2) writes idle into its own slots 2 and 3; the event-2 chains (chan 1 and chan 3, not the organ stores) write slot 0 (`0x122A71`) and processors 5 and 7 (`0x122B85`). When organ A is re-installed from a fresh boot, **organ B** (still event 0) writes slot 1, and A writes idle into 2 and 3: the generator of event 0 is installed once for both layers. Models 1, 3 (Vox, Pipe) put their generator into the layer slot 2 and leave slot 1 untouched.

The pass calls slot k with `I8 = DM(0xC01D6 + 4k)`, `I0 = DM(+1)`, `I1 = DM(+2)` (listing `0x1212E7..0x12113D`); streams `0xC00EA`, `0xC00F2`, `0xC00FA`, `0xC0102` are shared by slots k and k + 4.

### 4.2 Replay in the oracle (`organ_b3_pass.py`)

```bash
uv run --with unicorn python tools/organ_b3_pass.py boot-messages CODE SRAM boot.jsonl
uv run --with unicorn python tools/organ_b3_pass.py keys CODE SRAM keys.json --key 60 --drawbars 888000000 --cache target/settled.snap
python tools/organ_b3_pass.py run SRAM/core1_sram.bin boot.jsonl keys.json --exe target/release/sharc-chain
```

DM state = image + all boot messages + model 0 install + `0xC00BE348` (drawbar update of record 0) + `0xC00BF890` key-down on the drawbar items (`[record + 0x28]`, 9 items, range 0..127) and the percussion items (`[record + 0x2C]`, 2 items). Then 4,800 passes of the six non-idle slot routines.

| Key, drawbars | Ring partials (bin of 4,600 calls) | Ratio to lowest |
|---|---|---|
| 60, 16' + 5 1/3' + 8' | 27, 55, 82, 110, 165, 220, 277, 329, 439 | 1, 2.04, 3.04, 4.07, 6.11, 8.15, 10.26, 12.19, 16.26 |
| 72, 8' + 4' | 55, 110, 165, 220, 329, 439, 553, 658, 879 | the same series, one octave up |

Reading (VERIFIED): the nine words `DM 0xB330C, 0xB3318, 0xB331F, 0xB3324, 0xB332B, 0xB3330, 0xB3334, 0xB3337, 0xB333C` oscillate as sinusoids (rms / max 0.69, 0.707 for a sine); the ratios are the nine Hammond footages (16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1') regardless of which drawbars are pulled. Bin 55 of 4,600 calls for MIDI 60 (261.6 Hz) matches 2 frames per call at the DSP rate of 43.16 kHz (55.8 expected, 1 bin resolution).

The weight arrays written by the ARM are nonzero exactly at the ring offsets of the selected partials: for key 60 and drawbars 16', 5 1/3', 8' array A (`PM 0x951F1`) has entries 12, 24, 31 (ring words `0xB330C`, `0xB3318`, `0xB331F` = ring offset + 24), the percussion item writes entry 36 of array B (`PM 0x95241`, `0xB3324`, the 2nd harmonic); weights are about 0.012 to 0.015 (key taper).

Not shown: the summed output of the dot product. The processor writes it through `pm(i11 - 0xE)` = `DM 0xB46A8`, which stayed 0 in the replay while the percussion path (`DM 0xB46A0..0xB46A5`, bin 110 = the 2nd harmonic, rms 0.0014) is nonzero; the cause (state of the all-pass stage, second block, or a missing ARM word) is open. So the claim is: the generator and its ARM-driven weights are verified, the final mixed sample of the processor is not.

## 5. Static data flow (`tools/sharc_routine_io.py`)

```bash
python tools/sharc_routine_io.py PNO_c1.txt 0x121BBE 0x121E2B 0x122296 0x122B85 --ends 0x121BBE=0x121E28,0x121E2B=0x122293,0x122296=0x12261D,0x122B85=0x122EF5
```

| Routine | Absolute words touched | Reading |
|---|---|---|
| `0x121BBE` | reads `0xB33B8` (fine tune), `0xB32D0..D8`; writes `0xB33BA`, voice state `0xB45AE..0xB4666`; pointers `0x9510A, 0x95172, 0x9517E, 0x951DA, 0x954E6, 0x9559C, 0xB3049, 0xB32DA, 0xB4246` | phase accumulators (circular `0xB32DA`, 12 words), table lookups with fraction, 10-item loop over the voice blocks the ARM key-down writes (`0x954E6`, `0x9559C`, `0xB4246`); writes 104 ring words per block |
| `0x121E2B` | `0xB33BA..0xB33C0`; pointers `0x95471, 0x95491, 0x9549D, 0x954D9, 0x955B0, 0xB46E6`; ring `0xB3300 + index`; record `I1 = 0xB472C` gives `I10 = 0x951F1`, `I11 = 0x9556C` | 2 x 80 taps (`lcntr 0x13` x 4 MAC + 4) against `pm(I10)` weights, then three stereo all-pass sections (`0.99`) |
| `0x122296` | the same state words and constants as `0x121E2B` | B3 Bass variant, shorter |
| `0x122B85` | `0xC27E0..E6` read; `0xC2C16`, `0xC2C18` read/write; pointer `0xC2B56`, 20 + 23 + 23 + 22 tap loops | FIR over the ring `0xC2B52..` that `0x122A71` (pointers `0xC2B52`, `0xC2BB4`, reads `0xC27D0..DE`) fills |

## 6. What would settle the rest

1. ~~The summed output of `0x121E2B` (why `DM 0xB46A8` stays 0)~~ closed in [b3-generator](b3-generator.md): `0xB46A8` is a filter state; the output is the slot stream, silent because the FIR taps `0x95471..` and the dry gain `0xB44C0` are never written by the captured ARM bursts.
2. A replay of models 5 and 2 (`0x122296`, `0x122B85`) with the same ARM burst and the Farfisa 30-word block.
3. Which chain objects are chan 1 and chan 3 (they hold event 2 permanently): organ second voices or the piano pair.
4. A slot-ownership test with both organ layers on non-B3 models (the second layer's re-install did not take effect in `fresh --layers 0,1`): does slot 1 get released when no layer uses event 0?

## Implications

- `docs/research/organ-model-paths.md` s1 and s7 ("B3/Farfisa/B3 Bass have no PNO core 1 generator", "the B3 tone is not produced by the PNO core 1 oscillator slots") are superseded: the generator is shared in slots 0 and 1.
- `crates/nord-core` organ (not touched here): B3 is a **per-key bank of nine partials** with the Hammond harmonic series `[0.5, 1, 1.5, 2, 3, 4, 5, 6, 8] x f(key)` (16' at `key - 12` semitones; one wheel per footage and key, not a 91 wheel table), weights = drawbar 7-bit curve `[0, 3, 6, 13, 22, 39, 62, 87, 127]` x key taper (`arm-organ-messages` s7) and a percussion item on the 2nd harmonic (4' of the key); the 92-wheel bank stays the Pipe model. The partials are sinusoidal in the oracle (sine-like rms/max), so a plain sine per partial is the first model; the wheel waveform of the tonewheel table `0xB3049` is the next check.
- The PNO output chain after the dot product (all-pass stages, `0x95471..` arrays) is the vibrato / chorus-like part, ASSUMED.

## Follow-ups

1. Close section 6 item 1 and compare the processor output spectrum with the Rust drawbar model.
2. Dump the table at `DM 0xB3049` (waveform of the partial oscillators) from the image and fit it.
3. Replay Farfisa and B3 Bass; identify the event-2 chain owners.
4. Record on hardware or a reference for the key taper `0x25..0xFF` per key (79 keys from index 24, [organ-weights](organ-weights.md) s7).

# B3 and Vox against the video: which stage explains the gap

**Checked:** 2026-11-13
**Sources (local, never committed):** `NEW NORD STAGE 4 - PROGRAMAS PRESETS DEMO (PART A) [_zksT69H30c].webm`, `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (programs `B3 Chords n Solo`, `Vintage Vox J3`), `ns4 inspect --json` / `ns4 convert`, `tools/emu_vs_hw.py` (new `edits`), `ns4 render-patch` (new `--b3-tail on|off`), [organ-b3-tail](organ-b3-tail.md), [organ-keydown-weights](organ-keydown-weights.md), [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md). Spec of the runs: [emu_vs_hw_b3_vox_stages.json](data/emu_vs_hw_b3_vox_stages.json). No audio is committed.
**Question:** the oracle-faithful B3 (`b3wheels` with the processor tail) is 11.9 dB under the recording at 0.4 to 1.5 kHz with a crest of 23 dB (hardware 17.7 dB), and Vox is 10 dB over at the same band. Is a stage after the DSP organ output (Rotary, Amp, EQ, compressor, delay, reverb, level) missing or wrong?

Hardware audio is a lossy live recording; every number is shape and level evidence only. Bands are dB re RMS, emulator minus hardware; 8 to 20 kHz is not scored.

## 1. Answer in short

| Segment | Result | Tag |
|---|---|---|
| B3: effect state of the program | rotary on (slow, drive 2.44 of 10), vibrato on (type 5) on both layers, reverb 0.22 (type 0); **no amp, EQ, compressor, delay or modulation**; layers A (drawbars `8 0 8 0 2 0 0 0 0`, octave +2) and B (`8 8 8 6 3 3 1 5 0`, octave 0) both on at level 0.795 | VERIFIED (program decode) |
| B3: the missing 0.4 to 1.5 kHz is the **processor tail**, not a post stage | `b3wheels` without the tail: mid +0.4 dB; with the tail: -11.9 dB. Reverb off, firmware FX, vibrato off, layer B off move the mid band by at most 0.3 dB (reverb, firmware FX) to 4.7 dB (layer B, a level change). The tail is a measured oracle response with the constructor's tables (vibrato not programmed); the recorded program plays with vibrato type 5 on, which the ARM programs through the line box (hypothesis, not captured) | VERIFIED (ablation), cause of the difference ASSUMED |
| B3: crest and level are the **rotary stage** | rotary off: RMS -18.8 dB (on: -2.6 dB), peak -8.2 dBFS (on: 0.0), crest 31.2 dB (on: 23.1). `rotary.rs` applies `tanh((1 + 3 drive) x)` with drive 2.44, a fixed gain of 8.3 (+18.4 dB) that was calibrated for the `Legacy` bus scale (ASSUMED) and acts as a plain gain on the 0.01-level weights; the saturating `tanh` is what clips (0.021 % of samples). Not changed here (default render) | VERIFIED (ablation), model ASSUMED |
| B3: firmware FX | no effect (mid -12.0 against -11.9 dB): the program enables only the reverb, whose firmware mix law changes the clip by less than 0.3 dB; amp drive, tone stack, compressor, delay are off in this program | VERIFIED |
| Vox: effect state | Twin amp (type 289, drive 1.10), EQ bass -3 dB, delay (ping-pong, mix 0.157, feedback 0.488), reverb 0.244, modulation 2 on (type 6), no rotary, vibrato off, layer A drawbars `8 8 8 4 2 2 0 3 4` | VERIFIED (program decode) |
| Vox: mid +10 dB is **in the bank**, not in a post stage | all post stages off (`vox_dry`): mid +11.1 dB; the full chain: +9.9 dB. The amp adds presence and brilliance (-7.7 to -2.3 dB, -7.7 to -2.5 dB, score 5.17 to 3.44); delay, reverb, modulation and EQ move the mid band by 0.2 to 0.7 dB. The Vox bank has too little energy above 1.5 kHz and too much at 0.4 to 1.5 kHz (its output sections low-pass at about 340 Hz and 1.1 kHz) | VERIFIED (ablation) |
| Vox: level and crest | bank + chain RMS -6.5 dB, peak -6.8 dBFS, crest 26 dB against 15.2 dB: the key-down click and the 2 ms weight steps dominate the peak; the Legacy path is +11 dB, crest 12.6 dB | VERIFIED (run) |

## 2. B3 stage ablation (`bw_` = `ToneWheelSource::B3Wheels`, confirmed notes, 25 s)

| Variant | sub | low | mid 0.4-1.5k | pres | brill | centroid | RMS | score | peak dBFS / clip / crest |
|---|---|---|---|---|---|---|---|---|---|
| default (Legacy) | +17.5 | -0.2 | +0.3 | -5.7 | +0.5 | -208 | +14.9 | 4.83 | 0.0 / 10.3 % / 5.6 |
| bw_notail (weights only) | +13.9 | -0.2 | +0.4 | +1.3 | +8.0 | -78 | +3.1 | 4.76 | 0.0 / 0.022 % / 17.5 |
| **bw_tail** | +16.8 | -3.0 | **-11.9** | +1.2 | +7.4 | -91 | -2.6 | 8.07 | 0.0 / 0.021 % / 23.1 |
| bw_tail, rotary off | +15.7 | -4.2 | -8.9 | +6.3 | +11.7 | +498 | -18.8 | 9.36 | -8.2 / 0 / 31.2 |
| bw_tail, reverb off | +16.7 | -3.1 | -11.9 | +0.8 | +7.2 | -102 | -2.2 | 7.95 | 0.0 / 0.027 % / 22.8 |
| bw_tail, vibrato off | +16.2 | -2.3 | -11.6 | +2.1 | +9.2 | -36 | -0.9 | 8.27 | 0.0 / 0.057 % / 21.4 |
| bw_tail, layer B off | -1.5 | -3.8 | -7.2 | +2.3 | +8.8 | +225 | -9.6 | 4.71 | -1.5 / 0 / 28.6 |
| bw_tail, rotary + reverb + vibrato off | +14.4 | -4.2 | -8.7 | +8.2 | +13.2 | +822 | -16.0 | 9.74 | -6.8 / 0 / 29.7 |
| bw_notail, same three off | +12.8 | -0.3 | +0.1 | +4.7 | +12.2 | +120 | -11.1 | 6.03 | -6.3 / 0 / 25.4 |
| Legacy, same three off | +17.3 | +0.5 | -0.4 | -7.0 | +1.9 | -212 | +5.1 | 5.40 | 0.0 / 0.010 % / 15.4 |
| bw_tail + `--firmware-fx` | +16.7 | -3.1 | -12.0 | +0.9 | +7.2 | -103 | -2.3 | 7.97 | 0.0 / 0.027 % / 22.8 |
| hardware | | | | | | | | | -2.9 / 0 / 17.7 |

Reading:

- **Mid band:** the only switch that moves it by more than 5 dB is the tail (12.3 dB). The post chain of this program (rotary, vibrato, reverb) cannot produce -12 dB there.
- **Layer B** carries the sub: without it the sub band falls from +16.8 to -1.5 dB. Layer B has the 16' bar at 8 and no octave shift, layer A is two octaves up; whether the recorded layer split (the program's split zone) sends the low notes to B on the hardware is not established, and the clip's detected notes include low ghost notes ([emulator-vs-hardware-audio](emulator-vs-hardware-audio.md) caveats).
- **Rotary:** +16.2 dB of gain and the clip. The brightness the rotary adds (presence +6.3 to +1.2, brilliance +11.7 to +7.4 dB: it lowers the bands re RMS) is the Doppler and horn path, correct in direction.
- **Level staging:** no variant has the hardware crest (17.7 dB) at the hardware RMS; `bw_notail` with rotary does (17.5 dB, RMS +3.1 dB) by the coincidence of the saturating drive.

## 3. Vox stage ablation (confirmed notes, 11 s)

| Variant | sub | low | mid | pres | brill | centroid | RMS | score | peak / crest |
|---|---|---|---|---|---|---|---|---|---|
| default (Legacy) | -4.1 | +0.4 | +13.4 | -4.7 | -5.8 | -606 | +11.1 | 5.68 | -3.1 / 12.6 |
| **vox** (full chain) | -0.9 | +1.5 | +9.9 | -2.3 | -2.5 | -308 | -6.5 | 3.44 | -6.8 / 26.4 |
| vox, amp off | +1.6 | +1.6 | +10.6 | -4.0 | -7.7 | -797 | -8.7 | 5.10 | -12.6 / 22.9 |
| vox, delay off | -0.9 | +1.7 | +10.3 | -2.5 | -2.7 | -352 | -6.4 | 3.61 | -6.9 / 26.3 |
| vox, reverb off | -1.0 | +1.5 | +9.9 | -2.2 | -2.4 | -289 | -6.1 | 3.41 | -6.4 / 26.5 |
| vox, modulation 2 off | -0.8 | +1.5 | +10.0 | -2.2 | -2.5 | -305 | -5.1 | 3.42 | -5.5 / 26.4 |
| vox, EQ bass off | +0.4 | +1.4 | +9.6 | -2.6 | -2.9 | -395 | -6.1 | 3.39 | -6.9 / 26.0 |
| vox, all five off (`vox_dry`) | +1.4 | +1.8 | +11.1 | -4.0 | -7.7 | -786 | -7.0 | 5.17 | -10.6 / 23.1 |
| Legacy, all five off | -2.5 | -0.1 | +14.3 | -6.6 | -11.4 | -796 | +11.5 | 6.97 | -1.1 / 14.2 |
| vox + `--firmware-fx` | -0.7 | +1.5 | +10.1 | -2.4 | -2.5 | -317 | -6.1 | 3.44 | -6.5 / 26.4 |
| hardware | | | | | | | | | -11.6 / 15.2 |

Reading: the Twin amp is the one post stage that matters (it lifts presence by 1.7 dB and brilliance by 5.2 dB and costs 2.2 dB of RMS: score 5.10 to 3.44); the delay, reverb, modulation and EQ are within 0.7 dB of the mid band. The remaining mid excess (+10 dB) and the missing low sub/presence are the bank's own spectrum. The firmware FX switch changes nothing again (the Twin amp type 289 is not one of the ported types; `FirmwareFx::ALL` has no amp-type map, [fx-type-map](fx-type-map.md)).

## 4. Wiring found

| Finding | State |
|---|---|
| `render-patch` has no way to ablate a stage | added: `emu_vs_hw.py` variants take `edits` (dotted JSON paths applied through `ns4 convert`, passed as `--program-file`) and `ns4 render-patch --b3-tail on|off`; defaults unchanged (`render_hash` 25baa387... identical) |
| The Rotary stage is a fixed-gain `tanh` drive | ASSUMED model, +18.4 dB at the program's drive; calibrated for the Legacy bus scale and the reason a firmware-scale source clips or sits 16 dB lower without it. A level stage between the DSP organ and the rotary (the organ-level output stage of the brief) is not captured; changing it moves the default render, so it is documented, not changed |
| `--firmware-fx` on these programs | no wiring bug: B3 enables only reverb, Vox's amp type 289 has no firmware port |
| The oracle B3 chain used for the tail measurement had the constructor's default tables (vibrato off); the program plays vibrato type 5 | the likely cause of the 12 dB; unproven |

## Implications

- Do not tune any post stage to close the B3 gap: the mid deficit is the tail table, which is valid for the unprogrammed line box. The next B3 step is to capture the line box with the program's vibrato type on (the `Edt_Org_Preset_kVibratoEnable` handler is not reached by the ARM harness, [arm-organ-messages](arm-organ-messages.md)) or to run the oracle with the vibrato tables of the chorus type and re-measure `G(f)`.
- A calibrated organ-level / rotary-input stage is needed before B3Wheels and Vox can share the Legacy path's output level (rotary +16 dB at drive 2.44).
- Vox: the bank spectrum above 1.5 kHz (and the amp's contribution) is the lever, not delay, reverb or modulation; the click-dominated crest (26 dB) points at the 2 ms weight ramps and the key click.

## Follow-ups

1. ARM capture of the vibrato / chorus enable (`Edt_Org_Preset_kVibratoEnable`) and the oracle tail with the type-5 tables.
2. Organ output level: ARM/DSP stage between the organ chain and the master (the `L` of OQ17 and the layer level).
3. Vox bank brightness: compare the oracle's output sections and the Rust bank on a spectrum above 1.5 kHz; amp types 289 and others in the FX port.

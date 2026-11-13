# B3 and Vox key-down weights in Rust, against the oracle chord

**Checked:** 2026-11-13
**Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin`, `bank0_SRAM.bin` (OS v1.66), the PNO image `SRAM/core1_sram.bin`, `sharc-pass` (`crates/nord-sharc`), `tools/organ_fb_pass.py script --model 0 --keys 60,64,67` (ARM engine constructor, model install and key-down of the chord, 23,255 words), `tools/pno_full_pass.py`, the replay scripts and tables of [keydown-zones](keydown-zones.md) ([keydown-b3-wheels.csv](data/keydown-b3-wheels.csv), [keydown-organ-keys.csv](data/keydown-organ-keys.csv), [keydown-vox-flags.csv](data/keydown-vox-flags.csv), `keydown-{b3,vox}-c4e4g4.script`), [organ-farfisa-bass](organ-farfisa-bass.md) (dry gain), [dsp-frame-rate](dsp-frame-rate.md). New derived table: [b3-key-weights.csv](data/b3-key-weights.csv) (`tools/organ_b3_keys.py derive`). Comparison tool: `tools/organ_partials.py`. Only derived values are recorded.
**Question:** with the captured key-down data, how close do the Rust B3 (`B3Wheels`) and Vox (`Vox`) paths come to the oracle chord C4 E4 G4 per partial, and can either become the `ModelDefault` of its model (D22)?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Answer in short

| Item | Result | Tag |
|---|---|---|
| B3 amplitude `A` per wheel | The first write of every captured wheel is the existing `B3_WHEEL_GAIN` entry: 44 wheels are now captured values (the old fit was off by up to 0.14 at wheels 12, 16, 19, 21, 23, 56, 57, 59, 79, 84, 85) | VERIFIED (44 wheels), EMPIRICAL elsewhere |
| B3 footage weight `W(key, footage)` | the four items of every key-down (5 1/3', 8', 4', 16') are split out of the cumulative write words: 24 keys x 4 weights ([b3-key-weights.csv](data/b3-key-weights.csv)); `W` depends on the key and the footage, not on the velocity; keys 21..36 and 96..108 saturate; the other five footages were not in the capture (the 4' column is reused) | VERIFIED at the 24 keys, EMPIRICAL between them, ASSUMED for footages 2 2/3' and up |
| Partial amplitude | `A[wheel] * W(key, footage) * d7 / 127` times the wheel table, summed over keys; the partial `A W` of C4 8' is 0.294 x 0.0329 = 0.0097 | VERIFIED for the factors, see 2 for the sum |
| B3 against the oracle | **not within 1 dB**: the Rust dry sum is 12.7 dB (130 Hz) to 31.8 dB (587 Hz) above the oracle chord. The processor adds a filtered branch that the Rust path lacks (section 2) | VERIFIED (run) |
| Vox weights | `row = fold(key - 36 + I[slot])`, the default program's four level slots (16', 8', 4', 2') at 0.2475 each, no velocity: replaces "1.0 x velocity on the 8' only" | VERIFIED (24 keys x 20 slots, 24 keys x 4 rows) |
| Vox against the oracle | isolated lines within 0.2 dB, absolute (no normalisation); two frequencies of the G4 chain (rows 43 and 55) are 3.4 and 2.1 dB low in the chord only where C4 harmonics coincide with them (section 3) | VERIFIED (run) |
| Defaults | **both stay opt-in** (D22): B3 fails the 1 dB criterion by 12 to 32 dB, Vox by 3.4 dB on two phase-dependent lines and the 36-sine bank (rows 84..95) and the footage edits are not ported | decision |

## 2. B3 chord against the oracle

Oracle: ARM constructor + model 0 install + C4 E4 G4 key-down (23,255 words at pass 0) + `DM(0xB44C0) = 1.0`, 12,000 passes, set 2 pair 0 lane L, frames 3,000..11,192. Rust: `organ_ref --mode b3 --drawbars 888000000 --rate 43402.34375`, same window; each line is the amplitude at the line frequency of the signal's own tuning (ARM fine word for the oracle, `note_hz` for Rust, up to 1.3 cent apart), Hann window, no normalisation.

```bash
python tools/organ_fb_pass.py script CODE SRAM fb_m0.script --model 0 --keys 60,64,67 --cache settled.snap
echo "0 0xb44c0 0x3f800000" > dry_gain.script
python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass \
    --script fb_m0.script --script dry_gain.script --passes 12000 --out b3
cargo run --release -p nord-core --example organ_ref -- --mode b3 --drawbars 888000000 \
    --rate 43402.34375 --frames 12000 --notes 60,64,67 --out rust_b3
python tools/organ_partials.py b3 b3.f32.npy rust_b3.csv --keys 60,64,67 --drawbars 888000000 --skip 3000
```

| Wheel note | Hz (ARM tuning) | Oracle | Rust | Error (dB) |
|---|---|---|---|---|
| 48 (C3, C4 16') | 130.8 | 0.00273 | 0.01172 | +12.7 |
| 52 (E3) | 164.8 | 0.00213 | 0.01155 | +14.7 |
| 55 (G3) | 196.0 | 0.00164 | 0.01066 | +16.2 |
| 60 (C4 8') | 261.7 | 0.00100 | 0.00952 | +19.6 |
| 64 (E4 8') | 329.6 | 0.00075 | 0.00994 | +22.5 |
| 67 (C4 5 1/3' + G4 8') | 392.0 | 0.00114 | 0.02399 | +26.5 |
| 71 (E4 5 1/3') | 493.9 | 0.00045 | 0.01220 | +28.6 |
| 74 (G4 5 1/3') | 587.4 | 0.00032 | 0.01245 | +31.8 |

Findings:

- **The oracle has no 4' partial** in the chord (wheels 72, 76, 79 are below 1e-5): the fourth write of the key-down is not a drawbar item in the default program (probably the percussion item, [b3-generator](b3-generator.md) section 5). The Rust path with `888000000` has none either.
- **The amplitudes follow `A W` only at the factor level**: C4 8' alone should be 0.0097 (the Rust line), the oracle line is 0.0010. The loss grows with frequency (about -9 dB per octave from 130 to 590 Hz, with ripple: wheel 67 and wheel 74 do not sit on the trend), so the processor tail is a frequency-dependent filter, not a gain.
- **The output is not proportional to `DM(0xB44C0)`**: with the same messages and `0xB44C0 = 2.0` the peak is 0.0155 instead of 0.0095 (ratio 1.63, not 2), so a second branch of comparable size reaches the output. That is the filtered branch of the processor tail: FIR 1 (33 taps `DM 0x95471..`), FIR 2 (61 + 14 taps), both programmed by the engine constructor `0xC002187C` ([b3-generator](b3-generator.md) section 4, [organ-farfisa-bass](organ-farfisa-bass.md) section 2). Without the constructor messages the pass is silent at any dry gain (run: 40,000 frames of exact zeros with `0xB44C0 = 1.0` and the model install only).
- The dry gain `DM(0xB44C0)` stays ASSUMED at 1.0: the ARM writer was not found (OQ16), and the ratio above shows that the Rust dry path alone cannot be calibrated against it.

Verdict: `B3Wheels` is not made the B3 default. What is verified is the per-wheel factors (`A`, `W`, the fold into wheels 12..88, no velocity). What is missing is the processor tail: the 108 taps from the constructor (firmware data) and their ARM writer.

## 3. Vox chord against the oracle

Oracle: `pno-pass-install-vox.script` + the 12 weight writes of `keydown-vox-c4e4g4.script` (key-down) at pass 0, 12,000 passes, lane L; Rust: `organ_ref --mode vox --rate 43402.34375 --frames 12000`, rows 24, 28, 31, 36, 40, 43, 48, 52, 55, 60, 64, 67, harmonics 1..4, 39 lines above 1e-4 (the chord's largest line is 0.0163).

```bash
python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass \
    --script docs/research/data/pno-pass-install-vox.script --script vox_keyon.script --passes 12000 --out vox
cargo run --release -p nord-core --example organ_ref -- --mode vox --rate 43402.34375 --frames 12000 --out rust_vox
python tools/organ_partials.py vox vox.f32.npy rust_vox.csv --skip 3000
```

| Quantity | Result |
|---|---|
| Absolute level, fundamentals of rows 24, 28, 31 | +0.02 dB (0.01631 / 0.01630 / 0.01503 oracle) |
| Lines above 1e-4 within 1 dB | 31 of 39 |
| Lines outside 1 dB | rows 43 (-3.4 dB: 24 h3, 31 h2, 43 h1 are one frequency) and 55 (-2.1 dB: 31 h4, 36 h3, 43 h2, 55 h1), and the 1.5 dB of 55 h3: all lines of two coincident frequencies |
| G4 alone (rows 31, 43, 55, 67, oracle with only G4's four weights) | all lines within 0.14 dB (max 0.14, absolute) |
| Old map (weight 1.0 on the 8' only, `pno-pass-vox-weights-c4e4g4.script`) | fundamentals 0.00 dB (unchanged result of [organ-post-stage](organ-post-stage.md) section 8) |

The two deviating frequencies are exactly those where a harmonic of the C4 chain lands on a G4 fundamental (24 + 19 = 43, 36 + 19 = 55 in 12-TET). A single key matches, so the weights, the map and the level are right; the sum of two coincident lines depends on the relative start phases of the free-running oscillators (Rust starts all at 0, the oracle starts from the settled image state, `vox_source` doc, ASSUMED), so the chord check cannot pass 1 dB on those two lines until the start phases are read from the image (follow-up). Rows 84..95 (the 36-sine bank) get no oscillator: keys above note 96 lose the 8' to 2' footages that fold there (note 108 keeps only its 16').

## 4. Hardware comparison (`tools/emu_vs_hw.py --only organ_b3 organ_vox --confirmed-only`)

Same clips, notes and method as [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md) (video-confirmed notes, velocity 100, emulator minus hardware, bands in dB re RMS; 8..20 kHz not scored). The `default` rows reproduce the earlier run exactly; the opt-in rows are the ones this change moves.

| Segment | Variant | sub 20-120 | low 120-400 | mid 0.4-1.5k | pres 1.5-3k | brill 3-8k | centroid | RMS | width hw/emu | score |
|---|---|---|---|---|---|---|---|---|---|---|
| organ_b3 | default (Legacy) | +17.5 | -0.2 | +0.3 | -5.7 | +0.5 | -208 | +14.9 | 0.48/0.25 | 4.83 |
| organ_b3 | b3wheels before | +16.9 | -0.0 | +0.3 | -5.1 | +1.1 | -202 | +14.9 | 0.48/0.25 | 4.69 |
| organ_b3 | **b3wheels after** | +13.9 | -0.2 | +0.4 | +1.3 | +8.0 | -78 | +3.1 | 0.48/0.44 | 4.76 |
| organ_vox | default (Legacy) | -4.1 | +0.4 | +13.4 | -4.7 | -5.8 | -606 | +11.1 | 0.50/0.18 | 5.68 |
| organ_vox | vox before | -8.0 | +2.5 | +11.3 | -8.2 | -8.3 | -921 | -0.8 | 0.50/0.17 | 7.66 |
| organ_vox | **vox after** | -0.9 | +1.5 | +9.9 | -2.3 | -2.5 | -308 | -6.5 | 0.50/0.19 | 3.44 |

Level and clipping (peak dBFS, share of samples at full scale, crest, from the rendered WAVs): B3 default 0.0 / 10.3 % / 5.6 dB, **b3wheels after 0.0 / 0.022 % / 17.5 dB** (hardware -2.9 / 0 / 17.7); Vox default -3.1 / 0 / 12.6 dB, vox before about -3 dB, **vox after -6.8 / 0 / 26.4 dB** (hardware -11.6 / 0 / 15.2).

- B3 wheels: RMS gap +14.9 to +3.1 dB, clipping gone, crest matches the hardware (17.5 against 17.7 dB), centroid 130 Hz closer, presence 6.4 dB closer; the brilliance band is now +8.0 dB (was +1.1): the top wheels carry the full `A W` while the missing tail (section 2) would roll them off, which fits the oracle finding. Score 4.83 to 4.76 (the sub +13.9 dB is unchanged in kind: low notes of the performance).
- Vox: score 7.66 to 3.44 (default 5.68), mid 11.3 to 9.9 dB, presence and brilliance -8 to -2.4 dB, centroid -921 to -308 Hz, width unchanged. The mid bump of +10 dB is still there (the amp stage is not part of this source); the RMS is now 6.5 dB under the hardware (the crest of 26 dB says the key-down click dominates the peak).
- No default changes (section 1): the hardware numbers support the new weights (the Vox score is now the best of its three variants) but the oracle criterion is not met.

## Implications

- Per-wheel B3 amplitudes and per-key weights are now data, not fits; the Rust B3Wheels path uses them (`b3_keys.rs`, `b3_weights.rs`) and loses the legacy `0.08` bus scale (about 18 dB lower, ending the clipping of the chord).
- The B3 processor tail (108 constructor taps) is the one open piece for a verified B3 default; a captured response of that chain (probe a single wheel through the oracle, read the transfer function) would be a derived table.
- Vox needs the start phases of its 84 oscillators and a model of the 36-sine bank (rows 84..95) before the chord check can be closed.

## Follow-ups

1. B3: oracle transfer function of the tail per wheel (single wheel, constructor loaded) as a derived table; ARM writer of `DM(0xB44C0)`.
2. B3: key-down capture at drawbars `888888888` for the five footages missing from `W` and a sweep of the drawbar edit ids.
3. Vox: read the oscillator start phases from the image and re-run the chord (rows 43 and 55); port the sine bank rows 84..95.

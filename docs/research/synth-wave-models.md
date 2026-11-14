# Synth SHAPE oscillator: the bit-exact model of stage 0x1212D3

- **Checked:** 2026-11-13
- **Sources (local, never committed):** ORG core 2 image (`tools/ns4_dsp_ldr.py --sram`), `sharc-org`, `sharc-disasm`; `tools/synth_wave_oracle.py`, `tools/synth_wave_model.py`, `tools/synth_wave_golden.py`, `ns4 render-patch --synth-chain strip --synth-shape`, `tools/emu_vs_hw.py`. Builds on [synth-wave-stages](synth-wave-stages.md) (type 7 = stage `0x1212D3` = ANALOG cat 3 wave 0, Smooth Lead), [synth-voice-control](synth-voice-control.md) s5 (saw helper).
- **Question:** What does stage `0x1212D3` (and its helper `0x1251D4`) compute, can it be modelled word for word, and does it close the Smooth Lead presence gap?

Tags follow `.agents/rules/provenance-in-code.md`. Only derived words and formulas are recorded.

## 1. Short answer

| Question | Answer | Tag |
|---|---|---|
| Frame of the stage | stage-A call for channel 0: `I0 = 0xC1E02`, `I2 = 0xC1F96`, `I4 = 0xC1E8A` (scratch), parameter block `P = DM(0xC1F76) = 0xC169A`; SIMD: lane x and lane y run the same code on their own words (lane y one address above) and the mix swaps registers between the lanes (`R8 = S8`, ...). The pass rewrites the pitch word `DM(-29, I2)` and the shape words `DM(17, I2)`, `DM(19, I2)` before the stage runs; phases (`-21`, `-19`), the `P` words and everything else survive a poke | VERIFIED (run) |
| Model | `tools/synth_wave_model.py`: **320 of 320 output words bit equal** to the oracle (40 random trials: random phases, ratios 2^-2 .. 2^5.3 (increments up to the fade), mix weights, output gain; both lanes, 4 samples each), phases equal too. It needs 40-bit register arithmetic (`RND32 = 0`: 32-bit significand, nearest even, a memory store truncates the 8 extension bits); a float32 model is 1 to 8 ulp off | VERIFIED (oracle) |
| What the stage is | two oscillators per lane; each is the **difference of two band-limited parabolas** (helper `0x1251D4`), the second shifted by `(1 - W) / 2` cycles; `W = shape (1 - min(1, 1.5 a))`; the two oscillators and the two lanes are mixed with the `P` weights. At shape 0.5 one oscillator is triangle-like (H2 -9.8 dB, H3 -22 dB at 987 Hz), towards shape 0 it moves to a saw-like wave | VERIFIED (formulas); the shape and weight words of a real program: ASSUMED |
| Rust | `synth/fw_shape.rs` (one oscillator, ratio 1, weight 1) wired into the strip chain behind `SynthLayer::strip_shape` (default `None` = the old saw placeholder; `render-patch --synth-shape <0..1>`); against the Python model on 384 golden points the worst deviation is 1.9e-6 (f32 against the exact model) | VERIFIED (test) |
| Smooth Lead on the video | no shape value closes the gap (section 4); the strip chain stays opt-in | EMPIRICAL |

## 2. Equations (per lane, all words from `DM`, 40-bit registers)

```text
inc1 = FIX(f r1 2^31), inc2 = FIX(f r2 2^31)            f = DM(-29, I2), r = P[-32], P[-30]
a    = FLOAT(inc1) 2^-31 (= 2 i, i = cycles per SY frame); stored a = trunc32(a)
g    = C (B + clip(A - a, +-B))                          A, B, C of the saw helper (0x3EB0F34D, 0x3DEBEF11, 0x408AE2EA)
s    = clip(DM(17, I2) + DM(19, I2), 1);  W = s (1 - min(1, 1.5 a_stored))
D    = 1 - W^2;  r7 = 0.5 / a;  inv_d = 1 / D            two Newton steps from the RECIPS seed (bit exact)
off  = 2 FIX((W - 1) 2^30)                               shift of the second parabola, Q32 cycles
helper(x_k; R6, R7):  xf = x 2^-31,  xo = (x xor 2^31) 2^-31,  w = 1 - min(1, |xf r7|)
                      y_k = xo^2 - (4/3) R6 w^3;  R6 <- (4/3) R6 per call (the second call sees 16/9 a)
osc  = first(ph + k inc) - second(ph + k inc - off)      k = 1 .. 4, phase += 4 inc
mix1 = partner(osc1 wA1) + own(osc1 wB1)                 wA1 = P[-14], wB1 = P[-16]
mix2 = (own(osc2 wA2) + mix1 stored) + partner(osc2 wB2) wA2 = P[-12], wB2 = P[-10]
out  = mix2 stored * P[16] g inv_d  -> DM(-13, -11, -9, -7 (+1 for lane y), I2)
```

## 3. Oracle comparison

`python tools/synth_wave_oracle.py --image SRAM_ORG/core2_sram.bin` runs one pass per trial with the stage in slot 0 and prints `320 of 320 output words equal`. Ten recorded trials (words only) are in [data/synth-shape-oracle-cases.json](data/synth-shape-oracle-cases.json) and checked by `tools/tests/test_synth_wave_model.py` without the firmware. The Rust golden grid (`synth/tests_fw_shape_golden.rs`, 384 points) is generated from the model by `tools/synth_wave_golden.py`.

## 4. Smooth Lead against the video (`--synth-chain strip`, `tools/emu_vs_hw.py --only synth_lead --confirmed-only`)

| Variant | sub | low | mid | presence 1.5-3k | brilliance | RMS dB | centroid Hz | score |
|---|---|---|---|---|---|---|---|---|
| strip, saw placeholder (before) | -0.9 | -1.2 | +1.5 | **-9.0** | -7.6 | **+3.8** | -113 | 4.05 |
| strip, SHAPE stage, shape 0.5 (idle word) | +0.1 | -0.4 | +1.2 | -12.3 | -6.0 | +4.2 | -190 | 4.00 |
| strip, SHAPE stage, shape 0.3 | +0.4 | -0.1 | +1.1 | -14.0 | -6.4 | +4.2 | -207 | 4.41 |
| strip, SHAPE stage, shape 0.15 | +0.5 | -0.1 | +1.0 | -14.9 | -6.2 | +4.2 | -214 | 4.52 |
| strip, SHAPE stage, shape 0.05 | +0.5 | -0.0 | +1.0 | -15.2 | -6.0 | +4.2 | -215 | 4.55 |

The real stage is less bright than the saw placeholder when it runs as one oscillator with ratio 1 and weight 1. The hardware lead also has the second oscillator (ratios `P[-32]`, `P[-30]`: detune or interval), the mix weights and the knob-driven shape word, none of which the ARM sends in the idle harness state, plus the cutoff scale question of [synth-strip-path](synth-strip-path.md) s4. The shape, ratio and weight words of a loaded program are the missing input.

## 5. D22 decision

Stay opt-in. Oracle exact: met for this stage (section 3) and for the envelope and filter words ([synth-strip-path](synth-strip-path.md) s3). Video presence within about 3 dB: **failed** (-9.0 dB with the placeholder, -12.3 dB with the stage at the idle shape). RMS within about 3 dB: **failed** (+3.8 / +4.2 dB). `render_hash` unchanged.

## Implications

- The SHAPE category is a morphing parabola-difference oscillator (a triangle to saw-like family), not a triangle and not the saw helper: the decoder placeholder is wrong in kind. The stage, bit exact, is available (`SynthLayer::strip_shape`).
- The remaining unknowns are program words, not code: `P[-32]`, `P[-30]` (two oscillator ratios), `P[-14 .. -10]`, `P[16]`, the shape words `DM(17, I2)`, `DM(19, I2)` and the pitch word scale (`f = 2 i`, ASSUMED to follow the note).

## Follow-ups

1. Capture the program words the ARM computes for Smooth Lead (needs a loaded program in the ARM harness) and set ratios, weights and shape.
2. Model the other stages of [synth-wave-stages](synth-wave-stages.md) the same way (SAW `0x1215FB`: two saw helper calls; SQUARE / PULSE `0x121B99`; SUB; SYNC); the 40-bit register model and the oracle frame are reusable.
3. Confirm that the preset category / wave fields equal the edit-id values on a decoded program with a known wave (not done here; still ASSUMED).

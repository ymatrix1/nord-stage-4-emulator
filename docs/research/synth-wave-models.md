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

## 6. More stages (same skeleton, same 40-bit model)

The oscillator stages share one skeleton: pitch and two ratios give two increments, a helper produces four samples per oscillator, the two lanes and two oscillators are mixed with the `P` weights and scaled by `P[16]`. `mix_lanes` in `tools/synth_wave_model.py` is that common part.

| Stage | ANALOG waves (names from the ns4decode map, category / wave) | Helper | Oracle result |
|---|---|---|---|
| `0x1215FB` | PURE (0, 2) saw: the most common wave of the factory programs (108 layers) | `0x12501F`: falling band-limited saw `-amp g (xo - copysign(w^2, xo))`, `w = 1 - min(1, \|xf\| 0.5 / a)` | `stage_saw`: 320 of 320 words equal (`--stage 0x1215FB`) |
| `0x121B99` | PURE (0, 3..5) square / pulse 33 / pulse 10 | `0x1250C9`: `M0 - g saw(ph) + g saw(ph - off)` with `off = 2 DM(-8, I1)` (one word for both lanes, `0xC15AE` for channel 0; `M0` = 0) | `stage_pulse`: 320 of 320 words equal (`--stage 0x121B99`) |
| `0x1212D3` | SHAPE (3, 0) Shape Triangle | `0x1251D4` parabola difference | 320 of 320 (section 1) |

Six recorded oracle cases per new stage are in [data/synth-stage-oracle-cases.json](data/synth-stage-oracle-cases.json). Not modelled: PURE sine (`0x120CD0`) and triangle (`0x121183`), SUB OSC, SYNC, the other SHAPE waves (Shape Saw `0x12169D` is the most common wave of the factory set, 182 layers), SHAPE SINE, MULTI, SUPER, MISC.

## 7. Preset category / wave against the edit-id domain (VERIFIED by range)

The harness grid ([synth-wave-stages](synth-wave-stages.md)) gives 7, 3, 9, 5, 7, 6, 5, 3 valid waves for ANALOG categories 0..7. The ns4decode name lists have exactly these sizes (PURE 7, SUB OSC 3, SYNC 9, SHAPE 5, SHAPE SINE 7, MULTI 6, SUPER 5, MISC 3). The decoded `.ns4p` layers agree: over the 384 factory programs, all 578 ANALOG layers have `(osc_category, osc_wave)` inside that table, and every category reaches its last wave index (PURE 6, SYNC 8, SHAPE 4, SHAPE SINE 5, ...). So the preset fields and the edit ids are the same domain. Smooth Lead is SHAPE wave 0 ("Shape Triangle"), which matches the triangle-like waveform of the stage at the default shape. Check: `python` over `ns4 inspect --json` of `factory_programs/Program/*/*.ns4p` (scratch script, not committed).

## 8. The program words stay open

The P block (`P[-32]`, `P[-30]` ratios, `P[-14..-10]` weights, `P[16]` gain) and the shape words `DM(17, I2)`, `DM(19, I2)` are not derived from the strip words by the ORG or PNO core 2 pass of the harness: poking each of the 62 words of strip 0 and the 32 common words changes only the pitch word (strip word 2, common word 0). The pass installs only `0x12082F` / `0x120C68` from strip words 0 / 1, and no stage in slot 0, so the real device must reach these words through a path the oracle pass does not run (the apply flag `DM(0xC136C)`, the setup routine `0x120300`, which now runs without a fault). A loaded program in the ARM harness only produces the ARM side messages (controller bytes), not the DSP-side words. Capturing them needs the DSP-side installer, not more ARM work.

## 9. Where the P block and the shape words come from (oracle experiments, third round)

Experiments with `sharc-org` on the ORG and PNO core 2 images (commands in this section; `--watch` logs every store of the pass):

| Question | Result | Tag |
|---|---|---|
| Is the P block written by the pass? | No: `--watch 0xC1672:0x50` over 40 passes, and over a key-down pass, logs no store into it. Key-down (`DM(0xC1DCC) = 0x0B00E200` at pass 2) makes the pass run the setup routine `0x120300` once (4991 instead of 4265 steps): it resets the slot record (`DM(0xC10B8)` back to the null stage `0x120BF7`, which removes a stage installed by hand), clears the phases (`DM(-21 ..)`), but still writes nothing into `0xC1672..0xC16C2` | VERIFIED (run) |
| Is it built from strip words at boot? | No: `--pre` of each of the 62 words of strip 0 changes only the pitch record (strip word 2) | VERIFIED (run) |
| Content | one 150-word block per slot (`0xC169A + 0x96 s`), **identical for slots 0..3**: `P[-32..-17]` = 0.0770, -0.0775, -0.065, 0.0655, 0.053, -0.0535, -0.046, 0.0465, 0.031, -0.0315, -0.026, 0.0265, 0.010, -0.0105, 0, 0.0005 (a +- spread, the pairs are lane x and lane y); `P[-16 .. +17]`: four-word groups `0.25 0.25 0 0` (so the idle weights are `wB1 = wA2 = 0.25`, `wA1 = wB2 = 0`, gain `P[16] = 0.25`) | VERIFIED (dump) |
| Who writes the shape words `DM(17, I2)`, `DM(19, I2)`? | the pass itself: stores at pc `0x126966` / `0x126969` (and `0x126352`, `0x126355` for `DM(0xC1FAD..AF)`) in every pass, from the modulation source pointers (`I0 = DM(0xC0CD2)`, `I4 = DM(0xC0CD9)`, `I5 = DM(0xC0CD8)`, `I2 = DM(0xC0CD0)`), idle value 0.5 and 0 | VERIFIED (watch), sources open |
| Pitch word law | `DM(-29, I2)` is cycles per SY frame (`f / 43402.34`): strip 0 word 2 = 0 gives 0.0060278 = 261.619 Hz = C4 exactly; the top byte is a signed step count, 12 steps give a factor of 1.9801 (not 2; 17.3 cents per octave flat if the steps are semitones; see [data/synth-pitch-word.csv](data/synth-pitch-word.csv)), the low 24 bits interpolate a table (`0xB080D`) | VERIFIED (run) |

Consequence: with the oscillator increment `inc = FIX(f r 2^31)` (cycles per sample `f r / 2`), a played pitch needs `r = 2` (or a pitch word of twice the note frequency), and the idle `P[-32] = 0.077` cannot be the ratio of a running voice. Since no DSP-side routine in the pass, the setup or the key-down path writes the block, **the real P block is written from outside the ORG / PNO core 2 program**: by the other core of the same chip or by DMA (hypothesis, not tested: the oracle runs core 2 only). The ARM never sends a word into `0xC1600..0xC1700` (the captured messages in [data/synth-edit-messages.csv](data/synth-edit-messages.csv) go to `0xE0000..`, `0xC13A9..AC`, `0xC1B3C..46`, `0xB00xx`, `0x92xxx`). The next experiment is therefore on the core 1 image of the chip that plays the synth: which routine of it writes `0xC1672..` (find with `--watch` on core 1, `sharc-organ` / `sharc-bank`), fed by the same ARM messages.

## Implications

- The SHAPE category is a morphing parabola-difference oscillator (a triangle to saw-like family), not a triangle and not the saw helper: the decoder placeholder is wrong in kind. The stage, bit exact, is available (`SynthLayer::strip_shape`).
- The remaining unknowns are program words, not code: `P[-32]`, `P[-30]` (two oscillator ratios), `P[-14 .. -10]`, `P[16]`, the shape words `DM(17, I2)`, `DM(19, I2)` and the pitch word scale (`f = 2 i`, ASSUMED to follow the note).

## Follow-ups

1. Find the writer of the P block on the **core 1** program of the chip (section 9: core 2 never writes it) and replay the ARM messages of a loaded program into core 1 and core 2 together; then set ratios, weights and shape.
2. Model the remaining stages of [synth-wave-stages](synth-wave-stages.md) (sine, triangle, SUB OSC, SYNC, Shape Saw / Square / Half Sine / Parabolic, SHAPE SINE, MULTI, SUPER, MISC) with `mix_lanes` and the 40-bit helpers.
3. Port the saw and pulse stages to Rust as two-oscillator modules once the weights are known (the single-oscillator saw already exists as `SawModel::FirmwareSy`).

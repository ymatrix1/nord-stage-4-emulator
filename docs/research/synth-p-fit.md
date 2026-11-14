# Synth oscillator P block: empirical fit to hardware clips

- **Checked:** 2026-11-14
- **Sources (local, never committed):** Nord Stage 4 demo Part A (`_zksT69H30c`, Smooth Lead A:87 at 2420 s, 22 s, 156 video-confirmed notes, `tools/emu_vs_hw.py`), the Audible Genius lesson clips in `.reference/audible-genius-nord-4/` (Pure Saw and Pure Square notes of the pitch-blip clip, laser-zap clip, Super Saw clip), `tools/synth_wave_model.py` (bit-exact stages), `ns4 render-patch --synth-chain strip --synth-p`. Throw-away drivers lived in a scratch directory; the reusable parts are `tools/synth_p_fit.py` and `tools/tests/test_synth_p_fit.py`. Nothing from the clips (audio, video, frames) is committed, only derived numbers.
- **Question:** The stages are exact for any P block ([synth-wave-models](synth-wave-models.md)) but no reachable path writes the block of a program. Which block values make the exact stages reproduce the hardware sound, and is the strip chain then good enough to become the synth default (D22)?

Tags follow `.agents/rules/provenance-in-code.md`. The audio is lossy YouTube audio with speech, and the velocity of every note is unknown, so all numbers are shape evidence with the stated uncertainty.

## 1. Short answer

| Quantity | Result | Tag |
|---|---|---|
| Pure Saw stage | one oscillator: harmonics 2..12 of a steady G4 are the ideal `1/k` series, RMS residual 0.29 dB (blip clip); any second oscillator is excluded to about 0.3 dB on the even harmonics | EMPIRICAL (one clip, one note) |
| Pure Square stage | odd harmonics 3..11 within 0.3 dB of `1/k`, even harmonics below -70 dB | EMPIRICAL (one note) |
| Pulse against saw under one block | the bit-exact stage models differ by +6.0 dB in H1 (ideal unit pulse against ideal unit saw); the hardware notes differ by +2.7 dB, so the pulse stage runs 3.3 dB under the saw stage on the hardware: a different `P[16]` (or note velocity) per wave | EMPIRICAL (one pair, velocity unknown) |
| Shape stage (Smooth Lead), fitted | gain 0.60, second oscillator weight 0.35 at ratio 2.0 (octave up), shape word 0.5, cutoff factor 1.75 | EMPIRICAL (one program, half-clip hold-out) |
| Residual on the fit half / hold-out half | score (mean absolute delta of low, mid, presence, brilliance bands): 0.30 dB / 1.83 dB; presence +0.5 / -1.0 dB; RMS -0.6 / +1.0 dB | measured |
| Full clip, strip chain with `--synth-p fitted` | presence -0.8 dB, brilliance -2.9 dB, RMS +0.2 dB, centroid +14 Hz, score 0.97 (strip without P block: -9.0, -7.6, +3.8, -113 Hz, 4.05) | measured |
| Gain of the saw stage | **not identifiable**: the Pure Saw note is +6.0 dB over the strip render at velocity 100 (+3.7 dB at 127), the zap-clip saw is 13 dB lower than the blip-clip saw, the Smooth Lead wants -4.4 dB; velocity and output chain are confounded | open |
| D22 flip of `FirmwareStrip` | **no**: presence and RMS pass on the held-out half, but the hold-out is the same program and clip, no second synth program exists in the harness, and the gain disagrees by 10 dB across clips (section 6) | decision |

## 2. What the clips fix (and what they cannot)

Method: 65536-point Hann FFT of a clean steady note, strongest bin within +-30 cents of each harmonic (`tools/synth_p_fit.py harmonics`). Notes: blip clip G4 392.6 Hz.

| Note | H2..H8 re H1 (dB) | Ideal | Reading |
|---|---|---|---|
| blip clip 20.65 to 21.15 s, Pure Saw | -5.9 -9.3 -12.2 -14.1 -15.6 -17.1 -18.3 | -6.0 -9.5 -12.0 -14.0 -15.6 -16.9 -18.1 | residual 0.29 dB over H2..H12: one saw oscillator |
| blip clip 26.5 to 27.25 s, Pure Square | H3 -9.6, H5 -14.1, H7 -17.1, H9 -19.4, H11 -21.3; H2, H4, ... below -70 | -9.5 -14.0 -16.9 -19.1 -20.8 | one pulse oscillator, 50 % |
| zap clip 9.0 to 9.5 s, saw through the LPM filter | -6.5 -9.4 -11.4 -12.7 | -6.0 -9.5 -12.0 -14.0 | filter and resonance act, not a clean oscillator note |

Levels: the blip saw H1 is -12.3 dBFS, the square H1 -9.6 dBFS, the zap saw H1 -25.5 dBFS. The strip chain renders a G4 saw (filter off, velocity 100) at -18.3 dBFS and at -16.0 dBFS with velocity 127. The 13 dB spread between two Pure Saw notes shows that the clips do not fix an absolute level, so the P gain cannot be taken from them; the relative level of two waves in one clip is usable (pulse against saw, row 3 of section 1). The stage model under one block gives pulse H1 +6.12 dB and saw H1 +0.24 dB re an ideal unit saw (`python tools/synth_p_fit.py stage saw 392.6`, `... pulse 392.6`).

Second-harmonic point of the Smooth Lead (-3.6 dB, [synth-strip-path](synth-strip-path.md) s4): the single shape oscillator cannot reach it. For shape words -1..1 at B5 the stage gives H2 between -6.1 and -38 dB re H1 (sweep with `tools/synth_wave_model.py`), the saw limit is -6.1 dB. A louder second harmonic needs a second oscillator, which is what the fit finds (octave up).

Consistency with what is known:

- Pitch: the fit keeps ratio 1 for the played pitch (`r = 2` of the stage input is the base case, [synth-wave-models](synth-wave-models.md) s9); the second oscillator is relative to it (`ratio2 = 2.0`).
- Shape word: the fit lands on the idle value 0.5 of the frame (`DM(17, I2) + DM(19, I2)`); 0.35 to 0.7 are within 0.5 dB at unit gain. The shape knob law (0..1 to pulse width `W`) stays the stage's own `W = s (1 - 1.5 a)`.
- 16-voice Super Saw detune law (`measured.rs`): untouched, the P block acts only on the Pure Saw and SHAPE stages, not on `SuperSaw`.

## 3. Fit design

Free parameters (`synth/p_block.rs`): `gain`, `weight2`, `ratio2`, `shape`, `cutoff_scale`. The cutoff factor is not an oscillator word but is fitted jointly: on the Smooth Lead the oscillator and the cutoff scale are confounded ([synth-strip-path](synth-strip-path.md) s4: a 1.35 times higher cutoff alone closes the presence gap). Both the saw and the shape stage, one program, one fixed replay velocity (100).

Data: the confirmed notes of the Smooth Lead clip, the 22 s split at 11 s. Fit half: 0 to 11 s. Hold-out half: 11 to 22 s. Both halves are re-analysed with the same pipeline on the hardware slice and the emulator slice (`segment_metrics`), so the analysis bias cancels. The score is the mean absolute band delta of low, mid, presence and brilliance (level free). Four grid rounds (95 renders): shape {0.5, 1.0} x cutoff {1.0 .. 1.75} x (second oscillator off, or weight {0.25, 0.5} x ratio {2.0, 1.5, 1.006}); then weight {0.35, 0.5, 0.7} x shape {0.35, 0.5, 0.7} x cutoff {1.4, 1.5, 1.6}; then gain 0.6 with weight {0.35, 0.5, 0.7} x cutoff {1.5, 1.75, 2.0}.

Findings of the grids:

- Without a second oscillator and with the shape stage (shape 0.5 or 1.0, saw limit), presence stays at -9 to -12 dB at unit cutoff and brilliance is wrong; the saw is the brightest single oscillator the stage can make, so the oscillator alone cannot close the gap.
- With `ratio2 = 2.0` and weight 0.35 to 0.5 the fit half lands at 0.3 to 0.6 dB score for cutoff 1.5 to 1.75. Ratios 1.5 and 1.006 (detune) score 0.4 to 1.2 dB worse on the fit half.
- Gain and band deltas are not separable: at gain 1 the emulator output clips and adds brilliance (air band +30 dB re total on every variant, so that band is not used), so the gain is fitted after the cell: 0.6 (-4.4 dB) makes the fit-half RMS delta 0 within 0.6 dB.

## 4. Residuals and parameter uncertainty

| Cell (gain, weight2, ratio2, shape, cutoff) | Fit half score | presence / RMS (dB) | Hold-out score | presence / RMS / brilliance (dB) |
|---|---|---|---|---|
| **0.6, 0.35, 2.0, 0.5, 1.75 (chosen)** | **0.30** | +0.5 / -0.6 | **1.83** | -1.0 / +1.0 / +4.4 |
| 0.6, 0.5, 2.0, 0.5, 1.75 | 1.03 | +1.5 / +0.1 | 1.88 | -0.4 / +1.7 / +5.5 |
| 0.6, 0.5, 2.0, 0.5, 1.5 | 1.86 | +1.8 / +0.4 | 0.68 | -0.5 / +2.0 / -1.2 |
| 0.6, 0.7, 2.0, 0.5, 1.5 | 1.84 | +2.6 / +1.4 | 0.20 | -0.1 / +2.9 / 0.0 |
| 0.6, 0.35, 2.0, 0.5, 1.5 | 1.71 | +0.7 / -0.4 | 1.16 | -1.0 / +1.4 / -2.3 |
| 0.6, 0.5, 2.0, 0.5, 2.0 | 2.15 | +0.9 / -0.1 | 3.39 | -0.7 / +1.4 / +10.9 |

Uncertainty: weight2 0.35 to 0.7, cutoff factor 1.5 to 1.75, shape 0.35 to 0.7 and gain 0.45 to 0.85 all reproduce presence within 3 dB and RMS within 3 dB on both halves; the score is flat to about 1.5 dB across them and brilliance moves by +-4 dB between neighbouring cells (the hold-out half picks different cells than the fit half). So the values above are one point of a plateau, not a sharp optimum, and the fit does not separate the second oscillator weight from the cutoff factor: the two raise the 1.5 to 3 kHz band together. A cutoff factor of 1.5 with weight 0.7 is as good (hold-out score 0.20).

## 5. What was implemented

`synth/p_block.rs` (`PBlock`, `PSpec`): `out = gain (osc(f) + weight2 osc(f ratio2))` on the second oscillator's own phase, the SHAPE stage (`fw_shape_wave`) for the shape category and the firmware saw otherwise. The Pure Saw stage uses `PBlock::single` (unit gain, no second oscillator; section 2 excludes it, the gain is not identifiable) and only shares the cutoff factor. Opt-in: `--synth-p fitted` or `--synth-p gain,w2,r2,shape,cutoff` on `ns4 render-patch`, `FirmwareOptions::synth_p` (a dropdown entry later), strip chain only; `render_hash` is unchanged because the field is `None` by default. `PBlock::FITTED` is `gain 0.6, weight2 0.35, ratio2 2.0, shape 0.5, cutoff_scale 1.75`, tagged EMPIRICAL with these clips and residuals.

`tools/emu_vs_hw.py --only synth_lead --confirmed-only` (variant `strip+p` in `emu_vs_hw_segments.json`):

| Variant | sub | low | mid | presence | brilliance | RMS dB | centroid Hz | score |
|---|---|---|---|---|---|---|---|---|
| strip | -0.9 | -1.2 | +1.5 | -9.0 | -7.6 | +3.8 | -113 | 4.05 |
| strip + `--synth-p fitted` | -0.1 | -0.8 | +0.3 | -0.8 | -2.9 | +0.2 | +14 | 0.97 |

## 6. D22 decision: `FirmwareStrip` stays opt-in (superseded by [synth-validation](synth-validation.md): independent clips, per-wave table, default flipped)

| Criterion | Result |
|---|---|
| Oracle exact stages | met (envelope, filter, shape, saw, pulse) |
| Presence within about 3 dB, held-out half | met: -1.0 dB (full clip -0.8 dB) |
| RMS within about 3 dB, held-out half | met: +1.0 dB (full clip +0.2 dB) |
| Independent hold-out | **not met**: the hold-out half is the same program and clip as the fit (same filter settings, same cutoff scale, same player); the harness has one synth program (`synth_lead`), the lesson clips cannot validate the level (velocity unknown, two Pure Saw notes 13 dB apart, the Pure Saw level wants +6.0 dB while the lead wants -4.4 dB) |
| `render_hash` re-baselined | not applicable (not flipped) |

Flipping would apply a gain, an octave oscillator and a 1.75 cutoff factor derived from one preset to every synth program. Brilliance (-2.9 dB full clip, +4.4 dB hold-out half) is not stable between halves. The strip chain stays opt-in; `--synth-p fitted` is the recommended companion for A/B listening.

## Stop point

Time box reached after: the Smooth Lead fit with a half-clip hold-out, the Pure Saw / Pure Square harmonic checks, the opt-in implementation and the D22 evaluation. Not done: a second synth program for an independent hold-out (needs more video segments, Part A has other synth leads, `Space Drama II`, `Phat 3 Osc MiniM`, `Pinky Lead`); a level fit of the saw stage (needs a clip with known velocity); the Pure Square gain under the stage `0x121B99` (the pulse stage is modelled in Python only, [synth-wave-models](synth-wave-models.md); the other agent's lane); partner-lane weights `P[-14]`, `P[-10]` (set to 0, ASSUMED).

## Implications

- The oscillator P block cannot be recovered from clips alone: shape, second oscillator and cutoff factor are confounded on a filtered lead, and the level is confounded with velocity.
- The Pure Saw and Pure Square stages need no second oscillator.
- The strip chain with the fitted block reproduces the Smooth Lead bands within 3 dB on this clip; it does not yet justify a default.

## Follow-ups

1. Replay the other Part A synth leads (`Space Drama II`, `Phat 3 Osc MiniM`, `Pinky Lead`, `Filter Bank Ld`) with `--synth-p fitted` as independent hold-outs; flip only if presence and RMS stay within 3 dB there as well.
2. Read a played Smooth Lead note's P block from the device (the two methods of [synth-wave-models](synth-wave-models.md) s10); that replaces this fit.
3. Fit the saw gain from a clip with a known velocity (a controller-recorded note), and the Pure Square gain when the pulse stage is in Rust.

## Reproduce

```bash
cargo build --release -p nord-core --bins
python tools/emu_vs_hw.py --video <PartA.webm> --backup <factory.ns4b> --workdir <scratch> --only synth_lead --confirmed-only
ns4 render-patch "Program/Bank A/Smooth Lead.ns4p" --backup <factory.ns4b> --events <events.json> --wav out.wav --synth-chain strip --synth-p 0.6,0.35,2.0,0.5,1.75
python tools/synth_p_fit.py harmonics <clip.wav> 20.65 21.15 392.6      # ffmpeg -i clip.webm -vn -ac 2 -ar 48000 clip.wav
python tools/synth_p_fit.py stage pulse 392.6
```

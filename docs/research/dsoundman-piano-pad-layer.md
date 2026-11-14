# DSoundman piano + synth pad layer: balance and the synth layer level law

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `.reference/DSoundman-course/Nord Stage 4 - Building a Piano and Synth Pad Layered Sound Tutorial ... [hR31DQrxfT4].webm` (1258 s, 3840x2160 top-down view of the whole panel, hardware Stage 4) and its auto-generated `.srt` (cue boundaries only; speech is not synchronised to the notes). Factory restore `.reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b` for the emulator piano. Tools: ffmpeg frames (fader crops, native `1190,840` and `2030,840`), `ag_audio.py` helpers, `ns4 render-patch`, `ag_synth` (`notes=` list). Related: [organ-output-law](organ-output-law.md), [piano-downstream-level](piano-downstream-level.md), [audible-genius-synth-lessons](audible-genius-synth-lessons.md).
- **Question:** What is the piano versus synth-pad balance of the layered program on hardware, how does the emulator compare, and does the synth layer level need the `(v/127)^2` law of the organ and piano layers?

Tags follow `.agents/rules/provenance-in-code.md`. The audio is lossy, the presenter speaks over most of it, the notes and velocities are played live and differ between segments, and the recording gain is unknown, so only level differences inside the clip are used.

## 1. The program

The program is user-made ("PianoPad", H:13), not a factory program, so there is no `.ns4p` to render. Settings read from frames and speech:

| Item | Value | Confidence |
|---|---|---|
| Piano A | White Grand, layer fader at the top of its travel, Piano B fader at the bottom (off), string res on, pedal noise on, Dyn Comp 25 % (speech), Reverb "Stage" about 4, EQ treble +4, bass -4 (speech) | high (frames), medium (speech) |
| Synth A | analog Super Saw from preset "Soft Pad 1"; Amp Env attack 690 ms, decay Inf, release 1.94 s (OLED, 735 s); Filter Env attack lengthened to about 1 to 2 s, decay Inf, release 3.6 s; amp release 4.6 s (speech, later edited again); Freq about 70 to 73 (speech, panel units not shown in Hz), Cloud reverb dark, about 3.5 (speech) | medium |
| Synth fader | at the top of its travel until about 15:05, then about 0.7 of the travel (cap near the lower third) for the rest of the clip; at 18:44 it is assigned to the mod wheel (fader down, wheel up) | medium (cap position read by eye, about +-0.1) |

## 2. Segments and levels (EMPIRICAL)

RMS of the mono mix over `[Music]` cue runs of the `.srt` (no speech); bands are the share of total power in 20 to 250 Hz / 250 to 2 kHz / 2 to 8 kHz.

| Segment | Layer | Time | RMS dBFS | 2 to 8 kHz share (dB) |
|---|---|---|---|---|
| piano alone (init grand, no FX) | piano | 0:20, 0:31, 1:11, 1:45 | -15.6, -16.7, -16.3, -17.2 | -12 to -19 |
| piano alone with Reverb, EQ, Comp | piano | 3:43, 3:60, 4:37, 5:11 | -17.2, -18.5, -18.4, -15.7 (mean -17.5) | -15.5 to -22 |
| pad solo | synth (fader full) | 8:11, 12:06, 14:04 | -16.3, -16.8, -17.7 (mean -17.0) | -40.5, -46, -42.6 |
| pad solo, filter Freq at minimum | synth | 14:22 | -23.0 | -52.5 |
| pad solo, volume lowered, Cloud reverb | synth (fader about 0.7) | 17:23 | -20.4 | -45.9 |
| both, fader full | piano + synth | 12:40, 13:02, 13:52 | -13.4, -15.0, -14.5 | -17 to -20 |
| both, pad fader about 0.7 | piano + synth | 15:33, 17:34, 18:59 | -15.8, -15.9, -16.7 | -18 to -20 |

Pad attack and release were not measurable (the speech and the live playing hide them; the `[Music]` runs are chords played over and over). The pad spectrum is the only clear pad property: almost no energy above 2 kHz (-40 to -46 dB re total), so the pad filter is closed to roughly 1 kHz.

## 3. Balance against the emulator

Emulator renders of a four-note E major chord (E3, B3, E4, G#4), 4 s held:

| Layer | Setup | RMS dBFS (0.7 to 4.0 s) | 2 to 8 kHz share |
|---|---|---|---|
| piano | `ns4 render-patch "Program/Bank A/Velvet Grand.ns4p" --notes 52,59,64,68 --velocity 100` (identical with `--firmware-routing`) | -26.8 | -22.0 |
| pad | `ag_synth notes=52,59,64,68 wf=supersaw detune=3 cutoff=1000 amp_atk=0.69 amp_rel=1.94` (cutoff 600 / 1000 / 1500 / 2500 Hz give -57.8 / -44.3 / -34.4 / -24.6 dB share) | -26.3 | -44.3 |
| pad, `measured_laws` (Super Saw 16 voices) | same | -28.3 | -45.5 |

| Quantity | Hardware | Emulator | Error |
|---|---|---|---|
| pad minus piano RMS (both faders full) | +0.5 dB (-17.0 against -17.5; scatter of the cues about 1 dB) | +0.5 dB default, -1.5 dB with `measured_laws` | 0 dB default, -2 dB with `measured_laws`; inside the scatter |
| pad 2 to 8 kHz share | -40.5 to -46 dB | -44.3 dB at cutoff 1000 Hz | inside range (cutoff chosen to match, so this is a fit, not a test) |
| piano 2 to 8 kHz share (reverb, EQ on) | -15.5 to -22 dB | -22.0 dB (no reverb, EQ) | edge of range |
| both minus piano alone (power sum of equal layers is +3 dB) | +2.5 to +4 dB (13:02 to 12:40 against 3:43 to 5:11) | +3.0 dB by construction (equal levels) | consistent |

Conclusion: with both faders at the top and equal playing, the hardware piano and pad are within about 1 dB, and the emulator layers at layer level 1.0 and default levels (piano the engine's `0.55` sample scale, synth the default gain) are also within 0.5 dB (2 dB low with `measured_laws`). No balance error larger than the 1 to 2 dB scatter of the clip is visible. This is a weak test: velocity, chord voicing, pad Detune and the unknown Super Saw level against Detune are free.

## 4. Does the synth layer need `(v/127)^2`?

Evidence available: the pad fader goes from the top to about 0.7 of its travel at about 15:05.

| Measurement | Value |
|---|---|
| pad solo RMS before (14:04, fader full) and after (17:23, fader about 0.7, Cloud reverb added, filter Freq slightly lowered) | -17.7 and -20.4 dBFS: -2.7 dB |
| the mix, both layers (12:40 to 13:52 against 15:33 to 18:59) | -0.8 to -2 dB (equal layers, pad scaled by `g`: `10 log((1 + g^2) / 2)` is -1.3 dB for `g = 0.7` and -2.0 dB for `g = 0.49`) |
| linear fader to level (`g = 0.7`) | pad -3.1 dB |
| `(v/127)^2` law (`g = 0.49`) | pad -6.2 dB |

The solo difference (-2.7 dB) agrees with the linear law (-3.1 dB) and not with the square law (-6.2 dB); the mix difference is in between and compatible with both. The fader taper (position to value) is unknown, the cap position has an error of about 0.1, and the reverb and filter edits move the level by an unknown amount. **No evidence for `(v/127)^2` on the synth layer; weak evidence for linear.** The engine keeps the linear synth layer level. EMPIRICAL (one fader move, confounded).

## 5. Implications

- The default balance of piano and synth layers needs no change from this clip (error 0 to -2 dB, inside the scatter).
- The synth layer level law stays open (OQ22); the layered clip does not support a square law, but a clean test (same chord, fader at 127 / 90 / 64, no other edits) on hardware is needed.
- The pad's reverb and the filter-envelope attack of 1 to 2 s were not compared; only levels and the spectral tilt were.

## 6. Follow-ups

1. Hardware: one chord at synth fader 127, 100, 64, 32 with every other setting fixed; read the fader value on the OLED (Layer level page) to avoid reading the cap.
2. Emulator: render the full PianoPad program once a `.ns4p` of it is available (store dump), with `--firmware-routing`, and compare the layer sum and the pad envelope onset (attack 690 ms, release 1.94 s) on a single sustained chord with no speech.
3. Pad attack and release from a clip without speech and a single chord.

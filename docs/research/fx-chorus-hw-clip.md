# FX Mod 2 CHOR against a hardware clip: oscilloscope edge separation

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `.reference/chorus-nord-vs-ultimate-pianos2/` (video by Junior Porciuncula, "Nord Stage 4 Chorus against the Chorus of Kontakt Ultimate Pianos 2": `clip.mp4` AV1 1080x1920 30 fps + AAC 44.1 kHz, 41.8 s; `clip.srt`; `L.wav` = left channel = Nord Stage 4 Mod 2 Effect B Chorus on a piano, `R.wav` = right channel = the plugin chorus, both mono). Tools: [tools/fx_chorus_clip.py](../../tools/fx_chorus_clip.py), `crates/nord-core/examples/fx_chorus_probe.rs`. Derived data: [data/fx-chorus-hw-scope.csv](data/fx-chorus-hw-scope.csv), [data/fx-chorus-probe-grid.csv](data/fx-chorus-probe-grid.csv).
- **Question:** which Nord CHOR type (`kFx2Type` 4 or 5) matches the hardware, and are the firmware chorus (`FirmwareFx::mod2`) and the `Amt` / `Rate` laws closer to it than the engine chorus (D22 bar: no side worse than about 3 dB)?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Quantity | Hardware clip | Firmware `fw4` (type 4) | Engine chorus | Tag |
|---|---|---|---|---|
| LFO rate | 0.336 Hz (47 frames, spectral peak); 4 maxima at 24.5, 27.5, 30.5, 33.0 s | 0.343 Hz at rate word `v = 30` (`12800 + 4096 v`) | `mod2_rate_hz` as set | EMPIRICAL: rate matches the law for `v = 30` |
| Delay of the copy (5th to 95th percentile) | 0.62 to 1.78 ms, mean 1.17 ms | 0.87 to 3.28 ms, mean 2.02 ms (amount 8..64) | 4.5 to 13.5 ms at amount 1 (9 ms centre) | EMPIRICAL |
| Swing / centre | 0.48 | 0.58 | 0.50 | EMPIRICAL |
| Second edge over first (wet gain) | 0.86 (median, 54 rows) | 0.82 at amount 64, 0.88 at 90 (about 80 for 0.86) | 1.0 at amount 127 | EMPIRICAL |
| Nord trace against plugin trace | within 0.1 ms on all 7 overlapping frames | n/a | n/a | EMPIRICAL |

Verdict: the rate agrees with CHOR type 4 and the wet gain with an amount near 80; the **absolute delay is 0.55 times the firmware's** (and 0.13 times the engine's) if the displayed sawtooth is the 87.31 Hz note. The clip does not show that, so **CHOR stays opt-in** (D57). Type 5 cannot be told from the clip: its output is not a clean delayed copy of an edge (s4), the scope shows one.

## 2. What the clip contains

| Time (s) | Content |
|---|---|
| 0 to 6 | Nord panel, desk, DAW; narration |
| 6 to 22 | Logic Pro tracks with the Kontakt panel (Effect B = Chorus, Rate and Amount knobs, not readable); L / R playing chords (dominant peaks 100 to 765 Hz) |
| 22 to 38 | one sustained note, fundamental 87.31 Hz (F2; harmonic comb fit 86.5 to 88.2 Hz, `tools/fx_chorus_clip.py` `F0_HZ`) with a DAW oscilloscope in frame from 22.5 s to about 35 s: two traces (white upper, yellow lower) |
| 38 to 41.8 | DAW overview |

* `L.wav` and `R.wav` are mono (identical left and right samples), levels within 0.3 dB from 22 to 38 s (up to 3 dB earlier), envelope correlation 0.92, harmonics 1 to 4 of the F2 note equal to three digits, harmonics 5 and up differ (`0.008` against `0.014`, `0.004` against `0.009`).
* **No dry / wet A/B inside the clip**: both channels are wet for the whole time. Chorus on in the left channel is inferred from the narration and the scope, not from a dry reference.
* `tools/video_ref.py` note detection was **not** used: the keyboard and OLED are visible only in the first two seconds, so the notes come from the audio (comb fit above).
* The audio gives no LFO line: the log amplitude of harmonics 1 to 10 over 23 to 38 s peaks at 0.33 to 1.7 Hz without a common value (the performance moves the levels), identically in L and R for `k <= 3`.

## 3. Scope reading

The scope shows a **decaying sawtooth of 11.45 ms period** (period of 87.31 Hz) with, before every main edge, a smaller step: the delayed copy. Method (`scope` subcommand, 4 frames per second from 22.5 to 35.5 s, bright-pixel runs):

1. vertical edges = columns with at least 40 bright pixels in a run, merged over adjacent columns;
2. the first edge is followed within 15 to 130 px by the second one (higher on screen); a partner pair one period later (350 to 650 px, same screen row) must exist;
3. `delay_ms = sep_px / period_px * 11.454`; the period of the same trace calibrates the horizontal axis, so camera zoom and tilt cancel to first order.

Results ([data/fx-chorus-hw-scope.csv](data/fx-chorus-hw-scope.csv), 47 rows of the upper trace, 7 of the lower):

* delay series: minima at 22.75, 26.0, 29.0, 32.0, 34.25 s, maxima at 24.5, 27.5, 30.5, 33.0 s: period 2.97 s, **0.336 Hz** (spectral peak), 0.62 to 1.78 ms;
* edge height of the second edge over the first: 0.862 (upper), 0.857 (lower);
* the lower trace follows the upper one within 0.1 ms (7 shared frames: 1.134/1.206, 1.350/1.362, 1.603/1.607, 1.832/1.808, 1.737/1.676, 1.482/1.435, 1.285/1.229 ms).

Limits: a phone filming a screen (perspective, glare, exposure), 4 frames per second against a 3 s cycle (fine), the lower trace overlaps the upper one in some frames (7 rows only). Which trace is the Nord is not certain (the narration says yellow, the frames show white and yellow lines); both agree, so it does not change the numbers. **The sawtooth is not the F2 audio** (the folded F2 waveform of `L.wav` is a smooth low-passed piano shape with harmonic amplitudes `0.149 0.030 0.056 0.013`), so the scope signal is a separate test tone whose frequency and source are not in the clip; the 87.31 Hz calibration is an assumption (ASSUMED: the scope period is the F2 note period; a different frequency scales every delay).

## 4. Emulator on the same sawtooth

`examples/fx_chorus_probe` runs a mono file through one `LayerFxChain` (every other stage off) with `fw4`, `fw5`, `engine`, `engine5` at 48 kHz; `fx_chorus_clip.py grid` feeds it the same sawtooth (87.31 Hz, `exp(-3 p)` decay) and measures the same two-edge separation ([data/fx-chorus-probe-grid.csv](data/fx-chorus-probe-grid.csv), per output channel).

| Setting | Delay min / max / mean (ms) | LFO (Hz) | Second over first edge |
|---|---|---|---|
| `fw4` v 30, amount 8 / 16 / 32 / 48 | 0.87 to 3.28, mean 2.02 | 0.343 | 0.08 / 0.17 / 0.37 / 0.59 |
| `fw4` v 30, amount 64 / 90 / 127 | 0.87 to 3.29 / 0.89 to 3.63 / 1.08 to 5.04 | 0.343 | 0.82 / 0.88 / 0.99 |
| `fw4` v 10 / 60, amount 127 | 1.06 to 4.79 / 0.93 to 5.17 | 0.131 / 0.652 | 0.99 |
| `fw5` v 30, amount 127 | no edge pair | | |
| engine, 0.34 Hz, amount 64 / 127 | 6.7 to 8.8 / 0.7 to 8.4 (the detector window ends at 9 ms; the engine swing is 4.5 to 13.5 ms) | 0.340 | 0.34 / 1.0 |

* **Stereo:** the firmware stage returns the delayed copy on the right output only for a mono input (`ch 1`); the left output is the dry signal plus small spikes (`h` ratio 0.00 to 0.02). The clip is two mono recordings, so it cannot show whether the hardware chorus is stereo.
* **Type 5** (variation) gives a dispersed bump and dip after the edge, not a clean second edge; the clip scope shows a clean edge, so type 4 (non-variation) is the type that agrees, but the variation was not excluded by a second clip (the Nord knob state is unknown).
* The firmware swing is nearly amount independent below amount 64 (`c[-0x1E]` 117 to 156 frames, `c[-0x1D]` 171 to 180 frames), so the delay range cannot be fitted by the amount; only the edge height ratio can (amount near 80 for 0.86).
* Rate: `12800 + 4096 v` for `v = 30` is 135 680, 0.343 Hz; the hardware 0.336 Hz is within 2 %; the clip fixes one rate point, the other ten of the law stay as before (11 captured samples, EMPIRICAL between).

## 5. Implications

* The type 4 rate law and the dry plus delayed copy structure (one clean copy, wet 0.86 of dry) agree with the hardware; the engine chorus (9 ms centre) does not (7.7 times too long if the 87.31 Hz calibration holds).
* The absolute delay of the firmware stage is 1.7 times the clip's. Candidates: the scope frequency differs (a 150 Hz saw would match), the engine-rate rescale of the delay words (`delay_scale = fs / F`, ASSUMED), the hardware layer row (piano words used for every layer, ASSUMED), or the Nord knob state (the swing words come from the amount law between samples).
* D22 asks for band and modulation metrics with no side worse than 3 dB: the delay misses by a factor 1.7 (swing 2.1) and the clip has neither a dry reference nor an audio comb, so no band metric exists to compare; the flip is not justified. `FirmwareFx::mod2` stays out of `DEFAULT`; `render_hash` and the GUI default are unchanged.

## Follow-ups

1. From the user: the Nord Mod 2 settings of the recording (Rate knob position and the Amount knob, VARIATION on or off, program or sound, Mod 2 B and which layer), the plugin knob values, and what the scope shows (the signal source, its frequency, the time axis of the scope window).
2. One more clip with chorus off then on (or a dry reference recorded in the same take) so the audio comb can be fitted, not only the scope.
3. A stereo recording (left and right outputs of the Nord) to settle the right-only wet of the port.
4. Re-run `uv run python tools/fx_chorus_clip.py grid` with the corrected frequency when the scope frequency is known.

## Reproduce

```bash
cargo build --release -p nord-core --example fx_chorus_probe
uv run python tools/fx_chorus_clip.py scope .reference/chorus-nord-vs-ultimate-pianos2/clip.mp4 --out docs/research/data/fx-chorus-hw-scope.csv
uv run python tools/fx_chorus_clip.py stats docs/research/data/fx-chorus-hw-scope.csv
uv run python tools/fx_chorus_clip.py grid --workdir <tmp> --out docs/research/data/fx-chorus-probe-grid.csv
uv run pytest tools/tests/test_fx_chorus_clip.py
```

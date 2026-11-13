# Emulator vs hardware audio (Part A demo video), ranked by engine

**Checked:** 2026-11-13
**Sources:** `NEW NORD STAGE 4 - PROGRAMAS PRESETS DEMO (PART A) [_zksT69H30c].webm` (local, git-ignored), `Nord Stage 4 Factory Restore RevK v1.66.ns4b`, `docs/research/data/video_part_a_programs.json` / `video_part_a_scenes.json`, `tools/video_ref.py`, `tools/audio_notes.py`, `tools/key_vision.py`, `tools/emu_vs_hw.py` (new), `ns4 render-patch` (release build, `main` @ 1664155 plus the new opt-in flags). Raw numbers: `docs/research/data/emu_vs_hw_results.json`; segment spec: `docs/research/data/emu_vs_hw_segments.json`. Earlier work: `hardware-audio-reference-fx.md` (beans ns4-t5zr, ns4-rh3z).
**Question:** How far is the emulator's output from the hardware recording on the same notes, per engine (organ B3 / Vox / Pipe, piano, synth, amp FX), and what does each opt-in firmware path (`ToneWheelSource`, `FilterEngine::Firmware`, `FirmwareFx`) change?

The hardware audio is lossy YouTube audio of a live performance in a room, with knob and wheel moves we do not replay. Every number below is shape/level evidence, not bit-exactness. No audio, video or frames are committed.

## Method

1. Six clean single-engine segments were chosen from the program list (section-enable bits of the factory `.ns4p`: 4 = organ only, 2 = piano only, 1 = synth only; verified against the OLED frame at 3:56 for A:14).
2. `tools/emu_vs_hw.py` extracts the 48 kHz hardware slice (`video_ref.py extract`), detects the played notes (`video_ref.py notes`: audio salience plus fingertip video support), renders the **same note events** once per variant with `ns4 render-patch --events ev.json --velocity 100 [--confirmed-only]`, and compares with `video_ref.compare_wavs` (6-band spectrum re RMS, centroid, stereo width, RMS) plus a new attack metric (median 10 to 90 % rise of the 2 ms envelope after each onset; EMPIRICAL, low confidence because notes overlap).
3. Variants use new opt-in `render-patch` flags (default renders are unchanged; `render_opts.rs`): `--tone-source default|legacy|firmware|vox|b3wheels`, `--filter-engine zdf|firmware`, `--firmware-fx` (delay mix law plus firmware compressor on every layer).
4. **Score** = mean |band delta| in dB over the five bands 20 Hz..8 kHz. The 8..20 kHz band is not scored: the YouTube audio is low-passed (hardware is -69 to -74 dB re RMS there, emulator -43 dB), so it reads +14 to +31 dB on every clip and says nothing about the instrument.
5. Primary table uses **video-confirmed notes only**. Replaying all detected notes (including ghost/harmonic detections, 160 to 430 per clip) is in the JSON (`all_notes`): it raises emulator level by 2 to 4 dB and clips more, and does not change any ranking below.

Deltas are **emulator minus hardware**. Positive band delta = emulator has more energy in that band relative to its own RMS.

## Segments

| Id | Program (OLED) | Engine / FX | Clip | Notes (confirmed / detected) | HW RMS dBFS |
|---|---|---|---|---|---|
| organ_b3 | A:14 B3 Chords n Solo | organ B3, rotary, reverb | 3:58, 25 s | 101 / 285 | -20.5 |
| organ_vox | A:84 Vintage Vox J3 | organ Vox, Twin amp, reverb | 39:20, 11 s | 36 / 160 | -26.8 |
| organ_pipe | A:38 Space Drama II | organ Pipe2 (+ synth section on), reverb | 18:14, 30 s | 259 / 430 | -26.0 |
| piano_dry | A:61 Velvet Grand | piano Grand, no FX | 28:28, 25 s | 169 / 299 | -24.5 |
| synth_lead | A:87 Smooth Lead | synth Triangle LP24 | 40:20, 22 s | 156 / 210 | -22.5 |
| fx_amp | A:64 Tiny Transistor | organ Farfisa, Small II amp, EQ, reverb | 30:12, 25 s | 78 / 366 | -19.4 |

## Results (confirmed notes, emulator minus hardware)

Bands in dB re RMS; centroid in Hz; RMS in dB; width = side/mid RMS ratio (hw / emu); attack in ms (hw / emu).

| Segment | Variant | sub 20-120 | low 120-400 | mid 0.4-1.5k | pres 1.5-3k | brill 3-8k | air 8-20k (not scored) | centroid | RMS | width hw/emu | attack hw/emu | score |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| organ_b3 | default (Legacy) | +17.5 | -0.2 | +0.3 | -5.7 | +0.5 | +19.5 | -208 | +14.9 | 0.48/0.25 | 107/39 | 4.83 |
| organ_b3 | b3wheels | +16.9 | -0.0 | +0.3 | -5.1 | +1.1 | +21.2 | -202 | +14.9 | 0.48/0.25 | 107/40 | 4.69 |
| organ_b3 | firmware-fx | +17.5 | -0.2 | +0.3 | -5.7 | +0.4 | +19.4 | -208 | +14.9 | 0.48/0.25 | 107/39 | 4.83 |
| organ_vox | default (Legacy) | -4.1 | +0.4 | +13.4 | -4.7 | -5.8 | +13.7 | -606 | +11.1 | 0.50/0.18 | 86/45 | 5.68 |
| organ_vox | vox | -8.0 | +2.5 | +11.3 | -8.2 | -8.3 | +15.5 | -921 | -0.8 | 0.50/0.17 | 86/21 | 7.66 |
| organ_vox | vox+firmware-fx | -8.1 | +2.5 | +11.5 | -8.2 | -8.2 | +15.6 | -916 | -0.9 | 0.50/0.15 | 86/21 | 7.69 |
| organ_pipe | default (Firmware bank) | +4.1 | +3.0 | -0.6 | -5.9 | -11.2 | +0.8 | -316 | +7.3 | 0.74/0.33 | 58/70 | 4.95 |
| organ_pipe | legacy-tone | +3.3 | +2.9 | +0.3 | -4.3 | -9.2 | +1.6 | -271 | +6.7 | 0.74/0.33 | 58/67 | 3.99 |
| organ_pipe | firmware-fx | +4.1 | +3.0 | -0.6 | -5.9 | -11.2 | +0.8 | -316 | +7.3 | 0.74/0.33 | 58/70 | 4.95 |
| piano_dry | default | +2.1 | +0.3 | -1.6 | +3.1 | +6.4 | +5.0 | -26 | +9.3 | 0.46/0.47 | 87/68 | 2.70 |
| piano_dry | firmware-fx | +2.1 | +0.3 | -1.6 | +3.1 | +6.4 | +5.0 | -26 | +9.3 | 0.46/0.47 | 87/68 | 2.70 |
| synth_lead | default (ZDF) | +0.4 | -0.3 | +0.7 | -16.5 | -6.9 | +31.2 | -232 | -6.4 | 0.28/0.09 | 20/42 | 4.93 |
| synth_lead | fw-filter | +0.7 | +0.1 | +0.5 | -15.2 | -5.8 | +29.6 | -269 | +3.9 | 0.28/0.09 | 20/42 | 4.46 |
| synth_lead | fw-filter+fx | +0.7 | +0.1 | +0.5 | -15.2 | -5.7 | +29.6 | -269 | +3.9 | 0.28/0.09 | 20/42 | 4.46 |
| fx_amp | default | +4.4 | +2.3 | -0.7 | +1.3 | +3.8 | +1.5 | +127 | +7.0 | 0.09/0.06 | 22/42 | 2.51 |
| fx_amp | firmware-fx | +4.4 | +2.3 | -0.7 | +1.3 | +3.8 | +1.2 | +127 | +7.0 | 0.09/0.06 | 22/42 | 2.51 |

Level, clipping and crest (from the committed JSON): emulator peak is 0 dBFS on organ_b3 (10 % of samples at full scale, crest 5.6 dB vs hardware 17.6 dB), organ_vox default is -3 dB peak, piano_dry and fx_amp touch 0 dBFS (0.005 % / 0.18 % of samples). Hardware peaks are -2.9 to -11.6 dBFS except piano (0.0).

Cross-engine level calibration: the absolute RMS delta depends on the unknown performance velocity and YouTube loudness handling, but its **spread** across engines does not. Default RMS deltas are +14.9 (B3), +11.1 (Vox), +7.3 (Pipe), +9.3 (piano), -6.4 (synth), +7.0 (Farfisa+amp). Relative to the median (+8.2): B3 +6.7 dB hot, Vox +2.9, Pipe -0.9, piano +1.1, **synth -14.6 dB quiet**, FX amp -1.2. Opt-in paths move this a lot: Vox bank -12 dB (to a relative -9), firmware filter +10 dB (synth relative -4.3).

## Ranked gaps per engine (worst first)

Ranking criterion: size of the spectral/level/width gap that survives every variant, then the variant effect. Spectrum score for context: lower is better.

1. **Synth (LP24 lead), default ZDF filter. Score 4.93, worst single gap.**
   - Presence 1.5..3 kHz is **-16.5 dB** and brilliance -6.9 dB: emulator far darker than hardware. The `FilterEngine::Firmware` LP4 path narrows it only to -15.2 / -5.8 dB (score 4.46). The performer's cutoff/filter moves are not replayed, so part of this is control mismatch, but 15 dB is more than a typical knob offset.
   - Level -14.6 dB re the other engines with the ZDF filter; the firmware filter brings it to -4.3 dB (filter gain/normalisation differs by about 10 dB between the two paths).
   - Width 0.09 vs 0.28, attack about 2x slower (42 vs 20 ms).
2. **Organ B3 (Legacy default). Score 4.83.**
   - Sub 20..120 Hz **+17.5 dB**: the hardware has almost nothing below 120 Hz (-22.5 dB re RMS) while the emulator carries -5 dB re RMS. Either the drawbar/low-note weighting, or a hardware high-pass/rotary-cabinet roll-off we lack.
   - Level +6.7 dB hotter than the engine median and the output **clips on 10 % of samples** (peak 0 dBFS, crest 5.6 vs 17.6 dB): organ level staging or the master limiter is not calibrated for chords played at velocity 100.
   - Presence -5.7 dB, width 0.25 vs 0.48.
   - `ToneWheelSource::B3Wheels`: score 4.69 vs 4.83 (sub -0.6, presence +0.6, brilliance +0.6). No meaningful change; level is identical. Not evidence for or against it, because the dominant gaps (sub, level) are outside the tone source.
3. **Organ Vox. Score 5.68 default, 7.66 with `ToneWheelSource::Vox`.**
   - Mid 0.4..1.5 kHz **+13.4 dB** (default) / +11.3 dB (Vox bank), the largest band error of any clip; centroid -606 Hz (default) and -921 Hz (Vox).
   - The Vox bank is 12 dB quieter than the legacy Vox and removes the clipping, which is closer in level, but it is 2.5 to 3.5 dB darker in presence/brilliance (-8 dB each) and has 4 dB less sub. The hardware clip includes the Twin amp (treble shelf) and organ reverb, which the emulator applies in both variants, so the Vox bank's darkness is more likely a missing amp/post stage than the bank itself.
   - Only 36 confirmed notes in 11 s: lowest confidence of the six.
4. **Organ Pipe2 (Firmware bank default). Score 4.95.**
   - Brilliance 3..8 kHz **-11.2 dB**, presence -5.9 dB: emulator dark. `legacy-tone` (the pre-bank waveform) scores better (3.99; brilliance -9.2, presence -4.3), so the firmware 92-wheel bank is *less* like the hardware recording than the legacy waveform. This segment also has the synth section on, so the hardware clip may contain synth content we do not render; treat as a warning, not a verdict.
   - Width 0.33 vs 0.74 (largest relative width gap); level -0.9 dB re median (fine).
5. **Piano (dry Grand). Score 2.70, the closest engine.**
   - Brilliance +6.4 dB, presence +3.1 dB: emulator brighter, consistent with the earlier A:25 finding (+5 dB above 1.5 kHz). Mid -1.6 dB, centroid -26 Hz, width 0.47 vs 0.46 (match), attack 68 vs 87 ms. Level relative +1.1 dB.
6. **Amp FX (Farfisa + Small II + EQ + reverb). Score 2.51, closest spectrum.**
   - Sub +4.4 dB, brilliance +3.8 dB, others within 2.3 dB; centroid +127 Hz. Both are nearly mono (0.09 vs 0.06). Attack 42 vs 22 ms. This is the best evidence so far that the Small II voicing and EQ are in the right place; the +3.8 dB brilliance repeats the "3 to 5.5 dB brighter above 3 kHz" pattern noted for three earlier clips.

## Effect of the opt-in firmware paths

| Path | Segments | Measured effect |
|---|---|---|
| `ToneWheelSource::B3Wheels` | organ_b3 | score 4.83 to 4.69; sub and level unchanged |
| `ToneWheelSource::Vox` | organ_vox | -12 dB level (closer), but presence/brilliance -2.5 to -3.5 dB (further), score 5.68 to 7.66 |
| `ToneWheelSource::Firmware` (Pipe default) vs `Legacy` | organ_pipe | firmware bank is darker: score 4.95 vs 3.99 for legacy |
| `FilterEngine::Firmware` | synth_lead | presence +1.3, brilliance +1.1 dB, level +10 dB, score 4.93 to 4.46 |
| `FirmwareFx::ALL` (delay laws + compressor) | all six | no change (score delta below 0.02): none of these programs enables the delay or compressor, so the segments do not exercise it; the Amp drive, tone stack, all-pass and delay line are not switchable yet |

## Caveats

- One performance per program; controls (knobs, wheel, pedal) are not replayed. Sustain pedal is not detected, so decays are cut at the detected note-off.
- Velocity is fixed at 100; level deltas are only meaningful as a spread across engines.
- Note detection is heuristic. Confirmed-only keeps 21 to 74 % of detections; ghost notes at low MIDI numbers remain possible, which could inflate emulator sub energy (B3, +17 dB).
- The attack metric is unreliable for overlapping chords (the hardware figure of 107 ms on B3 chords is probably envelope overlap, not key attack). Use it only for monophonic lines (synth_lead) as a hint.
- The `render-patch` output is 1 s longer than the 25 s hardware slice (release tail after the last event); RMS is slightly low for the emulator because of it (less than 0.2 dB at these lengths).
- No provenance tags are upgraded: a live YouTube recording cannot make a constant `VERIFIED`. Existing `ASSUMED` Legacy organ, LP24 and amp constants stay `ASSUMED`; the measurements below are `EMPIRICAL` pointers only.

## Implications

- Biggest gains by engine: (1) synth filter gain/brightness (ZDF is 15 dB darker than hardware at 1.5..3 kHz and 15 dB quieter than the other engines); (2) organ B3 low-end/level staging and clipping; (3) Vox mid bump and amp stage; (4) Pipe treble.
- Opt-in paths are not yet strictly better: `FilterEngine::Firmware` helps (score and level), `B3Wheels` is neutral, `ToneWheelSource::Vox` and the Pipe firmware bank are darker than the hardware recording. Do not flip any of them to default on this evidence.
- Piano and Amp FX are close enough (score about 2.5) that further tuning should wait for calibrated note velocity.
- The `FirmwareFx` switch cannot be judged until a segment that uses delay or compression is added.

## Follow-ups

- Replay controls: detect OLED pop-ups and the mod wheel to drive `--cc` events, and fit per-segment velocity from hardware attack levels, then re-run (`uv run python tools/emu_vs_hw.py --video ... --backup ... --workdir <scratch> --confirmed-only`).
- B3: investigate the +17 dB sub and clipping first (output gain staging, hardware high-pass, left-hand low notes in the detector).
- Synth: compare ZDF and firmware filter gains on a sine/saw at fixed cutoff against the firmware (`synth-filter` golden) to explain the 10 dB level step, then re-check presence.
- Vox: render with the Twin amp bypassed and with the amp treble shelf off to split bank vs amp error; add a clip where a Vox program has no amp.
- Add segments for delay/compressor programs (e.g. Boxxx, A:28, Pipe1 plus delay) to exercise `FirmwareFx`.
- Replace the pure-Python spectrum in `video_ref.analyze_wav` with numpy; the 16-variant run takes about 6 minutes, nearly all in analysis.

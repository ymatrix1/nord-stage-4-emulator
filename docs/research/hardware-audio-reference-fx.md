# Hardware audio reference for FX validation (Bank A demo video)

**Checked:** 2026-11-11
**Sources:** `NEW NORD STAGE 4 - PROGRAMAS PRESETS DEMO (PART A) [_zksT69H30c].webm` (3840x2160, 41 min, overhead camera), `Nord Stage 4 Factory Restore RevK v1.66.ns4b`, `tools/video_ref.py`, `tools/audio_notes.py`, `tools/key_vision.py`.
**Question:** Can the emulator's per-layer FX (amp modes, EQ) be validated against hardware audio when the player plays chords, and what do the first results say?

## Method

1. **Program to timestamp:** `video_ref.py scenes <video> --storyboard out.png` compares the OLED title rows against each segment's reference frame (changed-pixel fraction, persistence check, NVDEC decode in 4 parallel chunks, ~2 min for the 41 min video) and finds every program change; names are read from the storyboard tiles. Result: `docs/research/data/video_part_a_programs.json` (67 entries, A:11 .. A:88) and the raw segments in `video_part_a_scenes.json`. An earlier 15 s sampling missed programs shown for less than 15 s and the last minute.
2. **Played notes:** `video_ref.py notes <video> --at 10:05 --duration 20 --events-out ev.json`.
   - Audio: Hann STFT (8192/2048), harmonic-sum salience over MIDI 28..100, iterative pick-and-suppress (max 8 notes/frame), temporal smoothing into events. EMPIRICAL/heuristic; validated only on synthetic tones (`tools/tests/test_audio_notes.py`).
   - Video: skin mask on the cropped keyboard strip, fingertip = local minimum of topmost skin pixel per column, mapped to the 73-key geometry (E1..E7). Geometry constants measured by eye on one frame (+-1 key).
   - An audio event is `confirmed` when a fingertip is within +-1 semitone in >= 30 % of the frames around it.
3. **Emulator render:** `ns4 render-patch "<entry>" --backup <ns4b> --events ev.json [--confirmed-only]` replays the same notes (release build; the debug build overflows the main-thread stack when constructing the reverb buffers).
4. **Compare:** `video_ref.py compare hw.wav emu.wav` (6-band spectrum, centroid, width, RMS).

## Result: A:25 Sidechain Whl II (piano_a = LP24, Bambino Upright + Super Saw), 20 s at 10:05

| Metric | Fixed 6-note chord | Detected notes (all) | Detected, video-confirmed |
|---|---|---|---|
| Centroid delta (emu - hw) | +311.9 Hz | +32.7 Hz | +9.6 Hz |
| low 120..400 Hz | +0.32 dB | +1.56 dB | +1.98 dB |
| mid 400..1500 Hz | +2.43 dB | +0.78 dB | +1.13 dB |
| presence 1.5..3 kHz | +11.33 dB | +5.06 dB | +5.08 dB |
| brilliance 3..8 kHz | +9.38 dB | +5.50 dB | +4.54 dB |

The audio detector reports many notes at MIDI 28 and 29; the video footprint (left hand over keys 29..36, right hand over 54..64) agrees that the left hand plays in that low region.

## Single-layer organ programs (detected, video-confirmed notes; emulator minus hardware)

| Program (amp mode) | Clip | mid 0.4..1.5k | presence 1.5..3k | brilliance 3..8k | air 8..20k | centroid |
|---|---|---|---|---|---|---|
| A:64 Tiny Transistor (organ, Small II) | 30:17, 25 s | -0.37 dB | 0.00 dB | +3.30 dB | +6.17 dB | +27 Hz |
| A:84 Vintage Vox J3 (organ, Twin) | 39:31, 12 s | +3.11 dB | +0.23 dB | +5.48 dB | -8.37 dB | +28 Hz |
| A:78 Jazz Guitar II (synth, Twin) | 37:35, 20 s | invalid | invalid | invalid | invalid | -271 Hz |

- **Jazz Guitar II is not a valid FX test:** the emulator render is almost mono (width 0.12 vs 0.96) and silent above 1.5 kHz (-46 dB). Cause found: the sample resolves correctly (`Samp Lib/Guitar_Plucked/Jazz Guitar_ST 4.1.nsmp4`, shown by `render-patch`), but layer A has `filter on/off = off` (third-party `ns4decode` output; cutoff 554 Hz is stored but bypassed) and this branch's engine applies the 554 Hz LP24 anyway. The decode and wiring of the filter on/off flag (`filter_on`) exists on `claude/synth-params-dsps-0x2f-b7b76d` (not yet in `main`). Re-run this comparison after that branch merges.
- Level difference is large (emulator +6 to +8 dB RMS on the organ clips); organ drawbar levels are not calibrated here.

## Caveats found while working

- The player touches knobs, sliders and the mod wheel during clips (OLED pop-ups such as `Filter Frequency 5.6 kHz`, `Level Synth A Off`). Renders with fixed controls therefore differ from hardware in those windows. Follow-up: detect OLED pop-ups (`scenes --rows 0,1`) and the wheel position to drive CC events.
- Across three different clips and amp modes the emulator is consistently 3 to 5.5 dB brighter than hardware above 3 kHz. Common to LP24, Twin and Small II programs, so a shared stage (master/cabinet/DAC low-pass) is the first suspect, not one voicing. Three clips is not enough to tune constants.

## Implications

- Replaying the performed notes removes most of the spectral bias of a fixed chord (centroid error 312 Hz to 10 Hz).
- The remaining +5 dB excess above 1.5 kHz is not a note-choice artifact. It is consistent with a stronger treble roll-off on hardware (LP24 cutoff/slope, the Super Saw's own filter, or reverb damping), and **does not yet validate** the `ASSUMED` LP24/HP24 topology or the cabinet voicings.
- One 20 s clip, mixed layers (piano LP24 + synth): attribution to a single FX block is not possible.

## Follow-ups

- Pick single-layer programs with one dominant FX setting (e.g. *Silverface Mk I* piano Twin, *Jazz Guitar II* synth Twin with +15 dB bass) and repeat.
- Use moments where the player mutes a layer (OLED shows `Level Synth A Off` in *Boxxx* at 13:00) to isolate layers.
- Validate note detection against a known performance (MIDI-recorded) before trusting low-register results.
- Tune the key-vision geometry per video; pressed vs resting fingers are not distinguished.

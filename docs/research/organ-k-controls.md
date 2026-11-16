# Organ `K` per engine: measurement controls (D113)

- **Checked:** 2026-11-15
- **Sources (local, never committed):** Part A video clips `_zksT69H30c` (B3 238 s, Vox 2360 s, Pipe 1094 s; cached `hw.wav` per segment), the video-detected events (`events.json`, with `confirmed` flags), the Transkun stream `docs/research/data/piano_events__zksT69H30c.json`, release `ns4` renders made by `tools/emu_vs_hw.py --spec docs/research/data/organ_k_segments.json`, analysis by `tools/organ_k_controls.py` (+ `tools/organ_k_dsp.py`). Derived numbers: `docs/research/data/organ_k_controls_transkun.json`, `organ_k_controls_video.json`. Background: [organ-output-law](organ-output-law.md) s6.3, [organ-output-level](organ-output-level.md), D85, D106, OQ24.
- **Question:** the per-engine `K` needed on the Transkun streams spans 13 dB (Vox 1.7 to 1.8, Pipe 3.6 to 6.2, B3 4.9 to 8.4). How much of that is a measurement effect (band, sub, horn hole, silence, note range, stereo) and how much is the engine?

Tags: **VERIFIED** = reproduced by a command here, **EMPIRICAL** = measured on lossy YouTube audio, **ASSUMED** = reading.

## 1. Method

`k_needed = k_used * 10^(-delta/20)`, `delta` = emulator minus hardware level (dB), level = mean power over both channels (the same definition as `video_ref.analyze_wav`). The renders are `K = 2.8` (`--organ-level firmware`) and chain 0.597 (`--organ-level chain`) per engine; B3 uses `b3wheels` with `--firmware-routing` and the three rotary drive laws, Vox `--tone-source vox`, Pipe organ-only (sections, mod and reverb cut). Controls, each applied identically to emulator and hardware:

| Control | What it removes |
|---|---|
| `broadband_mono` | nothing; mono downmix (the first-pass reading) |
| `broadband` | nothing; stereo power |
| `band` | 160 Hz to 1.3 kHz: sub (below note 48, 130.8 Hz), the B3 horn hole (1.5 to 3 kHz) and everything above |
| `band_wide_nohole` | 160 Hz to 8 kHz with 1.5 to 3 kHz cut |
| `active` | `band`, frames (50 ms) within 45 dB of the loudest in both clips |
| `notes` | `band`, frames where a stream note sounds (+0.25 s release) |
| `matched_48up` / `matched_48_64` | `band`, frames where a note in the range sounds and no note outside it does (same frames on both clips) |

**Reproduction check (VERIFIED).** The `broadband` column reproduces D106 for Vox (+3.75 vs +3.99 dB) and Pipe (-7.09 vs -6.96 dB). B3 reads 1.4 dB hotter (the D106 variant flags were not recorded; `--firmware-routing` is my choice) and the Pipe chain render 2.6 dB lower. An independent hand script (plain numpy, FFT band 160 to 1300 Hz) gives `k_needed` 4.21 / 1.44 / 7.07, identical to the tool for B3 engine drive, Vox and Pipe. `agy` (own script in an isolated directory, same method) also reproduced them: B3 -3.55 dB / 4.21, Vox +5.77 / 1.44, Pipe -8.04 / 7.07 in band and 4.14 / 1.82 / 6.34 broadband.

## 2. K needed per engine under each control (Transkun stream, `K = 2.8`)

| Control | B3 engine drive | B3 unity drive | Vox | Pipe | Spread (B3 engine, Vox, Pipe) | Frames kept (B3 / Vox / Pipe) |
|---|---|---|---|---|---|---|
| `broadband_mono` | 3.92 | 6.79 | 1.66 | 5.09 | 9.7 dB | 1 / 1 / 1 |
| `broadband` | 4.14 | 7.17 | 1.82 | 6.34 | 10.8 dB | 1 / 1 / 1 |
| `band` | 4.21 | 7.30 | 1.44 | 7.07 | 13.8 dB | 1 / 1 / 1 |
| `band_wide_nohole` | 4.28 | | 1.79 | 7.04 | 11.9 dB | 1 / 1 / 1 |
| `active` | 4.15 | 7.19 | 1.42 | 6.03 | 12.6 dB | 0.82 / 0.88 / 0.74 |
| `notes` | 4.18 | | 1.42 | 6.56 | 13.3 dB | |
| `matched_48up` | 4.15 | 7.19 | 1.40 | 6.23 | 13.0 dB | 0.82 / 0.51 / 0.23 |
| `matched_48_64` | 5.00 | | 2.17 | 7.70 | 11.0 dB | |

The same renders with the video-confirmed notes at uniform velocity 100 (`--confirmed-only`, no Transkun):

| Control | B3 engine drive | B3 unity drive | Vox | Pipe | Spread |
|---|---|---|---|---|---|
| `broadband` | 3.73 | 6.46 | 3.06 | 2.53 | 3.4 dB |
| `band` | 3.50 | 6.06 | 2.30 | 2.69 | 3.7 dB |
| `active` | 3.44 | 5.95 | 2.30 | 2.69 | 3.5 dB |
| `matched_48up` | 5.47 (0.4 % of frames) | | 2.34 | 3.51 (21 %) | 7.4 dB, invalid |

## 3. What each control explains

| Control | Part of the 13 dB spread it explains | Reading |
|---|---|---|
| (1) band, sub and horn hole out | **none**: spread 10.8 (stereo broadband) to 13.8 dB; Vox moves -2.0 dB, Pipe +1.0 dB, B3 +0.1 dB | the sub (+14 dB on B3 in the older notes) is already gone with a 3-engine RMS; the horn hole is above the band |
| (1b) mono downmix | 1.1 dB of the spread: Pipe reads 1.9 dB too quiet in mono (hardware stereo width 0.74, emulator 0.00) | the `video_ref` stereo power is the right unit |
| (3) active frames, note-active frames | up to 1.2 dB (Pipe 7.07 to 6.03) | silent gaps hold no energy to be missed |
| (4) matched range, both clips | none; also unusable on the video notes (B3 notes are all below 48, 0.4 % of frames kept) | do not cut only the emulator notes (D106 note-range rows over-read by 1.5 dB) |
| (2) linearity | none for the spread; removes the "nonlinear" reading (s4) | |
| **Stimulus: video-confirmed notes at velocity 100** | **13.8 dB to 3.7 dB (10 dB)** | the replay stream, not the engine |
| Stimulus: Transkun rescaled to video concurrency (ASSUMED) | 13.8 dB to 6.7 dB (B3 3.1, Vox 1.43, Pipe 2.81) | see s5 |

## 4. Compression and drive (control 2)

Level slope `d(emulator dB) / d(20 log10 K)` between `K = 2.8` and chain 0.597 (13.4 dB of gain; 1.0 = linear):

| Engine | Stream | Slope broadband | Slope band | `k_needed` gap band (dB) | Crest delta band (dB) |
|---|---|---|---|---|---|
| B3, unity drive | Transkun | 0.83 | 0.97 | 0.48 | -1.0 |
| B3, unity drive | video | 0.95 | 0.98 | 0.25 | -1.4 |
| Vox | Transkun | 0.98 | 0.97 | 0.43 | 0.0 |
| Vox | video | 0.995 | 0.99 | 0.11 | +0.2 |
| Pipe | Transkun | 0.83 | 0.99 | 0.09 | +0.1 |
| Pipe | video | 0.97 | 1.00 | 0.03 | 0.0 |

In 160 Hz to 1.3 kHz all three engines are linear in `K` within 3 %, B3 included. The broadband "non-linearity" of D106 (2.2 dB `k_needed` gap, Pipe crest delta -10 dB) is content outside the band that does not scale with `K` between the two renders (the `air` band reads +28 dB re RMS on the chain render against +17 dB on the `K = 2.8` render); it is not a soft clip or a compressor in the level path. **No Leslie or rotary drive nonlinearity is shown** at the level range of `K`; the B3 rotary drive law (a different gain curve inside the rotary, not `K`) is a separate, real 4.8 dB effect (video band: unity 6.06, engine 3.50).

## 5. Stimulus: note density, duration, concurrency (control 3)

| Segment | Stream | Notes | Notes / s | Mean duration (s) | Mean concurrent notes | Mean velocity |
|---|---|---|---|---|---|---|
| B3 (25 s) | Transkun | 525 | 21.0 | 0.052 | 1.09 | 52.6 |
| B3 | video confirmed | 101 | 4.0 | 0.498 | 2.01 | uniform 100 |
| Vox (11 s) | Transkun | 59 | 5.4 | 0.313 | 1.68 | 44.4 |
| Vox | video confirmed | 36 | 3.3 | 0.519 | 1.70 | uniform 100 |
| Pipe (30 s) | Transkun | 202 | 6.7 | 0.128 | 0.86 | 64.9 |
| Pipe | video confirmed | 259 | 8.6 | 0.633 | 5.47 | uniform 100 |

The Transkun stream fragments held organ chords (B3 525 notes of 52 ms: the Leslie re-triggers of [organ-preamp-search](organ-preamp-search.md) s6.1) and under-counts Pipe concurrency (0.86 against 5.5). Incoherent-sum scaling `K_matched = K_needed * sqrt(poly_t / poly_v)` (**ASSUMED**: no phase, Leslie or FX coupling) gives B3 3.10, Vox 1.43, Pipe 2.81 in band, against the video 3.50, 2.30, 2.69: B3 and Pipe agree within 1.1 dB, Vox does not (Vox concurrency is equal in both streams, so its 4.1 dB residual (1.43 against 2.30) is not density; the note sets differ, 59 against 36 notes, and the organ may not scale with the Transkun velocity at all). Organ velocity use in the engine was not checked here.

## 6. Verdict

- **Single `K` or per engine?** On the stimulus that matches the hardware performance best (video-confirmed notes), `K` needed is 3.5 (B3), 2.3 (Vox), 2.7 (Pipe), a 3.7 dB spread, each within 2 dB of 2.8 and inside the B3 rotary-law choice (4.8 dB). A single `K = 2.8` is the supportable value. A per-engine `K` with a rotary-drive cause is **not justified**: the level path is linear in `K`, and the 13 dB spread vanishes (to 3.7 dB) when the replay matches the played notes.
- **What it does not settle:** the absolute `K` (EMPIRICAL, hardware clip level and lossy audio), the Vox residual on the Transkun stream (4.1 dB), the velocity scale of the organ, and the PNO lane to FX hand-over (OQ24). The Piano to Organ program-recall ARM capture and the single-note recording per engine remain the only routes to a firmware `K`.
- **Recommendation (no default changes, D22):** keep `K = 2.8` and `Chain` opt-in; do not add per-engine `K`; for future level checks use video-confirmed or concurrency-matched notes and a 160 Hz to 1.3 kHz stereo-power band, apply any note-range cut to both clips, and report the B3 rotary drive law next to `K`.

## Implications

- OQ24 narrows: the spread is not an engine gain; the open item is the absolute level, not its per-engine split.
- `emu_vs_hw.py --events-from transkun` is a poor level reference for organs (fragmented held notes); use `--confirmed-only` for `K`.

## Follow-ups

1. Replay the Transkun stream with notes merged across Leslie re-triggers (hold to the key-up), then re-run `organ_k_controls.py` to see whether the Vox and B3 residuals close.
2. Check whether the engine organ level depends on velocity (a uniform-velocity replay of the Transkun notes against the per-note one).
3. A single-note hardware recording per engine with a known pedal position (OQ24).

# Synth scale source hunt (OQ7): videos, manuals, forums

**Checked:** 2026-11-12
**Sources:** NS4 User Manual v1.6X (local `.reference` PDF, text via `pdftotext`); web searches; midi.guide Nord Stage 4 page; Nord User Forum threads (search snippets only, the site returns HTTP 403 to automated fetchers and to the in-app browser, so no thread was opened); Nord Wave 2 manual v1.2X (official PDF, 10 MB+, not downloaded); Kraft Music / Syntorial / Nord listings.
**Question:** Where can the remaining ASSUMED Synth scales (glide rate, unison cents, osc-envelope pitch depth, velocity steps, vibrato depth) be confirmed without owning a Stage 4?

## Result summary

| Scale | Found? | Evidence | Status |
|---|---|---|---|
| Vibrato rate range | yes | NS4 manual p.37 "Vibrato Menu": Rate `2.0..8.0 Hz`, Amount `0..10`; Wave 2 manual v1.2X says the same (`5.5 Hz` default) | range confirmed by the manual; linear 128-step mapping stays ASSUMED |
| Vibrato delay times | partly | Manual display lists `0.2 s, 0.7 s, 1.2 s, 2.0 s` steps; ns4decode formula `0.1 + 2.9*raw/29` s (0.1..3.0 s) | consistent, not firmware |
| Sample pitch tune | yes | Manual: coarse `-24..+24` semitones, fine `+/-50 cents` (Samples mode) | consistent with corpus coarse `+/-24`; fine corpus range `+/-17` |
| Glide rate law | qualitative only | Manual: "constant rate" (the greater the distance, the longer the glide); Legato glides only when overlapping; forum users report the NS4 maximum is much shorter than NS3 (user reports, no numbers; some call it a bug, firmware 1.06 vs later) | law EMPIRICAL (manual), scale still ASSUMED |
| Unison cents (Synth) | no | Manual: "1 = classic dual-oscillator detune with subtle stereo width; 2 and 3 add more oscillators and widen the stereo image"; Wave 2 spec: 3 amounts; no cents anywhere | ASSUMED (3.5 / 7 / 11.5 cents) |
| Osc-env to pitch depth | no | forum bug thread exists (could not open it); no semitone range in manuals | ASSUMED |
| Velocity steps | partly | Manual: amp velocity has 4 settings (off + 3), filter/osc env velocity on/off | structure confirmed, curve ASSUMED |
| MIDI mapping of the raw ranges | yes | midi.guide: Glide rate CC 48, Unison NRPN 3/18, Vibrato mode NRPN 3/19, delay NRPN 3/20, amount CC 45, rate CC 46, LFO waveform NRPN 3/80, destination 3/81, filter env amount CC 73 (all `0..127`) | confirms every decoded field is a 7-bit value; no physical units |

Also learned: the synth engine of the Stage 4 is the **Nord Wave 2** engine (new third layer, Enhanced Sample Unison), so Wave 2 videos and manuals are valid sources for the same scales; Stage 3 videos are not (Lead A1 engine).

## Candidate videos and sources (queue)

Useful only if a single dry sound is played while the panel value is visible.

| Candidate | Why | Access | Status |
|---|---|---|---|
| Syntorial 2 "Nord Stage 4 Lesson Pack" (lessons include a Unison gallery) | structured per-parameter demos with the value on screen | paid product (Plugin Boutique); user does not own this pack | not available |
| Kraft Music "Nord Stage 4 - Advanced Tutorial with Chris Martirano" (Jan 2024, chapters Intro / Specifications / What's new) | possible synth-section segment | YouTube | unchecked content |
| Nord NAMM 2023 "Nord Stage 4 product demo and overview" | official, may show Unison on a synth pad | YouTube | unchecked |
| Wave 2 "No Talking" / OctaveRange synth demo, Nord "The sound of the Nord Wave 2" (J3PO) | same engine, dry sounds, Unison button | YouTube | unchecked |
| `_zksT69H30c` Part A (already local) | 7 factory glide leads with known settings (`Smooth Lead` glide 29, `Space Drama II` 88, `Phat 3 Osc MiniM` 14, `Filter Bank Ld` 27..36, `Pinky Lead` 26 with vibrato DLY) | local `.webm` + `docs/research/data/video_part_a_programs.json` (other branch) | pitch contour of `Smooth Lead` measurable (autocorrelation, confidence ~0.97) |
| Wave 2 manual v1.2X PDF | repeats the same synth chapter, may add ranges | official PDF over 10 MB | not downloaded |

## Dead ends

- Nord User Forum: HTTP 403 for `WebFetch` and for the in-app browser. Threads worth reading by hand: *Nord Stage 4 - Glide Rate Range (Bug?)*, *Bug - NS4 OSC ENV with ENV to PITCH*, *Different glide types for NS4*, *Nord Stage 3 VS Nord Stage 4*.
- Firmware: the glide / unison / envelope-pitch scales run in the SHARC DSP images that are not in the update file (`docs/research/synth-0x2f-ns4decode-crosscheck.md`).

## Implications

- Two scales improve from ASSUMED: vibrato rate range and amount range are manual-documented (`synth_preset.rs`, `synth_params.rs` tags updated).
- The only data source that can settle glide with the material we already have is the local demo video; unison, env-pitch depth and velocity need hardware or a clean third-party recording.

## Follow-ups

- Build the video glide measurement in `tools/sweep_analyze.py` (robust f0 tracker, glide-segment finder, program to timestamp lookup).
- Read the four forum threads by hand and record any numbers here.
- If the user finds a Stage 4 or Wave 2 owner, run `docs/research/synth-hardware-sweep-plan.md`.

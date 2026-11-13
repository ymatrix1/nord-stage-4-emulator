---
# ns4-t5zr
title: 'Validate FX against hardware video: Bank A presets A/B audio comparison'
status: in-progress
type: task
priority: normal
created_at: 2026-11-12T01:46:46Z
updated_at: 2026-11-12T02:31:05Z
parent: ns4-trkb
---

## Context
tools/video_ref.py can extract OLED crops and WAV slices from the NS4 preset demo video (_zksT69H30c). OLED storyboard shows program names; A:11..A:25 mapped for 0..10 min.

## Higher Goal
Replace ASSUMED FX constants (cabinet voicings, shelf corners, Q) with EMPIRICAL ones from hardware audio.

## Acceptance Criteria
- [ ] Map Bank A programs to video timestamps for whole video
- [ ] Pick programs with strong EQ / amp mode / reverb / delay
- [ ] Render each via ns4 render-patch and compare 6-band spectrum with video_ref.py compare
- [ ] Record results in docs/research and update provenance tags

## Out of Scope
Sample-accurate waveform match (live performance, room, different samples).


## Progress (2026-11-11)
- [x] Played-note detection: tools/audio_notes.py (audio) + tools/key_vision.py (fingertips) + `video_ref.py notes`
- [x] `ns4 render-patch --events <json> [--confirmed-only]` replays detected notes
- [x] First comparison (A:25 Sidechain Whl II): centroid error 312 Hz -> 10 Hz; remaining +5 dB above 1.5 kHz (docs/research/hardware-audio-reference-fx.md)
- [ ] Single-layer programs (Silverface Mk I, Jazz Guitar II) and layer-mute moments


- [x] Jazz Guitar II invalid: filter on/off flag ignored (sample resolves). Fix lives on claude/synth-params-dsps-0x2f-b7b76d (filter_on); re-run after merge.
- [x] ns4 render-patch now prints resolved sounds (piano/synth refs).

## Progress (2026-11-13)
- [x] Six single-engine segments compared per engine with `tools/emu_vs_hw.py` and the new opt-in `render-patch` flags (`--tone-source`, `--filter-engine`, `--firmware-fx`): `docs/research/emulator-vs-hardware-audio.md`. FirmwareFx not exercised (no delay/compressor in the chosen programs); Farfisa+Small II amp spectrum within 4.4 dB of hardware in all bands.

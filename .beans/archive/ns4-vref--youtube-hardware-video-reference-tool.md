---
# ns4-vref
title: 'Build reusable YouTube/local hardware video & audio reference workbench (tools/video_ref.py)'
status: completed
type: task
parent: ns4-trkb
created_at: 2026-11-11T21:31:00Z
updated_at: 2026-11-11T21:39:00Z
---

## Context
Hardware preset calibration (e.g. against Filipe Martins' 4K Nord Stage 4 preset demo `https://www.youtube.com/watch?v=_zksT69H30c`) required repeating ad-hoc `ffmpeg`/`ffprobe`/Pillow crop and WAV FFT scripts across sessions.

## Higher Goal
Provide a reusable, tested Python CLI tool in `tools/video_ref.py` that locates or downloads YouTube/local hardware demo videos, auto-detects the Nord Red top panel, crops section/OLED/keybed ROIs, builds preset storyboards, extracts 48 kHz WAV slices, and compares 6-band spectral/stereo telemetry between hardware and emulator renders.

## Acceptance Criteria
- [x] Register `https://www.youtube.com/watch?v=_zksT69H30c` (`_zksT69H30c`, alias `part_a`) in `tools/video_ref.py` (`sources`, `fetch`, `probe`) with local cache discovery and `yt-dlp` support
- [x] Implement crimson-chroma Nord panel auto-detection (`detect_red_panel_box`) and panel-relative section ROI extraction (`extract`, `storyboard`)
- [x] Implement PCM + `WAVE_FORMAT_EXTENSIBLE` float WAV reader and zero-dependency FFT spectral/stereo analyzer + comparator (`analyze`, `compare`) with optional NumPy acceleration
- [x] Add comprehensive offline unit tests (`tools/tests/test_video_ref.py`) and verify against the real 4K `_zksT69H30c` demo video
- [x] Pass all Ruff, Pytest, `check_tests.py`, and Cargo workspace checks, update `docs/ARCHITECTURE.md`, commit, and push

## Out of Scope
- Bundling third-party YouTube videos or audio clips in git (`.reference/videos/` remains git-ignored)

## Summary of Changes
- Added `tools/video_ref.py` supporting `sources`, `fetch`, `probe`, `extract`, `storyboard`, `analyze`, and `compare`.
- Added unit tests in `tools/tests/test_video_ref.py`.
- Updated `docs/ARCHITECTURE.md` layout list to document `crop_panel.py` and `video_ref.py`.

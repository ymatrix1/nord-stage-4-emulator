# DSoundman synth LFO lesson: rate, shapes, filter depth (hardware vs emulator)

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `.reference/DSoundman-course/7-7-synthsection-workingwithlfos (2160p).mp4` and its `.srt` (93 s, 3840x2160, hardware Stage 4, OLED legible, mono-able 48 kHz audio). Tools: `tools/ag_frames.py` (OLED strips at 1 to 10 fps, native box `780,620,480,50`), `tools/ag_audio.py` (STFT), throw-away lock-in / sideband scripts on the 48 kHz WAV, example `ag_synth` (new keys `lfo`, `lfo_dest`, `lfo_rate`, `lfo_amt`, `notes`, `wf=pulse`). Method and earlier laws: [audible-genius-synth-lessons](audible-genius-synth-lessons.md).
- **Question:** What do the hardware LFO rate range, shape set, filter-cutoff depth and onset behaviour look like on a real Stage 4, and how far is the emulator `SynthLayer` LFO from them?

Tags follow `.agents/rules/provenance-in-code.md`. The audio is lossy YouTube audio; hardware and emulator numbers use the same analysis code.

## 1. What the clip contains

The `.srt` is 22 cues of speech; the demonstration is audio only (no speech 43 to 64 s and 80 to 88 s).

| Time | Content | Usable for |
|---|---|---|
| 0 to 42 s | speech, Synth section tour (OLED: AMP ENVELOPE 586 ms / 1.70 s / 3.22 s, waveform Pure Square) | nothing numeric |
| 42 to 64 s | LFO destination = Filter (button pressed 3 times, both OSC PITCH and OSC CTRL LEDs lit as the FILTER bracket), one held note (G4, 392 Hz), Mod Amt and Rate/Time knobs turned by hand; OLED overlay shows the value of the knob being moved | rate range, rate steady points, filter depth saturation |
| 55 to 57 s | Filter Frequency turned down (1.3 kHz to 196 Hz) | none |
| 70 to 80 s | LFO Waveform page: TRIANGLE, SAW, INV SAW, SQUARE, S/H, no notes | shape set and order (OLED only) |
| 80 to 92 s | S/H selected, Rate 45, 76, 33, 24 Hz shown while turning | none (no steady rate) |

Not demonstrated: pitch or amplitude destination, delay / fade-in, key sync, tempo sync (MST CLK LED visible, never used). Nothing about those can be measured from this clip; the earlier [Audible Genius vibrato](audible-genius-synth-lessons.md) section stays the only onset evidence (delay 0.7 s: onset 0.74 to 0.85 s, 0.4 s ramp).

## 2. LFO rate (VERIFIED display set, VERIFIED rate in Hz)

OLED readings (high confidence, two-line overlay "LFO Rate nn Hz"): 0.15, 0.16, 0.51, 0.55, 0.95, 1.0, 1.5, 2.2, 3.0, 4.1, 4.4, 5.6, 7.6, 8.2, 10, 13, 16, 21, 24, 30, 38, 45, 48, 56, 82 and 305 Hz (the maximum reached, 59.6 s). Every value is an entry of `LFO_RATE_HZ_TABLE` (firmware display strings, 0.03 to 523 Hz in 128 steps, [presets.rs](../../crates/nord-core/src/cbin/presets.rs)); test `synth::tests_osc_lfo::lfo_rate_table_contains_every_rate_read_off_the_hardware_oled`. Values such as 6.8 Hz that I first read were low-resolution misreads (the table has 6.5 and 7.0).

The knob angle could not be read: the hand covers the Rate/Time knob in almost every frame. A sweep down at 10 frames per second (18, 12, 10, 7.6, 6.5, 5.6, 4.1 Hz at 60.0 to 60.7 s) steps by constant ratios (0.86, 0.86), consistent with the log-spaced table at constant knob speed; the table is used for the knob mapping, nothing new.

Rate actually produced (the held note, filter LFO, 1 to 3 kHz band envelope, lock-in over 0.5 to 1.1 s windows and sideband spacing around 392 / 784 / 1176 Hz):

| Displayed | Hardware (lock-in) | Hardware (sidebands) | Emulator, same pipeline | Error hw vs display | Error emu vs display |
|---|---|---|---|---|---|
| 8.2 Hz (53 to 54 s) | 8.14 | 8.0 to 8.1 | 8.22 | -0.7 % | +0.2 % |
| 8.2 Hz (54.5 to 55 s) | 8.00 | | | -2 % | |
| 16 Hz (48.2 to 49.3 s) | 16.36 | 16.2 to 16.5 | 16.00 | +2.3 % | 0 % |

Resolution of the windows is 1 to 2 %, so hardware = display within measurement error: the LFO runs at the displayed Hz and the emulator `lfo_rate_hz` is the displayed Hz (unit test `lfo_runs_at_the_displayed_rate`: 8.2 and 16 Hz within 0.5 Hz over 2 s). No change to the emulator.

## 3. Shape set (VERIFIED on the OLED)

The waveform page lists exactly five shapes in this order: TRIANGLE, SAW (falling ramp, a line from top left down to the right), INV SAW (rising ramp), SQUARE, S/H (stepped random, 6 to 7 steps drawn). The decode in `cbin/synth_preset.rs` maps raw 0..4 to Triangle, SawDown, Saw, Square, SampleAndHold, so the hardware "SAW" is the emulator `SawDown` and "INV SAW" is `Saw`; the emulator has all five, no sine and no extra "random". The shape of the LFO that drove the filter demo (before 70 s) was not displayed; it is assumed TRIANGLE (the page opens on TRIANGLE at 70 s, before any change).

## 4. Filter-cutoff depth (EMPIRICAL, only a bound; not added to `measured_laws`)

The envelope of the 1 to 3 kHz band at 5 ms resolution (53.0 to 53.5 s, Rate 8.2 Hz, Mod Amt 6 to 9) is periodic with a 120 ms period: a bump about 60 ms wide, then a flat floor about 22 dB lower for about 60 ms; the 3 to 9 kHz band shows a flat top and a flat floor 11 dB lower. Both ends are saturated: the cutoff swings from above 9 kHz to below about 400 Hz around a base of 2.6 kHz (the last Filter Frequency overlay before the demo, 49 to 51.5 s). The swing in the 1 to 3 kHz band stays about 20 dB for every Mod Amt shown (4.4, 5.4, 6.1, 6.9, 7.5, 8.5, 9 to 10, 8.0 at 52.5 to 55 s), so the depth is already saturated at 4.4.

| Setting | Hardware (1 to 3 kHz swing, dB: lock-in peak-to-peak at the LFO rate, or p95 - p5) | Emulator default law `cutoff x (1 + 0.8 a lfo)` (lock-in, same band, square wave G4, LP24, 2.6 kHz) | Note |
|---|---|---|---|
| Mod Amt 4.4 | about 21 | 4.1 (a swing of -0.6 / +0.4 octave) | too shallow by a factor of at least 5 in octaves |
| Mod Amt 8 | 25 (p5 - p95 over 54.6 to 55 s) | 8.7 | |
| Mod Amt 10 | about 19 to 20 | 12.1 | |

The emulator's bipolar linear law reaches at most +0.85 / -2.3 octave at full amount and almost nothing at the lower amounts, while the hardware sweeps at least +1.8 octave (2.6 kHz to above 9 kHz) and -2.7 octave (to below 400 Hz) at Mod Amt 4.4. An exponential law of `6.2 x amount` octaves (the filter envelope scale of [audible-genius-synth-lessons](audible-genius-synth-lessons.md), applied peak = amount x 6.2 octaves) gives -2.7 octave at 4.4, which satisfies the bound, but as rendered it is too deep (1 to 3 kHz swing 34 to 81 dB against 20 dB measured) because the plateaus on the hardware sit only 11 to 22 dB below the peak while an emulator LP24 closes much further. Filter type, resonance and the exact cutoff are unknown for this patch, so the clip cannot pin an octave scale. Not committed to `measured_laws`.

The hardware also spends more time at the floor than at the peak (floor fraction 0.5 to 0.6 against 0.2 to 0.3 near the peak in the 1 to 3 kHz band), unlike a symmetric triangle through a symmetric exponential; this suggests the sweep is not centred on the displayed cutoff (the swing might be downward from the cutoff). One patch, so unresolved.

## 5. Implications

- Rate: no work left; the table is the hardware table and the Hz are real.
- Shapes: nothing to change. The only open shape question is the S/H step distribution (the emulator's hash, 6 to 7 steps are drawn on the OLED at 45 Hz).
- Filter depth is the defect: the emulator default depth is far too shallow at low and middle amounts. A hardware measurement with a known filter type, Res 0 and a ramp of Mod Amt 1, 2, 3 (below saturation) is the way to fit the law (see follow-ups).
- OQ7 / OQ22 keep their open items; only the note that rate and shape set are VERIFIED is added.

## 6. Follow-ups

1. Hardware sweep (or a clip) with Mod Amt 0.5 to 3 at a known Filter type / cutoff / Res 0 and the LFO Triangle to fit octaves per unit.
2. Pitch and amplitude destinations, delay / fade-in, key sync and tempo sync: no hardware evidence yet.
3. S/H step statistics and the LFO phase at note-on (free run versus key sync) need a long held note with a fixed rate.

## Reproduce

```bash
python tools/ag_frames.py "<clip>.mp4" --box 780,620,480,50 --fps 4 --every --start 42 --end 64 --cols 6 --scale 0.5 --out sheet.png
ffmpeg -i "<clip>.mp4" -vn -ac 1 -ar 48000 lfo.wav
cargo run --release -p nord-tools --bin ag_synth -- e.wav notes=67 wf=pulse cutoff=2600 lfo_rate=8.2 lfo_amt=8 hold=3 len=3.5 filter=lp24
```

# B3 and Vox presence gap: which mechanism is missing, and why no default flips

- **Checked:** 2026-11-14
- **Sources (local, never committed):** Part A demo video `_zksT69H30c` (segments `organ_b3` = `B3 Chords n Solo`, `organ_vox` = `Vintage Vox J3`, `fx_reverb` = `B3 Old Forest`), factory `.ns4b` v1.66, DSoundman drawbar clip `SuilmCpKeps`, `tools/emu_vs_hw.py` (`edits`), new `tools/organ_gap.py`, `ns4 render-patch` (this branch, release build). Builds on [b3-vox-video-gap](b3-vox-video-gap.md), [organ-output-law](organ-output-law.md), [organ-vox-click](organ-vox-click.md), [dsoundman-course-organ-piano](dsoundman-course-organ-piano.md). No audio is committed.
- **Question:** B3 (`b3wheels` + routing + firmware level) and Vox (`vox` + firmware level) read presence (1.5 to 3 kHz) 7 to 20 dB and brilliance (3 to 8 kHz) 10 to 13 dB under the hardware clips. Which mechanism (key click, percussion, a tone/tube stage, tonewheel crosstalk, the firmware tone routine, EQ, the post chain) is missing, and can a firmware-derived model be switched on?

Tags: **VERIFIED** = reproduced by the commands below, **EMPIRICAL** = measured on the lossy clips, **ASSUMED** = reading.

## 1. Answer in short

| Item | Result | Tag |
|---|---|---|
| Is there a firmware-derived stage that can be ported now? | **No.** Nothing in the ARM or PNO listings read so far (oracle-exact bank, output stage `0x1229B3`, boot gain word, tail table) carries 1.5 to 8 kHz content that the Rust models lack: the Vox bank equals the oracle within 0.16 dB, the B3 tail is a measured oracle response. No engine code, default or `render_hash` changed | VERIFIED (negative) |
| Vox: where the hardware 2 to 6 kHz energy comes from | The oracle-exact **dry** Vox bank is 15 dB (presence) and 25 dB (brilliance) under the hardware (`fw_noamp`: -15.0 / -24.9). The Twin amp (type 289) of the program lifts that to -7.5 / -9.7. The table is emulator minus hardware, so **7.5 dB (presence) and 9.7 dB (brilliance) are still missing after the amp**. A stronger amp drive (5) only gets -6.1 / -7.6 at +4 dB level: no amp level reaches the clip with this amp model, so the missing piece is the **amp / cabinet response** (FX port of type 289, [fx-type-map](fx-type-map.md)), not an organ-side stage | VERIFIED (ablation), amp attribution EMPIRICAL |
| Vox: is the bank tonal where the hardware is tonal? | Yes: the dry bank is more line-like than the hardware above 1.5 kHz (peakiness 24.6 / 26.3 dB against 19.4 / 19.0 dB), the chain with amp reads 19.3 / 24.1 dB. The bank has the harmonics but at the wrong level; the amp stage decides the level | VERIFIED (run) |
| B3: where the hardware 0.8 to 3 kHz energy comes from | Hardware reads **-9.5, -13.1 dB re total at 0.8 to 1.5 and 1.5 to 3 kHz** with line-like content (peakiness 24.3 / 22.0 dB); `b3wheels` reads -23 / -25 dB and **noise-like** (13.1 / 10.1 dB, rotary and reverb smearing, no lines). Legacy has the same hole (-24 / -24 dB, 14.9 / 10.8 dB). So the lines are absent from **both** tone sources | VERIFIED (run) |
| B3: stage ablation | Rotary off: presence -19.9 to -11.4, brilliance -13.4 to -7.0, but still no lines (peakiness 7.8 / 8.0 dB). Rotary drive 5 and 10 (`tanh` law): presence -19.3 / -19.2 dB, no change; the drive only adds level. Layer B off: presence +2.3 dB ([b3-vox-video-gap](b3-vox-video-gap.md)). Reverb, vibrato, firmware FX: below 3 dB | VERIFIED (ablation) |
| B3: the likely cause is the replayed note set, not a stage | The replay (`--confirmed-only`) holds 101 of 285 detected notes in octaves 3 to 7: **65 in octave 3, 31 in octave 4, 2 in octave 5** (MIDI 36 to 71). The solo and the layer A (octave +2, drawbars `8 0 8 0 2 ...`) register is not in it; the 72 octave-5 notes of the full detection are unconfirmed (ghosts and real notes mixed). The 0.8 to 6 kHz lines of a clip that is mostly right hand cannot come out of a left-hand replay | EMPIRICAL (note histogram, [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md) caveat) |
| Key click | not the mechanism: the clips' high bands are **sustained** lines (peakiness 19 to 24 dB over 25 s and 11 s), a click is a 2 to 10 ms transient. Legacy B3 (click on, extra harmonics) reads presence -7.7 against -19.9 dB on the firmware path, but its 1.5 to 6 kHz content is noise-like (peakiness 11 dB), which is why Legacy scores better and still has the 0.8 to 3 kHz hole (-25 / -28 dB against -13 / -12 dB) | VERIFIED (run) |
| Default flip (D22 bar) | **Not made.** `b3wheels` and `vox` lose to the current defaults on the score (B3 7.62 against 5.54, Vox 6.13 against 3.03, `fx_reverb` 5.24 against 3.76), and presence / brilliance are 7.5 to 20 dB out. No program sweep was needed because no default moved | VERIFIED (run) |

## 2. Per-clip table (`tools/emu_vs_hw.py`, `--confirmed-only`, velocity 100, emulator minus hardware, dB re RMS)

"before" is the current default; "after" is the best opt-in firmware path. Nothing changed in this PR, so before and after of the shipped defaults are identical.

| Clip | Variant | sub | low | mid | pres | brill | centroid Hz | RMS | attack hw / emu ms | T60 hw / emu s | score |
|---|---|---|---|---|---|---|---|---|---|---|---|
| B3 Chords n Solo | default (Legacy), before and after | +17.7 | +0.1 | -0.4 | -7.7 | -1.9 | -226 | +6.1 | 106.7 / 86.2 | 5.24 / 0.65 | 5.54 |
| B3 Chords n Solo | `b3wheels` + routing + firmware level | -1.2 | +1.2 | +2.3 | -19.9 | -13.4 | -132 | -2.7 | 106.7 / 96.0 | 5.24 / 0.65 | 7.62 |
| B3 Chords n Solo | same, rotary off | +17.2 | +1.6 | -1.1 | -11.4 | -7.0 | -230 | -2.2 | 106.7 / 88.5 | 5.24 / 0.64 | 7.66 |
| B3 Chords n Solo | same, rotary drive 5 / 10 | +16.8 / +16.7 | +1.4 | -0.9 / -0.8 | -19.3 / -19.2 | -12.1 / -12.3 | -230 | +4.6 / +8.0 | 106.7 / 88 | 5.24 / 0.65 | 10.10 / 10.09 |
| B3 Chords n Solo (all 285 notes) | default | +15.7 | -1.1 | +1.7 | -6.0 | -4.8 | -124 | +10.9 | 80.5 / 76.8 | n/a | 5.85 |
| B3 Chords n Solo (all 285 notes) | `b3wheels` + routing + firmware level | +3.8 | -0.8 | +3.1 | -15.6 | -9.4 | -68 | +2.3 | 80.5 / 88.1 | n/a | 6.55 |
| B3 Old Forest | default (Legacy) | +3.3 | +0.1 | -7.3 | +2.2 | +5.9 | +269 | +7.4 | 73.5 / 27.1 | 42.35 / 0.61 | 3.76 |
| B3 Old Forest | `b3wheels` + routing + firmware level | +2.9 | +1.6 | -6.4 | +1.6 | -13.8 | -220 | +0.1 | 73.5 / 20.5 | 42.35 / 0.61 | 5.24 |
| Vintage Vox J3 | default (Legacy) | -3.2 | -0.2 | +8.1 | -1.3 | +2.3 | +292 | +8.9 | 86.0 / 23.2 | 3.12 / 1.03 | 3.03 |
| Vintage Vox J3 | `vox` + firmware level | +2.7 | +3.2 | +7.5 | -7.5 | -9.7 | -1108 | -0.6 | 86.0 / 63.6 | 3.12 / 1.01 | 6.13 |
| Vintage Vox J3 | same, amp off | +2.7 | +2.4 | +10.9 | -15.0 | -24.9 | -1171 | -0.2 | 86.0 / 66.7 | 3.12 / 1.03 | 11.17 |
| Vintage Vox J3 | same, amp drive 5 | +2.2 | +2.6 | +7.5 | -6.1 | -7.6 | -1012 | +3.5 | 86.0 / 42.2 | 3.12 / 1.00 | 5.21 |
| Vintage Vox J3 (all 160 notes) | default | -1.9 | -4.2 | +4.4 | +0.1 | +4.0 | +987 | +16.3 | 87.0 / 33.1 | 3.16 / 0.95 | 2.92 |
| Vintage Vox J3 (all 160 notes) | `vox` + firmware level | +1.5 | +1.8 | +5.6 | -11.8 | -12.6 | -1174 | +7.7 | 87.0 / 82.6 | 3.16 / 0.94 | 6.65 |

The rotary drive rows also move the sub band by +18 dB (the `tanh` law adds a sub-bass rumble at high drive); the rotary file is owned by the Leslie work and was not touched. T60 is the median reverb decay of the clip (hardware is dominated by the room and the sustain). Attack: the Vox bank attacks 40 ms slower than the Legacy path (weights step on the 3 / 17 / 47 ms timeline), hardware 86 ms.

## 3. Long-term spectrum, dB re clip total (`python tools/organ_gap.py <segdir> <wavs>`)

Vintage Vox J3 (confirmed notes):

| band Hz | hardware | default | `vox` + level | `vox`, amp off | `vox`, amp drive 5 |
|---|---|---|---|---|---|
| 300-400 | -13.9 | -9.4 | -6.9 | -5.8 | -7.1 |
| 600-800 | -26.4 | -12.1 | -16.1 | -10.2 | -16.1 |
| 1500-2000 | -18.3 | -14.3 | -24.2 | -22.3 | -22.0 |
| 2000-3000 | **-5.4** | -8.6 | -13.9 | -27.1 | -12.6 |
| 3000-4000 | **-7.4** | -7.8 | -17.7 | -33.0 | -15.9 |
| 4000-6000 | -16.4 | -10.3 | -33.5 | -47.0 | -26.4 |
| 6000-8000 | -25.7 | -18.4 | -39.8 | -55.0 | -36.0 |

The hardware Vox puts its loudest band at 2 to 4 kHz. The amp is worth +13 dB there, the bank alone sits at -27 / -33 dB. Hardware peakiness 1.5 to 6 kHz: 19.4 / 19.0 dB; the emulator chain 19.3 / 24.1 dB.

B3 Chords n Solo (confirmed notes):

| band Hz | hardware | default | `b3wheels` + level | `b3wheels`, rotary off |
|---|---|---|---|---|
| 100-200 | -12.2 | -4.8 | -12.1 | -4.6 |
| 400-600 | -10.8 | -7.9 | -5.2 | -9.2 |
| 800-1000 | **-13.5** | -25.6 | -25.2 | -28.1 |
| 1000-1500 | **-11.7** | -27.6 | -27.4 | -27.1 |
| 1500-2000 | -14.8 | -25.8 | -27.1 | -25.7 |
| 2000-3000 | -18.0 | -29.6 | -29.5 | -25.4 |
| 3000-4000 | -24.5 | -26.8 | -35.8 | -31.8 |

Peakiness (95th percentile minus median bin level): hardware 24.3 / 22.0 / 20.3 dB at 0.8 to 1.5 / 1.5 to 3 / 3 to 6 kHz; `b3wheels` 13.1 / 10.1 / 9.7 dB; Legacy 14.9 / 10.8 / 11.2 dB; `b3wheels` rotary off 10.3 / 7.8 / 8.0 dB. A noise-like spectrum reads about 10 dB, so the emulator has no partial lines above 800 Hz on this replay while the hardware has strong ones.

## 4. Why no stage is added

1. **Vox.** The three firmware pieces that sit between the oscillator bank and the FX chain are verified and already in Rust: the bank, the output sections (low-passes at 344 Hz and 1.24 kHz of the image coefficients, the ARM writes only the gains `1.0 / 0`, [organ-model-paths](organ-model-paths.md) s4) and the swell word. They give the dry bank above. The hardware has 20 to 25 dB more energy at 2 to 6 kHz than that bank plus a Twin amp that is nearly linear at drive 1.1. The one component left that can supply a broad 2 to 6 kHz lift is the amp / cabinet of the program (type 289), which `FirmwareFx` does not port and which belongs to the FX lane. An organ-side fit (shelf or presence peak) would hide that and would be fitted on one clip: not added.
2. **B3.** No stage explains lines that the replay cannot excite: the lines at 0.8 to 6 kHz are partials of right-hand notes (the clip is a chord comping and a solo; the confirmed set is 96 of 101 notes in octaves 3 and 4). Adding a presence stage to `b3wheels` would be tuned to the wrong note set; Legacy only looks better because its click and its extra harmonics are noise that is not line-like (peakiness 11 dB).
3. **Tonewheel crosstalk / leakage**: not applied on the firmware sources ([organ-engine](../specs/organ-engine.md) 3.1.6, 3.1.7), but the DSoundman clip puts the hardware leakage at the -35 dB level (2nd partial of a lone 8' at -35.7 dB, [dsoundman-course-organ-piano](dsoundman-course-organ-piano.md) s2): far too low for a 12 to 18 dB band hole. The key click is on in the B3 firmware path and is a transient, see section 1.
4. **Percussion** is off in both programs (`percussion_on` is false in the decoded `B3 Chords n Solo` and `Vintage Vox J3`).

## 5. What the tool adds

`tools/organ_gap.py` makes the three checks above repeatable on any `emu_vs_hw.py` work directory: a 12-band table of the long-term spectrum, the tonal peakiness per band, and the per-octave histogram of the detected notes (all and video-confirmed). It reads only the files in that directory. Tests: `tools/tests/test_organ_gap.py`.

## Implications

- Do not tune the organ banks to the Part A B3 / Vox clips as they are replayed: the B3 verdict needs the right-hand notes, the Vox verdict needs the Twin amp.
- The next lever for Vox is the type-289 amp port (FX lane), then a re-run of this table with `--firmware-fx`; only then can the 7.5 / 9.7 dB residual be assigned to the organ.
- The B3 note set must include the solo register before presence is judged (OQ16): a confirmed-note filter that keeps notes above C4 once their harmonic lines are present in the audio, or a manual note list for the clip.

## Follow-ups

1. B3: a replay note list for `organ_b3` with the right-hand register (audio-line tracking of the 0.8 to 3 kHz partials), then repeat the table; if lines are still 12 dB short, the tail table with the type-5 vibrato (OQ16) is the next suspect.
2. Vox: the same table with a ported Twin amp; a Vox clip without an amp program (none known in the factory set or the course clips).
3. A clip of a lone Vox or B3 note with the amp / rotary off would separate bank from post chain; none of the local clips has one.

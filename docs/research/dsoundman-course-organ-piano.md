# DSoundman course clips: organ drawbars and soft piano

Checked 2026-11-14.

Sources: local, git-ignored `.reference/DSoundman-course/` (`Using the Organ Drawbars on the Nord Stage 4 [SuilmCpKeps].webm`, `How to Make a Soft Piano Sound [NVMtX0epXog].webm`, the two other piano clips only read through their `.srt` titles, not analysed), the auto-generated English subtitles next to them, `tools/dsm_partials.py` (new), `tools/ag_audio.py`, `ns4 render-patch` (release build of this branch), [organ-weights](organ-weights.md), [organ-output-law](organ-output-law.md), [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md). No video, audio or frames are committed. The audio is lossy (YouTube Opus), the instructor speaks over the playing and the rotary speaker is on, so every number is shape evidence.

Question: do the clips pin the drawbar level law (position 0..8) or the per-partial balance of the B3 tone source, and do they pin the piano timbre / level behaviour against the emulator?

## 1. Answer in short

| Question | Answer | Status |
|---|---|---|
| Model in the drawbar clip | **B3** only (OLED / panel: `B3` lit, later `B3 BASS` is shown but only talked over). No Vox, Farfisa or Pipe segment exists in the clip. It does **not** give B3-versus-Pipe parity evidence. | VERIFIED (panel frame at 112 s, 130 s) |
| Drawbar level law 0..8 | **Not measurable here.** The instructor only moves drawbars between 0 and 8 (all LED ladders full or off in every frame read). The firmware curve `0,3,6,13,22,39,62,87,127` is untouched by this clip. | VERIFIED (frames 190-211 s) |
| Partial balance, 8' + 4' (+ 2') on one held C4 | Hardware 4' is 3.6 to 4.5 dB below 8'; 2' is 2.3 dB below 8'. Windows are 0.6 to 1.2 s, speech overlaps, the slow rotary modulates: +-3 dB. | EMPIRICAL, low confidence |
| Emulator | Default tone source: 4' +1.8 dB (rotary on, +0.2 off), 2' -1.0 dB. `b3wheels`: 4' -0.1 dB, 2' -5.7 dB. `firmware`: same as default for 8'/4'/2'. No variant matches both partials; the 4' error is +3.5..+6.3 dB, the 2' error is +1.3 or -3.4 dB. | EMPIRICAL |
| Law applied behind a switch | **None.** The evidence is one note, three states, low confidence. | n/a |
| Piano program | White Grand (Grand class, `Piano/Grand/White Grand XL 6.3.npno` in the factory backup, name match only), after `Shift + Layer Init`, no effects, no EQ. | ASSUMED (mapping by name) |
| Piano timbre normal vs soft, dynamic compression | Long-term spectrum only (music, different notes): 2 to 4 kHz band falls by 2.4 to 5.7 dB relative to the total when Timbre goes to Soft; the level change is below 1 dB. Dynamic compression is not separable. | EMPIRICAL, low confidence |
| Piano emulator comparison | **Not done** (stop point, section 5). | open |

## 2. Organ drawbar clip, B3 (video `SuilmCpKeps`)

Segment (subtitle 3:10 to 3:35, "the white drawbars relate to our octaves"): the instructor holds **C4 (261.6 Hz, 260.7..262.6 Hz peak)** with the right hand (off camera) and pulls the white drawbars out one at a time with the left. Rotary on, slow, vibrato / percussion off (subtitle 1:25 to 1:50 set it up; percussion and vibrato are only turned on from 5:03).

Drawbar positions are read from 4K crops of the panel (columns 16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1'; a ladder is "8" when all eight LEDs are lit, "0" when none is). Every ladder read was either fully lit or dark.

| State | Video time | Drawbars at 8 (all others 0) | Position confidence |
|---|---|---|---|
| A | 192.3 to 193.8 s | 8' | high (LED ladders, frames 192 and 193) |
| B | 198.4 to 199.0 s | 8', 4' | high (frame 198) |
| C | 202.6 to 203.8 s | 8', 4', 2' | high (frame 202 to 205) |
| D | 207 to 211 s | 8', 4', 2', 1' | high for the drawbars, but the **note changed** (spectrum at 208 s peaks near 377 Hz, 16' and 5 1/3' are +11.8 dB above 8'): unusable for a one-note comparison |

Hardware partial levels, dB re the 8' partial (`python tools/dsm_partials.py <wav> --start .. --end .. --note 60`; power sum within +-0.8 % of each partial, Welch):

| State | 16' | 5 1/3' | 4' | 2 2/3' | 2' | 1 3/5' | 1 1/3' | 1' |
|---|---|---|---|---|---|---|---|---|
| A (8' only) | -33.8 | -46.9 | -35.7 | -52.3 | -41.0 | -49.8 | -44.3 | -46.5 |
| B (8' + 4') | -49.4 | -59.2 | **-4.5** | -52.8 | -37.1 | -56.5 | -55.6 | -56.9 |
| C (8' + 4' + 2') | -34.3 | -12.9 (speech) | **-3.6** | -22.7 | **-2.3** | -39.7 | -25.7 | -31.0 |

State A is the noise and speech floor: a drawbar that is off sits 34 to 52 dB below the 8' partial in this recording. State C has speech at 392 Hz and 1.5 kHz (5 1/3', 2 2/3', 1 1/3' read -13..-26 dB where the drawbars are off), so its 4' and 2' values may be raised by speech too: confidence low. B and C windows are shorter than one rotary period, which adds about +-3 dB.

Emulator, same note and drawbars (`ns4 render-patch` with the program's piano / synth layers disabled, C4, velocity 127, 4 s, analysis window 1.0 to 3.5 s, rotary slow):

| State | Variant | Rotary | 4' | 2' |
|---|---|---|---|---|
| B | default | on / off | +1.8 / +0.2 | n/a |
| B | `--tone-source b3wheels` | on / off | -0.1 / -1.7 | n/a |
| B | `--tone-source firmware` | on / off | +1.9 / +0.2 | n/a |
| C | default | on / off | +1.8 / +0.2 | -1.0 / +0.2 |
| C | `--tone-source b3wheels` | on / off | -0.1 / -1.7 | -5.7 / -4.5 |
| C | `--tone-source firmware` | on / off | +1.8 / +0.2 | -1.0 / +0.3 |
| C | hardware | on | **-3.6** | **-2.3** |

Gaps (emulator minus hardware, rotary on): default 4' +5.4 (B: +6.3), 2' +1.3; `b3wheels` 4' +3.5 (B: +4.4), 2' -3.4; `firmware` equals default. `--organ-level firmware` changes only the absolute level (abs 8' 60 dB vs 51 dB for `b3wheels`), not these ratios. `--tone-source b3wheels --firmware-routing` rendered **silence** in this organ-only render (all partials -200 dB): recorded as a finding, not investigated (the flag probably needs the firmware FX / piano path that the stripped program removes).

Reading: the hardware 4' sits clearly below the 8' while the emulator puts them level or above. The 2' fits the default / firmware tone source, the 4' fits neither; no variant is closer on both. With +-3 dB measurement error this does not justify changing a law, and it contradicts nothing in [organ-weights](organ-weights.md) (equal d7 on both drawbars gives equal weights; the 4 dB difference must then come from the per-wheel generator amplitude, the scanner / Leslie crossover (the 523 Hz partial is in the drum channel, the 1046 Hz partial in the horn) or the key-down velocity, none of which this clip isolates).

## 3. Piano clip, "How to Make a Soft Piano Sound" (video `NVMtX0epXog`)

Program: Layer Init, then the piano section layer A with the **White Grand**, no EQ, no reverb (subtitle 1:03 to 2:05). Timbre is moved from Normal to Soft at about 2:18 to 2:31; dynamic compression is set to 2 (about 50 %) at 3:11 and off again at 3:25; reverb (Cathedral) comes in at 4:22. The playing is music (chords and lines), so single-note level versus velocity, attack and release cannot be separated: per-onset peak levels, centroids and decay slopes (10 ms level jumps, 113 to 240 s) scatter by more than 10 dB with pitch and polyphony alone.

Long-term spectrum (Welch, 16384-point, band power relative to total; video time):

| Segment | RMS dBFS | 60-250 Hz | 250-500 | 500-1k | 1-2k | 2-4k | 4-8k |
|---|---|---|---|---|---|---|---|
| Normal timbre 113-140 s | -17.6 | -6.8 | -3.9 | -6.1 | -12.4 | -19.5 | -13.1 |
| Soft timbre 152-168 s | -16.8 | -3.7 | -5.7 | -6.6 | -14.4 | -21.9 | -15.3 |
| Soft + dyn comp 2, 192-205 s | -18.0 | -3.7 | -5.7 | -7.1 | -14.1 | -22.4 | -13.3 |
| Soft, comp off, 206-224 s | -18.4 | -3.7 | -4.5 | -8.0 | -12.6 | -25.2 | -21.3 |

The 4 to 8 kHz band is dominated by codec noise and room (not used). Normal to Soft: 2 to 4 kHz -2.4 dB (to -5.7 dB in the longest soft passage), 60 to 250 Hz +3 dB, overall level about +0.8 dB. Direction agrees with a darker timbre; the size is tied to which notes were played. The emulator's `.npno` piano has a timbre field in the program (`piano_preset.rs`); the matching render (same notes, normal vs soft) was not run.

## 4. Implications

- No drawbar law or piano law is solid enough to apply; no switch was added.
- The clip is B3 only: **no new B3-versus-Pipe parity evidence**. A Pipe / Vox / Farfisa drawbar clip is still needed (OQ19, [pipe-video-gap](pipe-video-gap.md)).
- To pin the drawbar curve a clip must move a drawbar through positions 1..7 on a held note with the voice silent. In this clip that happens only in the wide shot (118 to 134 s, hands over the panel, music playing).
- The B3 4' partial is 3.6 to 4.5 dB under the 8' on hardware and level or above in the emulator: candidate cause is the per-wheel generator amplitude or the rotary crossover, to be tested with the `sharc-organ` oracle and a rotary-off clip.

## 5. Follow-ups and stop point

Stop point (time box): organ states A, B, C measured and compared; the D state and the wide-shot progressive pull (118 to 134 s) are not analysed; the B3 Bass, percussion and vibrato segments (4:28, 6:03 to 7:50) are not measured; the piano clips "SOFT GRAND Setup and Demo" and "Grand Piano and Single Note String Layer Tutorial" were not opened; no piano emulator render and no `tools/piano_velocity.py` run.

- Cut the organ audio with a speech mask (speech has no fixed pitch; keep only frames whose 261.6 Hz comb dominates) and repeat A, B, C with 3 s windows.
- Read the wide-shot progressive pull for drawbar positions 1..7 (needs frame-by-frame LED counts).
- Run the same White Grand notes in the emulator with Timbre normal and soft and compare the 2 to 4 kHz band.
- Segments are in `data/emu_vs_hw_segments.json` under `dsoundman_segments` (spec only, `tools/emu_vs_hw.py` does not run them yet).

| Item | Status |
|---|---|
| Drawbar positions per state (A, B, C) | VERIFIED by frame reading, `tools/dsm_partials.py` input times |
| Partial levels B, C | EMPIRICAL, low confidence |
| Piano band shift normal to soft | EMPIRICAL, low confidence |
| White Grand to `.npno` mapping | ASSUMED |

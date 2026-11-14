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
| Emulator | Default tone source: 4' +1.8 dB (rotary on, +0.2 off), 2' -1.0 dB. `b3wheels`: 4' -0.1 dB, 2' -5.7 dB (rotary on; section 5.2 adds the firmware-weight check). `firmware`: same as default for 8'/4'/2'. No variant matches both partials; the 4' error is +3.5..+6.3 dB, the 2' error is +1.3 or -3.4 dB. | EMPIRICAL |
| Law applied behind a switch | **None.** The evidence is one note, three states, low confidence. | n/a |
| Piano program | White Grand (Grand class, `Piano/Grand/White Grand XL 6.3.npno` in the factory backup, name match only), after `Shift + Layer Init`, no effects, no EQ. | ASSUMED (mapping by name) |
| Piano timbre normal vs soft, dynamic compression | Long-term spectrum only (music, different notes): 2 to 4 kHz band falls by 2.4 to 5.7 dB relative to the total when Timbre goes to Soft; the level change is below 1 dB. Dynamic compression is not separable. | EMPIRICAL, low confidence |
| Piano emulator comparison | Done in section 5.4 (White Grand: bands, attack, T60); the Soft Grand XL is not in the factory backup. | EMPIRICAL |

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

## 5. Second pass (2026-11-14)

### 5.1 The silent `--firmware-routing` render is not a bug

The program JSON used for the organ-only renders carries `section_enable_i = 1` (bit 0 = Synth, bit 2 = Organ, `combine_scene_mask`, `presets.rs`). `--firmware-routing` installs the decoded exact scene masks (`routing.rs`), which gate each section by its `section_enable` bit, so the organ section was **off in scene I** and the engine was silent as designed; the legacy masks fall back to the layer enables and sounded. With `section_enable_i = 4` the same render sounds with routing (peak 0.354 default, 0.236 `b3wheels`; with `section_enable_i = 1` the peak is 0.0000 for all three routing variants). No engine change is needed. Rule for organ-only renders: disable the other layers **and** set `section_enable_i` to the organ bit.

### 5.2 The 4' and 2' partial ratio: firmware weights versus the clip

The release linker of this machine stopped working during the session (`link.exe` exit `0xc0000135`), so these renders use the binary built earlier from `main` @ ecf6833 (the `b3wheels` path is unchanged since). A new oracle run needs the ARM settled snapshot (not available here; `keydown-b3-c4e4g4.script` alone through `sharc-pass` gives silence: no boot state). The oracle numbers below are therefore the documented ones plus the firmware tables, not a new oracle run.

C4 (note 60), drawbars 8' + 4' (+ 2') at 8, amplitude `A(wheel) * W(key, footage)` from [keydown-b3-wheels.csv](data/keydown-b3-wheels.csv) and [b3-key-weights.csv](data/b3-key-weights.csv), then the VERIFIED tail gain ([b3-tail-gain.csv](data/b3-tail-gain.csv), [organ-b3-tail](organ-b3-tail.md)):

| Footage | Partial note | `A` | `W` (key 60) | `A W` re 8' (dB) | Tail gain (dB) | Total re 8' (dB) | Rust `b3wheels`, rotary off / on | Clip |
|---|---|---|---|---|---|---|---|---|
| 8' | 60 | 0.2940 | 0.03294 | 0 | +6.93 | 0 | 0 | 0 |
| 4' | 72 | 0.3704 | 0.03222 | +1.8 | +3.37 | **-1.8** | -1.7 / -0.1 | **-3.6 to -4.5** |
| 2' | 84 | 0.4879 | 0.03222 (4' column reused, ASSUMED) | +4.2 | -2.23 | **-5.0** | -4.5 / -5.7 | **-2.3** |

- The firmware weights plus the verified tail predict the 4' at -1.8 dB and Rust agrees (-1.7 dB rotary off): the Rust 4' is **not** a weights bug; the clip is 1.8 to 2.7 dB lower (low confidence, +-3 dB), inside the clip's error.
- The 2' clip value is 2.7 dB above the model that reuses the 4' column. If real, the 2' `W` at key 60 is about 1.37 times the 4' value (+2.7 dB): the reused 4' column under-estimates the 2' footage at this key. One key, one clip, +-3 dB: it supports the ASSUMED reuse being a bit low, it does not replace it. A key-down capture at drawbars `888888888` still pins `W` for 2 2/3' and up.
- Default tone source: 4' at +1.8 dB (no tail). `b3wheels` is closer for 4' (error +2.1 versus +5.4 to +6.3 dB); for 2' the default is closer with rotary on (+1.3 versus -3.4 dB). Mean absolute error over the two partials: `b3wheels` 2.4 dB (rotary off) / 3.0 dB (on); default 3.4 / 3.4 dB.

### 5.3 Wide-shot progressive pull (118 to 134 s)

Method: 4K crops of the nine LED ladders at 4 fps (red-pixel test on each LED, 0 to 8 lit per column) and the partial power of the held chord at 0.5 s windows (C3 + G3: peaks 130.7 and 195.9 Hz). The program is in **preset mode** (the `PRESET` LED is lit, subtitle 7:57 to 8:30): the sound follows the program until a drawbar moves, and the hand covers the ladders for most of the pull, so the LED count is unreliable (columns 3 to 9 read 0 to 7 while their partials sound). Only the 16' drawbar has a clean ramp before the hand covers the panel:

| Time (s) | 16' LEDs lit | G3 16' partial (98.0 Hz, dB) | Firmware d7 level re position 8 (dB) | Measured minus model (dB) |
|---|---|---|---|---|
| 118.75 | 1 | -8.4 | -32.5 | 24.1 |
| 119.00 | 3 | -3.1 | -19.8 | 16.7 |
| 119.25 | 3 | +3.1 | -19.8 | 22.9 |
| 119.50 | 4 | +5.8 | -15.2 | 21.0 |
| 119.75 | 5 | +11.0 | -10.3 | 21.3 |
| 120.00 | 6 | +15.7 | -6.2 | 21.9 |

From position 3 to 6 the measured rise is +12.6 dB against +13.6 dB for the firmware curve `0,3,6,13,22,39,62,87,127` (steps predicted +4.6, +4.9, +4.1; measured +2.7, +5.2, +4.7). The offset (measured minus model) is 21.5 +- 2.5 dB over positions 3 to 6: a linear-amplitude law over positions 3..6 within about 2 dB. Positions 1 and 8 are not usable (position 1 is a half-moved fader; later windows have other notes in the 98 Hz band). EMPIRICAL, one drawbar, one note, low confidence; it does not contradict `DrawbarLaw::Firmware` and it is the only per-position evidence in these clips. No law change.

### 5.4 Piano clips

| Clip | srt | Content | Usable |
|---|---|---|---|
| `NVMtX0epXog` Soft Piano | yes | White Grand (Layer Init), normal timbre 1:28 to 2:18, then soft | emulator comparison (below) |
| `0-upeS5SzbE` Soft Grand | yes | Soft Grand XL, a **library sound installed with the Sound Manager** (not in the factory backup: `Piano/Grand` holds Astoria, Bright, EGrand, Royal, Silver, Studio, Velvet and White only) | hardware reference only; the emulator cannot render it |
| `Rq7N88wLKRw` Grand Piano + Single Note String | **no srt** in the folder | not opened (no spoken dynamics to anchor on) | no |

Soft Grand XL, clean passages (no speech, video 540 to 570 s and 575 to 610 s), long-term spectrum relative to the total: 60-250 Hz -6.2 / -5.5 dB, 250-500 -4.5 / -4.2, 500-1k -4.3 / -5.4, 1-2k -15.5 / -13.6, 2-4k -27.2 / -23.6, **4-8k -48.6 / -44.7**, RMS -16.6 / -16.9 dBFS. White Grand 516 to 530 s (the instructor talks) reads 4-8k at -12.0 dB: that is speech. The piano alone has less than -44 dB above 4 kHz, so the 3-8 kHz and air bands of every speech-mixed segment (including section 3) are speech and codec noise, **not piano**. The 2 to 4 kHz drop of section 3 is speech-contaminated as well. The srt states no dynamics ("play softly" only), so level and brightness versus dynamics cannot be read from these clips; dynamic compression 0 / 2 / 3 (12:28 to 13:37) moves the RMS by under 1 dB (-17.1, -17.7, -17.1 dBFS) and is not separable.

White Grand, normal timbre, 113 to 140 s of `NVMtX0epXog` (27 s of music; `video_ref.py notes`: 386 audio notes, 179 confirmed by the video, `--confirmed-only`). Emulator: `White Grand XL 6.3` through the factory backup with the program's other layers off (`section_enable_i = 2`), Timbre Off, no effects. Deltas emulator minus hardware (`tools/emu_vs_hw.py segment_metrics`; speech and codec noise make the hardware 3-8k and air bands unreliable):

| Variant | sub 20-120 | low 120-400 | mid 0.4-1.5k | pres 1.5-3k | brill 3-8k | RMS | attack hw / emu (ms) | T60 hw / emu (s) |
|---|---|---|---|---|---|---|---|---|
| uniform velocity 88, default (firmware piano level) | +6.5 | -0.3 | -3.5 | -1.0 | -12.0 | +2.4 | 52.7 / 65.6 | 5.4 / 1.0 |
| uniform 88, `--legacy-piano-level` | +6.8 | -0.1 | -3.3 | -1.6 | -10.1 | +4.4 | 52.7 / 72.1 | 5.4 / 1.2 |
| uniform 88, `--firmware-all` | +6.5 | -0.3 | -3.5 | -1.0 | -12.1 | +2.4 | same | same |
| per-note velocities (`tools/piano_velocity.py`: median 99, mean 89.6, gain -9.5 dB, 99 of 179 notes contaminated) | +5.5 | -1.4 | -3.5 | +1.0 | -3.9 | +5.3 | 52.7 / 67.4 | 5.4 / 1.0 |

Reading: mid band 3.5 dB low and sub band 6.5 dB high in every variant (the sub excess is pedal and string-resonance rumble plus notes the detector put an octave low: audio-only register p50 is MIDI 51). Per-note velocities lift the 3 to 8 kHz band from -12 to -3.9 dB, i.e. the played dynamics were wider than uniform 88 (median 99), but 55 % of the notes are contaminated: low confidence. The emulator attack is 13 to 20 ms slower than the recording (median 10 to 90 % envelope rise); the 5.4 s hardware T60 against 1.0 s is dominated by the sustain pedal and the room of the recording, not a measured release law. Absolute RMS is not comparable (the tool's recording gain is -9.5 dB).

## 6. Follow-ups and stop point

Stop point (second pass, time box): organ states A, B, C and the 16' ramp measured; the D state, B3 Bass, percussion and vibrato segments are not measured; `Rq7N88wLKRw` has no srt and was not analysed; the Soft Grand XL cannot be rendered; the release link step failed after the merge, so no new Rust code or tests were added.

- Cut the organ audio with a speech mask and repeat A, B, C with 3 s windows.
- Capture a B3 key-down at drawbars `888888888` (ARM harness) to pin `W` for 2 2/3' and up (section 5.2), then re-check the 2' error.
- Re-measure the 16' ramp with the drawbars in live mode (`PRESET` off) and a held single note, hand off the ladders.
- Find a piano clip with stated dynamics for the level law; these course clips state none.
- Segments are in `data/emu_vs_hw_segments.json` under `dsoundman_segments` (spec only).

| Item | Status |
|---|---|
| Drawbar positions per state (A, B, C) | VERIFIED by frame reading |
| Partial levels B, C | EMPIRICAL, low confidence |
| 16' ramp positions 3..6 follow `0,3,6,13,22,39,62,87,127` within 2 dB | EMPIRICAL, low confidence |
| 2' weight 2.7 dB above the reused 4' column at key 60 | EMPIRICAL, low confidence (one key) |
| Piano band shift normal to soft (section 3) | superseded: speech-contaminated |
| White Grand to `.npno` mapping | ASSUMED |
| `--firmware-routing` silence | VERIFIED (section 5.1), not a bug |

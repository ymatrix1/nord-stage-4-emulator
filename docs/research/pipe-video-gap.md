# Pipe organ against the Part A video: where the gap is (and is not)

**Checked:** 2026-11-13
**Sources:** `NEW NORD STAGE 4 - PROGRAMAS PRESETS DEMO (PART A) [_zksT69H30c].webm` and the factory `.ns4b` (both local, git-ignored); `ns4 convert` of `Program/Bank A/Space Drama II.ns4p`; `tools/emu_vs_hw.py` (variants with program edits, spec `data/emu_vs_hw_segments.json`); the new `tools/mix_fit.py`; the SHARC oracle `sharc-organ` on the PNO image (`tools/organ_oracle_audio.py`, derived from `target/fw_unpacked/bank0_CODE.bin`); `crates/nord-tools/src/bin/organ_ref.rs` (new `--mode pipe`); `organ/pipe_lines.rs` (new test). Raw rows: [data/pipe_video_gap_results.json](data/pipe_video_gap_results.json). Builds on [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md), [organ-oracle-audio](organ-oracle-audio.md), [organ-model-paths](organ-model-paths.md), [keydown-zones](keydown-zones.md), [dsp-frame-rate](dsp-frame-rate.md), [arm-boot-state](arm-boot-state.md).
**Question:** [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md) put the Pipe2 clip (A:38 Space Drama II) at brilliance (3 to 8 kHz) -11.2 dB, presence -5.9 dB, stereo width 0.33 against 0.74, and scored the pre-bank `Legacy` waveform (3.99) better than the firmware bank (4.95). Is that a Pipe bank error (copy mix, ladder, post stage, level staging), or something else in the clip?

Tags follow `.agents/rules/provenance-in-code.md`. Numbers are shape evidence from a lossy live recording; the oracle numbers are VERIFIED.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is the clip a Pipe-only signal? | **No.** Program decode: `global.section_enable_i/ii = 5` (organ 4 + synth 1), organ layer A Pipe2 `16' + 8'` (drawbars `8 0 8 0 0 0 0 0 0`, octave +1), **two synth layers on** (levels 0.56 / 0.60), vibrato off, rotary off, no amp, EQ flat, organ FX = `mod2` type 4 + reverb type 4 (amount 0.80). It is the only Pipe program in the video with the organ section on (Boxxx A:28 has section bits 3: organ off). | VERIFIED (program JSON) |
| Isolation | Rendering the synth section alone (reverb and mod2 off) scores **2.45**, the organ section alone **6.35 to 6.55**, the full program 4.95. A non-negative power mix of the two renders explains the hardware spectrum to within +-2 dB up to 1.3 kHz **with the synth supplying almost all of it**; the organ share is 10 to 26 dB below the hardware above 400 Hz (section 3). | EMPIRICAL |
| Does the Rust Pipe path match the oracle? | **Yes, 0.00 dB per line** on three chords (mid, low with all nine stops, high with octave foldback) in both weight modes, 41 lines; test spread under 0.1 dB (`pipe_lines.rs`). The bank, mix stage, 12-TET ladder, x table and foldback are not the cause. | VERIFIED |
| Copy mix (`COPY_SPREAD_CENTS`) vs two true stereo lanes | Not involved: the constant was removed earlier; this program has vibrato off, so the table is the exact one (gains `1, 0, 1, 0`) and both lanes are copy 0 in the oracle and in Rust (dual mono). The stereo of the clip does not come from the Pipe bank. | VERIFIED |
| Ladder against the hardware anchor | Rust sets pitch from `wheel_hz` (A4 = 440, C1 = wheel 0), independent of the frame rate. The ARM's own Fine Tune word puts wheel 0 at **C1 - 0.35 cent** at 43,402.34 Hz ([dsp-frame-rate](dsp-frame-rate.md) 3.3), the table is 12-TET within 0.055 cent: at most 0.4 cent, inaudible and spectrally irrelevant. Not changed. | VERIFIED (offset), no effect |
| Effects after the organ | Rotary, amp and EQ are off in this program. Organ `mod2` + reverb move every scored band by 1.2 dB or less and add the only width the organ has (0.00 dry, 0.17 mod2, 0.29 reverb, 0.34 both). | EMPIRICAL |
| Level staging | The Legacy drawbar bus scale (`0.08`, ASSUMED) is **+14.4 dB** above the verified ARM weight unit (`127 * 1008 * 2^-23 = 0.01526`, `WeightMode::Firmware`, level `L` assumed 1.0 and open, OQ19). The mix fit wants the organ about 9.3 dB below the synth render, the same direction as the known synth level deficit (-14.6 dB re the other engines). | EMPIRICAL |
| Why is Legacy "better"? | Legacy Pipe2 is `0.7 sin p + 0.3 sin 2p`: a second harmonic at -7.4 dB where the oracle table has -19.0 dB. It adds 2.7 dB of brilliance (dry organ: -11.4 to -8.7 dB) and so fills part of the treble the *synth* is missing. It is not closer to the instrument. | EMPIRICAL |
| Default changed? | **No.** The bank already meets the acceptance test (<= 1 dB per line, three chords), and no variant of the Pipe path both keeps that and shrinks the video error, because the video error is not in the Pipe path. `render_hash` unchanged (D22). | |

## 2. Section isolation (confirmed notes, emulator minus hardware)

Same 259 video-confirmed notes, velocity 100, `tools/emu_vs_hw.py` variants with program edits (`global.section_enable_i/ii`: 4 = organ only, 1 = synth only). Bands in dB re RMS, width = side/mid (hardware 0.74), score = mean |band delta| over the five bands 20 Hz .. 8 kHz.

| Variant | sub | low | mid | presence | brilliance | centroid Hz | RMS dB | width | score |
|---|---|---|---|---|---|---|---|---|---|
| default (organ + synth, firmware bank) | +4.1 | +3.0 | -0.6 | -5.9 | -11.2 | -316 | +7.3 | 0.33 | 4.95 |
| legacy-tone (organ + synth) | +3.3 | +2.9 | +0.3 | -4.3 | -9.2 | -271 | +6.7 | 0.33 | 3.99 |
| organ only (mod2 + reverb) | +5.3 | +3.5 | -1.4 | -10.3 | -12.2 | -384 | +6.0 | 0.34 | 6.54 |
| organ only, reverb off | +5.5 | +4.1 | -1.0 | -9.4 | -11.6 | -387 | +6.8 | 0.17 | 6.34 |
| organ only, mod2 off | +5.5 | +3.5 | -1.3 | -10.4 | -12.0 | -388 | +7.7 | 0.29 | 6.55 |
| organ only, dry | +5.8 | +4.1 | -1.0 | -9.5 | -11.4 | -390 | +8.5 | 0.00 | 6.35 |
| organ only, dry, Legacy tone | +5.2 | +3.8 | -0.1 | -5.9 | -8.7 | -335 | +7.5 | 0.00 | 4.77 |
| synth only | -3.1 | +2.5 | +1.6 | -2.0 | -9.3 | -175 | +1.8 | 0.30 | 3.73 |
| synth only, reverb and mod2 off | -1.0 | +2.0 | +2.1 | +0.2 | -6.8 | -97 | +3.6 | 0.00 | 2.45 |

The organ alone is further from the recording (6.4 to 6.5) than the mix (4.95); the synth alone is closer (2.45 to 3.73). The clip's treble and mid are synth content, and the 8 kHz to 20 kHz band (not scored) is the YouTube low-pass.

## 3. Power-mix fit (`tools/mix_fit.py`)

`uv run python tools/mix_fit.py hw.wav emu_org_only_conf.wav emu_synth_dry_conf.wav`: non-negative least squares on third-octave band power (relative error), 63 Hz to 8 kHz.

| Band Hz | hw dB | fit error dB | organ share dB re hw | synth share dB re hw |
|---|---|---|---|---|
| 100 to 320 | 25 to 30 | -0.0 to +2.0 | -2 to -7 | -4 to +1 |
| 400 to 1270 | 29 to 35 | -2.7 to +0.3 | -10 to -15 | -3 to 0 |
| 1600 to 2540 | 25 to 30 | -2.3 to -3.3 | -18 to -26 | -2 to -3 |
| 3200 to 6400 | 11 to 24 | -8.5 to -16.4 | -10 to -25 | -9 to -20 |

Fitted powers: organ 0.023, synth 0.20 (the organ render is 9.3 dB too strong relative to the synth in the mix, or the synth 9.3 dB too weak). Above 3 kHz **neither** render, alone or mixed, reaches the recording (8.5 to 16 dB short): that is the known synth presence gap ([emulator-vs-hardware-audio](emulator-vs-hardware-audio.md), synth_lead -16.5 dB at 1.5 to 3 kHz) plus whatever the real organ adds. The clip cannot separate the Pipe treble from the synth treble; a Pipe-only recording is needed (follow-up 1).

## 4. Oracle line comparison (bank + mix stage, three chords)

Method: `sharc-organ` (PNO image from `tools/ns4_dsp_images.py` / `ns4_dsp_ldr.py`, OS v1.66), 16,384 frames, first 2,048 dropped, exact table, level 1; `organ_ref --mode pipe` renders the product Pipe2 layer (default tone source = bank, no key click, rotary off); `organ_oracle_audio.py compare` prints per-line `rust/oracle` in dB after one common gain. Results (all lines 0.00 dB to two decimals):

| Chord | Notes | Drawbars | Lines | Legacy weights | Firmware weights | Time-domain SNR after gain |
|---|---|---|---|---|---|---|
| mid | C4 E4 G4 | 888000000 | 8 | 0.00 dB | 0.00 dB | 40.4 dB |
| low | C2 G2 C3 | 888888888 | 17 (wheels 0 to 60, up to 3 stops per wheel) | 0.00 dB | 0.00 dB | 39.8 dB |
| high | C6 E6 G6 | 888888888 | 16 (wheels 48 to 88, foldback at the top) | 0.00 dB | 0.00 dB | 20.8 dB |

The `high` SNR is a phase effect, not an amplitude one: lines at wheels 71 to 88 (1.5 to 4.2 kHz) drift apart over 16,384 frames because Rust steps the wheels from `2^(w/12)` Hz and the oracle from the ARM words (0.055 cent quantisation, 0.35 cent anchor), so partial amplitudes agree to 0.00 dB while the waveform correlation drops. Tests: `organ::pipe_lines::pipe_lines_match_the_oracle_in_both_weight_modes` (oracle amplitudes embedded, line spread under 0.1 dB, Hann Goertzel) and `legacy_bus_scale_is_the_assumed_gap_to_the_firmware_scale` (Legacy minus Firmware = +14.39 dB).

Chord run on the "earlier prefix" state of [organ-oracle-audio](organ-oracle-audio.md) (poked weights and gains), not the full boot-log replay: the two layer-level sentinel words `DM 0xB2ECF` / `0xB2FF0` gate the B3 and B3 Bass chains ([arm-boot-state](arm-boot-state.md) s4); the Pipe bank does not read them, but a replay with the boot log on the Pipe key-down was not run here (follow-up 3).

## 5. Hypotheses

| Hypothesis | Verdict |
|---|---|
| Copy mix (detuned pair) vs two true stereo lanes explains width 0.33 vs 0.74 | **Refuted for this clip.** Vibrato off: exact table, dual mono. With vibrato on, Rust already routes the firmware lane matrix (`stereo_lanes.rs`, per lane SNR 37.6 dB). The width of the clip is FX and synth: organ dry 0.00, synth dry 0.00, reverb 0.29, mod2 0.17. Hardware 0.74 is a reverb / modulation stereo gap (`fx/`, other lane). |
| 12-TET ladder or frame rate wrong | **Refuted** (0.4 cent at most, no spectral effect). Frame rate 43,402.34 Hz only sets the lane smoothing time here. |
| Rotary, amp, EQ, reverb missing after the organ | **Not applicable**: rotary, amp and EQ are off in the program; reverb and mod2 move scored bands by at most 1.2 dB. |
| Organ level staging | **Open, small**: Legacy `0.08` bus is +14.4 dB over the ARM weight unit; level `L`, velocity source and layer level scaling are OQ19. The mix fit suggests the organ is hot relative to the synth by about 9 dB in this program. Not changed without a source for `L`. |
| Post stage | The Pipe path has none beyond the mix stage (`0x12272D` is the Vox bank, [organ-post-stage](organ-post-stage.md)); the flat `x0.92` in `render_frame` is not high-frequency damping (comment fixed, tagged ASSUMED). |
| Firmware bank too dark | **Refuted**: lines are exact; the table has 2nd to 5th harmonics at -19, -36, -44, -48 dB by firmware fit. A 16' + 8' registration of near-sines has no 3 to 8 kHz content unless played in the top octaves; the hardware treble in this clip is not organ. |

## Implications

- The Pipe bank, mix stage, ladder and foldback are oracle-exact (0.00 dB per line on three chords); the "Legacy scores better" result of the video comparison reflects a waveform with a wrong second harmonic (-7.4 dB against the firmware -19 dB) filling the synth's treble deficit. Keep the firmware bank as the Pipe default; do not move the default (no `render_hash` change).
- Space Drama II is a **synth-dominated** clip. The next work on its score (4.95) is the synth presence and level (TD-11, OQ22), the organ / synth level ratio (OQ19 `L`), and reverb / modulation width (OQ21), not the organ bank.
- The Pipe level is the open organ item: Legacy is 14.4 dB hotter than the ARM weight unit with `L = 1`.

## Follow-ups

1. Capture a Pipe-only hardware clip (organ section only, Pipe1 or Pipe2, no synth), or add a video segment whose section bits are organ only, and re-run `emu_vs_hw.py` `organ_pipe` variants plus `mix_fit.py`; only then can the Pipe treble be judged.
2. Settle the organ level `L` and the weight scale (OQ19) and re-check the organ / synth ratio of Space Drama II (fit: organ about 9.3 dB too hot).
3. Run the Pipe key-down chords through `sharc-pass` with the full boot log (`tools/arm_bulk_tables.py`, [arm-bulk-tables](arm-bulk-tables.md)) and compare the lines again, to confirm the sentinel words do not touch the Pipe bank.
4. Widen reverb and mod2 stereo against the 0.74 width (fx lane); re-measure the clip with synth and organ FX separately.
5. Optional: step the wheels from the ARM increment words (`fracmul(inc, 0x3FA02A00)`) to make the time-domain match bit-exact and remove the 0.35 cent anchor offset; this moves every Pipe sample by less than one cent and needs a `render_hash` update under D22.

## Reproduce

```bash
uv run python tools/emu_vs_hw.py --video <part-a.webm> --backup <factory.ns4b> --ns4 target/release/ns4 \
    --workdir <scratch> --only organ_pipe --confirmed-only --out <scratch>/r.json
uv run python tools/mix_fit.py <scratch>/organ_pipe/hw.wav <scratch>/organ_pipe/emu_org_only_conf.wav \
    <scratch>/organ_pipe/emu_synth_dry_conf.wav
# oracle lines (see organ-oracle-audio.md s7 for the SRAM / DSP images)
uv run python tools/organ_oracle_audio.py render SRAM/core1_sram.bin --exe target/release/sharc-organ \
    --notes 84,88,91 --drawbars 888888888 --out hi
target/release/organ_ref --mode pipe --notes 84,88,91 --drawbars 888888888 --weights firmware --out hi_rust
uv run python tools/organ_oracle_audio.py compare hi.f32.npy hi_rust.wav --notes 84,88,91 --drawbars 888888888
cargo test -p nord-core --lib pipe_lines
```

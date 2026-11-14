# Vox oscillator start state, the 36-sine bank and the footage level slots

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the PNO image `SRAM/core1_sram.bin` (OS v1.66, `tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`), `sharc-chain` and `sharc-pass` (`crates/nord-sharc`), `tools/pno_full_pass.py`, `tools/organ_post_ref.py`, `bank0_CODE.bin` (`tools/arm_xref.py dis`, `tools/arm_vox_slots.py`), the video `_zksT69H30c` and the factory backup (`tools/emu_vs_hw.py`). Builds on [organ-post-stage](organ-post-stage.md), [organ-keydown-weights](organ-keydown-weights.md), [pno-full-pass](pno-full-pass.md), [keydown-zones](keydown-zones.md), [b3-vox-video-gap](b3-vox-video-gap.md), [arm-boot-state](arm-boot-state.md). Only derived values are recorded (`crates/nord-core/src/organ/vox_image.rs`: 84 start words, 3 sine phases, 4 gains, the polynomial and base words, the ladder).
- **Question:** what makes the 8 chord lines of the Rust Vox bank diverge from the oracle by up to 6 dB, is the 36-sine bank (rows 84..95) needed, how do the Vox footage edits reach the 20 level slots, and can `ToneWheelSource::Vox` become the `ModelDefault` of the Vox model?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Answer in short

| Item | Result | Tag |
|---|---|---|
| Start phases | the divergence was the start state: the Rust bank started all 84 note phases and the LFO at 0, the oracle starts from the image words `DM(0xE2B14 + k)` (all 84 non-zero) and the LFO at `0x80000000` (`|s| = 1`, triangle at its top). Loading them: chord C4 E4 G4, 39 lines, **max 0.24 dB (fundamentals 0.02 dB)**, rows 43 and 55 +0.02 dB (were -3.4 / -2.1) | VERIFIED (run, section 2) |
| Reset at key-down | none. No ARM message of the key-down, key-up or model install writes `0xE2B14..0xE2B67`, `0xE21F0`, `0xE2B98..0xE2BBB` (script scan of the three captured scripts: 0 hits), and the routine `0x12272D` only adds to them. The oscillators free-run: on hardware the relative phases of a chord are those of the running bank, so the chord lines of a real keyboard vary per key press; the oracle (and the Rust source) fix them at the image state | VERIFIED (scan), hardware reading ASSUMED |
| 36-sine bank | ported (`vox_sines.rs`): base `B = 0x7572967D` times `1 + mod`, ladder `L_j = 0x1AE89F99 2^(j/12)` (j < 24; folded by 0.75 for j 24..31 and 0.63 for 32..35), three octave gain groups `1, 0.1718, 0.1660, 0.0841`, start phases per octave group, polynomial sine, buckets `b = j, j - 12, j - 24, j - 32` per group. Python golden `organ_vox_golden.py` (`organ_post_ref.sines`): max 1e-3 (test `golden_buckets_match_the_python_model`). Chord D (keys 50, 57, 62 and 90, the last one reaching bucket row 90): steady-state SNR 39.9 dB against the oracle | VERIFIED (golden, run) |
| Role of the sines | at the image base they run at 4.2 to 19.5 kHz and pass the output stage's 340 Hz / 1.1 kHz sections at -50 dB or more (peak 0.0026 for weight 1 on a bucket): rows 84..95 are the top two key rows (keys above C6 with the 2' slot, above C7 with 4', ...), not a tone source | VERIFIED (run) |
| Weight writes | instantaneous (the output stage reads `DM(0x95E46 + row)` as written, no smoothing in `0x1229B3`): the assumed 2 ms ramp is removed | VERIFIED (listing) |
| Footage slots | decoded from the setter `0xC0228E00` (section 4); the edit value to level law and the ARM tick that sets it are not captured: the render keeps the default four slots | VERIFIED (slots), law open |
| Default | stays **opt-in**: oracle criterion met, video criterion not (section 3) | decision |

## 2. Oracle chords (`sharc-pass`, 12,000 passes, `organ_partials.py`, Hann window, absolute dB)

Oracle: install burst of `keydown-vox-c4e4g4.script` plus the key-down weight words rebuilt from the verified map (`tools/organ_vox_chord.py`; the capture itself contains the key-up at pass 2, which has to be left out: with it the pass is silent). Rust: `organ_ref --mode vox --rate 43402.34375 --notes ...`.

| Chord | Keys | Lines above 1e-4 | Max error, all lines | Max error, fundamentals | Steady-state waveform SNR (frames 3000..12000) |
|---|---|---|---|---|---|
| A (the earlier chord) | 60, 64, 67 | 39 | **0.24 dB** (was 4.71) | 0.02 dB (was 3.40) | |
| B | 45, 52, 57, 61 | 4 keys x 4 harmonics | **0.34 dB** | 0.04 dB | 37.7 dB |
| C | 48, 55, 60, 65 | 4 keys x 4 harmonics | **0.12 dB** | 0.04 dB | 41.0 dB |
| D (key 90 on a sine bucket) | 50, 57, 62, 90 | | | | 39.9 dB |

All three chords are within 1 dB on every line, including the rows 43 and 55 of chord A. The first 3000 frames differ (waveform SNR 12.4 to 12.7 dB over all 12,000 frames): the Rust output sections and gains start settled, the oracle's start from the raw image state (the ASSUMED "start settled" of `vox_output`). Reproduce:

```bash
python tools/organ_vox_chord.py 45,52,57,61 > chordB.script
python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass --script chordB.script --passes 12000 --out chordB
cargo run --release -p nord-tools --bin organ_ref -- --mode vox --rate 43402.34375 --frames 12000 --notes 45,52,57,61 --out rust_B
python tools/organ_partials.py vox chordB.f32.npy rust_B.csv --keys 45,52,57,61 --skip 3000
python tools/organ_vox_image.py check SRAM/core1_sram.bin --exe target/release/sharc-chain
```

## 3. Video check (`tools/emu_vs_hw.py`, segment `organ_vox`, `emu_vs_hw_amp_vox.json`, confirmed notes, 11 s)

Bands are dB re RMS, emulator minus hardware. The Twin amp (type 289) is the default of `FirmwareFx` now.

| Variant | sub | low | mid 0.4-1.5k | pres | brill | RMS | peak dBFS | crest |
|---|---|---|---|---|---|---|---|---|
| default (Legacy) | -3.2 | -0.2 | +8.2 | -1.3 | +2.4 | +8.9 | -2.1 | 15.9 |
| **vox** (this PR, with key click) | -1.7 | -0.9 | **+4.2** | -0.1 | +3.4 | **-5.9** | -3.4 | **29.4** |
| vox, key click off (experiment, not committed) | +2.1 | +3.0 | +7.9 | -7.3 | -11.4 | -9.9 | -22.0 | 14.8 |
| vox, amp off | +1.6 | +1.5 | +10.6 | -4.1 | -7.7 | -8.7 | -12.7 | 22.9 |
| hardware | | | | | | | -11.6 | 16.1 |

Criteria: mid within about 3 dB (+4.2: fails by 1.2), RMS within about 3 dB (-5.9: fails), crest within about 3 dB (29.4 against 16.1: fails). The crest is the key click: without it the crest is 14.8 dB (inside the criterion) but the level falls 4 dB more and presence and brilliance drop 7 and 11 dB, so the click is also the brightness of the clip. The click is the legacy ASSUMED noise burst of the layer (`process_key_click`), not a firmware stage; removing the 2 ms ramp (instant weights) raised the click-dominated crest from 26.4 to 29.4 dB. The mid excess fell from +9.9 to +4.2 dB with the start phases, the sines and the amp default. The organ level `L` (OQ16/OQ17) is still ASSUMED at 1.0, which together with the click explains the RMS gap. Hence `ModelDefault` stays `Legacy`.

## 4. Footage edits to level slots (static decode of `0xC0228E00`, `tools/arm_vox_slots.py`)

`0xC022C678(this, i, value)` (virtual, vtable word `0xC04444B4`) maps the drawbar index `i` through the identity table `0xC02BE988` (`0..8`), stores the value at `[this + 0xDB4 + 4 i]` and calls the setter `0xC0228E00(this, i, value)`, which jumps through the table at `0xC0228E28` and stores `value` into the 8-byte slots `[this + 0xD18 + 8 slot]`, clears the queue and calls `0xC0224468` (weight sender):

| Drawbar index `i` | Level slots written | Footage offsets (`SLOT_OFFSETS`) |
|---|---|---|
| 0 (Db1) | 0 | 0 |
| 1 (Db2) | 2 | 12 |
| 2 (Db3) | 5 | 24 |
| 3 (Db4) | 10 | 36 |
| 4 (Db5) | 3, 13 | 19, 40 |
| 5 (Db6) | 8, 11, 16 | 31, 36, 48 |
| 6 (Db7) | 15, 17, 18, 19 | 43, 48, 52, 60 |
| 7 (Db8) | none | |
| 8 (Db9) | not a level (`[this + 0xDE4]`, the tone / wave word) | |

Slots 1, 4, 6, 7, 9, 12 and 14 are not written by this setter. This explains the default program (Db1..Db4 at 8: slots 0, 2, 5, 10 at 0.2475, the rest 0, VERIFIED in [keydown-zones](keydown-zones.md) section 5). Not captured: the law from the edit value `0..8` to the 23-bit level (8 gives `2076180 / 2^23`; the B3 curve of `FIRMWARE_DRAWBAR_CURVE` does not reproduce it, so no Rust gain law is guessed) and a Vox drawbar edit through the ARM harness (the layer client of `0xC00CC4F0` does not reach the Vox engine in the settled snapshot). The Rust render therefore keeps `default_levels()`; `vox_keys::add_key` already takes the 20 levels.

## Implications

- The oracle criterion for Vox is met for three chords (every line within 0.34 dB) once the image start state is used; the earlier 3.4 / 2.1 dB were pure start-phase interference.
- The video criterion is not met: the click and the unknown organ level `L` decide crest and RMS, not the bank.
- The sine rows only matter for keys above C6 and are inaudible beyond the top octave after the output low-passes; the port is complete for the oracle's sake.

## Follow-ups

1. Level law of the Vox drawbar edit values (a call of `0xC022C678` from the ARM main loop with a Vox engine object) and wiring of the drawbars into `levels`.
2. Whether the hardware click of the Vox model exists (`Edt_Sys_kOrganClickLevel`) and the organ level `L`; replace the legacy click burst on this source with the measured one.
3. The key-down bursts (1, 2, 1 rows at ticks 3, 17, 47) stagger the weights over 47 ms on the hardware: model the queue drain if the click needs it.
4. Real boot state: the phases of a running keyboard differ per power-on; the full boot log replay ([arm-boot-state](arm-boot-state.md)) would give the phases after the boot passes.

## 5. Level candidates from the boot log (coordinator finding, bisected here)

The B3 agent saw Rust Vox +4.2 to +4.3 dB above an oracle that has the full boot log in front and named four boot words near 0.634 to 0.640 (`DM 0xB4E30`, `0xB4E3A`, `0xB4EDC`, `0xB4F4A`; (101/127)^2 = 0.6325, the squared default layer level). Test: the Vox chord C4 E4 G4 (install + key-down, 6,000 passes) with each of the four words poked to `0x3F21F212` (0.6326) one at a time: the output is **bit-identical** to the unpoked run for all four (max difference 0.0). These words are not read on the Vox path of the install-script oracle (they may be read after the boot's other state, e.g. a gain that the install replaces), so they are not confirmed as the Vox level; the +4.2 dB is still open and not folded into the Rust path. Next step: replay the full boot log in front of the Vox install and bisect the 364 extra boot words as in [arm-boot-state](arm-boot-state.md) section 4.

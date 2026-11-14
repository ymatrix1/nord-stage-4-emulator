# Vox key-down timeline, the output gain word and the drawbar law

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the boot log replay (`tools/arm_bulk_tables.py capture`, 29,760 PNO core 1 words), `sharc-pass`, `tools/pno_full_pass.py`, `docs/research/data/keydown-organ-keys.csv` (model 1), `bank0_CODE.bin` (`tools/arm_vox_slots.py`), `tools/emu_vs_hw.py` (video `_zksT69H30c`). Builds on [organ-vox-start-state](organ-vox-start-state.md), [keydown-zones](keydown-zones.md), [arm-boot-state](arm-boot-state.md), [b3-vox-video-gap](b3-vox-video-gap.md).
- **Question:** how much of the video crest and brightness does the real key-down timeline explain, where does the +4.2 dB of the Rust Vox against the full boot state come from, and what is the Vox drawbar value law?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Answer in short

| Item | Result | Tag |
|---|---|---|
| Key-down timeline | the ARM sends the weights of a key in bursts: slot 0 (16') at tick 3, slots 2 and 5 (8', 4') at tick 17, slot 10 (2') at tick 47; key-up zeroes slot 10 at tick 1, slots 2, 5 at tick 7, slot 0 at tick 37 (all 24 notes of `keydown-organ-keys.csv`, model 1). Rust replays it per key (`vox_timeline.rs`, weights as instant steps) | VERIFIED (ticks), tick length 1 ms ASSUMED (the ARM timer was not read), grouping of the zero slots ASSUMED |
| Oracle against the timeline | chord C4 E4 G4, weights written at passes 65 / 369 / 1020 (`organ_vox_chord.py --timeline`), 16,000 frames: waveform SNR 41.9 dB after frame 3000 (gain 1.003); the first 3000 frames differ (11 dB) because the oracle output stage starts from the raw image (its gain smoothing `g1` starts low), the hardware has run for ever | VERIFIED (run) |
| Crest | with the timeline and **without** the legacy key click: crest 14.8 to 16.1 dB against 16.1 dB of the hardware (was 29.4 dB with the click). The firmware timeline explains the crest completely; the click is removed from the Vox firmware path | VERIFIED (run) |
| Brightness | not explained by the timeline: presence -7.3, brilliance -9.5 dB re RMS (the click had hidden a bank that is too dark above 1.5 kHz, see section 4) | VERIFIED (run) |
| Output gain word | `DM(0x95FD1)` = `DM(G + 0xB)` = `0x3F1EB100` (0.61989) written by the ARM boot (not by the model install). Found by bisection (section 3). Applied as `OUTPUT_LEVEL` in `vox_source.rs`: the Rust chord now matches the full-boot oracle on every line within 0.16 dB | VERIFIED (bisection, run); link to the layer level ASSUMED |
| Drawbar value law | the setter `0xC0228E00` stores `[0xC0443AF4 + 0x278 + 4 v]` for the drawbar value `v` = `0, 629145, 838860, 1048576, 1258291, 1468006, 1677721, 1887436, 2076180` (/ 2^23: steps of 0.025, 8 = 0.2475). Wired: `vox_keys::levels_from_drawbars` fills the 20 slots through the group map of [organ-vox-start-state](organ-vox-start-state.md) section 4; the layer uses its drawbars | VERIFIED (table read from the image) |

## 2. Video check (`emu_vs_hw.py`, `organ_vox`, program drawbars `8 8 8 4 2 2 0 3 4`, Twin amp default)

| Variant | sub | low | mid | pres | brill | RMS | peak dBFS | crest |
|---|---|---|---|---|---|---|---|---|
| before (PR #197, click, gain word 1.0) | -1.7 | -0.9 | +4.2 | -0.1 | +3.4 | -5.9 | -3.4 | 29.4 |
| timeline, no click, gain word 1.0 | +2.6 | +3.1 | +7.9 | -7.5 | -11.9 | -9.9 | -21.9 | 14.9 |
| + gain word 0.6199 | +2.6 | +3.1 | +7.9 | -7.5 | -11.9 | -14.1 | -26.2 | 14.8 |
| + drawbars (this change) | +2.5 | +3.2 | +7.8 | -7.3 | -9.5 | -13.7 | -24.5 | 16.1 |
| experiment: + 14 dB gain before the FX | | | +7.7 | -7.7 | -12.0 | -0.1 | -12.0 | 15.1 |
| hardware | | | | | | | -11.6 | 16.1 |

The crest is inside the criterion (16.1 against 16.1 dB). The level is not: the oracle-faithful organ output is 13.7 dB under the recording, so a gain stage between the DSP organ and the FX chain is missing (the "organ level" of OQ16/OQ17; with +14 dB the RMS and the peak match the hardware, the FX chain does not change the band shape). The spectral shape is not: mid +7.8 dB, presence -7.3 dB, brilliance -9.5 dB re RMS, and a gain does not move it (the Twin amp stays linear at this level). Hence the default **stays `Legacy`**; the failing criteria are mid, RMS (level stage) and the 1.5 to 8 kHz bands.

## 3. Bisection of the output gain (oracle, `sharc-pass`, 4,000 passes, chord C4 E4 G4)

Install-only RMS 0.027499; full boot log in front of the install: silent (the two sentinel words `0xB2ECF`, `0xB2FF0` of [arm-boot-state](arm-boot-state.md) section 4); without them 0.017046 = 0.6199 (-4.15 dB), the +4.2 dB of the B3 agent's finding. Halving by deleting ranges of the 29,758 remaining boot words (8 ranges per round, 6 rounds: 3,719, 465, 58, 7, 1 word): `DM(0x95FD1) = 0x3F1EB100` is the only word whose removal restores 0.027499. Its twin `DM(0xB44CD) = 0x3F1EB100` is written next to it (a B3 path word, no effect on the Vox chord). The four candidates named earlier (`0xB4E30`, `0xB4E3A`, `0xB4EDC`, `0xB4F4A`) do not change the Vox output (bit-identical pokes). 0.61989 is not `(101/127)^2` = 0.6325 (the squared program level) nor `(100/127)^2` = 0.6200 exactly; the value is the default program's `(layer level)` image under a law that was not captured, so the Rust path applies the word as a constant.

## 4. Why the bank is dark

The Rust bank equals the oracle on every line (0.16 dB, section 1), so the missing brightness is not in `0x12272D` / `0x1229B3`: either a later stage of the hardware chain adds it (the FX chain of the Vox program: Twin amp drive, EQ, modulation type 6, delay, reverb are all emulated), or the layer sums the second organ path. The legacy click carried 2 to 4 dB of presence and brilliance only by its noise burst. Open.

## Implications

- The firmware weight timeline replaces the legacy key click on the Vox source; crest is solved.
- The oracle criterion holds with the full boot state (all lines within 0.16 dB at the default drawbars).
- The default flip needs an organ output level stage (+13.7 dB) and an explanation of the 1.5 to 8 kHz deficit.

## Follow-ups

1. Read the ARM timer period (tick length) and the queue drain that sets 3 / 17 / 47.
2. The law behind `0x3F1EB100` as a function of the layer level (a second capture with another layer level).
3. Brightness: which stage of the real chain restores presence and brilliance (check the program's EQ and the amp tone stack against the recording with the level corrected).
4. Drawbar edit through the ARM harness to confirm the wiring end to end (the value law is read from the image, the call path was not run).

## Update 2026-11-13 (output law)

The Vox word `0x3F1EB100` is the swell word of the default pedal (`Fnc_Swell = 100`), the layer level law is `(v / 127)^2`, and the program rotary drive is now imported in the firmware unit (`preset / 10`): see [organ-output-law](organ-output-law.md) and D25.

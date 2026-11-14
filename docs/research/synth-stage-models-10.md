# Synth stages, tenth batch: the Ladder24 cutoff law (strip word to stage position)

- **Checked:** 2026-11-14
- **Sources (local, never committed):** ORG core 2 image (`tools/ns4_dsp_images.py` + `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (pass control block left in place), `sharc-disasm` (listings `0x24930A..0x249A00`, `0x241EAA..0x242306`), OS v1.66 `bank0_CODE.bin` / `bank0_SRAM.bin` in the ARM harness (settled snapshot), `capstone` listing of `0xC01C56CC`. Builds on [synth-stage-models-9](synth-stage-models-9.md) s4 and [synth-adr-and-strip-law](synth-adr-and-strip-law.md) s4.
- **Question:** Which word does the ARM write for the Ladder24 cutoff (`kFltType = 2`), how does it reach the stage position `DM(-30)`, and is the ASSUMED "host Hz = section pole frequency" law consistent with it?

Tags follow `.agents/rules/provenance-in-code.md`. Result: the stage position is **`R` of the strip table itself** (VERIFIED end to end through the pass control block), the ARM cutoff sender reads no filter type (VERIFIED by listing and run), and the knob law is `i = knob - 52` (VERIFIED run). SUPER 2..4 and SHAPE 3 / 4 were **not modelled** in this batch (time box spent on the cutoff law; s3).

## 1. Cutoff law, end to end

| Step | Word | Evidence |
|---|---|---|
| ARM `kFltFreq` (0x134), `0xC0017B7C` -> sender `0xC01C56CC(layer + 0x68, &value)` | the value cell is Q16.16: `pos = knob << 16` (clamped to `0x7FFFFF`) | VERIFIED listing; the older capture fed plain `0..127`, saw position 0 and reported "value independent" |
| Sender: table `0xC02F9968` (`pos >> 16`, 8-bit interpolation) | strip word 3 = `(T(pos + [+0x88]) - T(pos)) / 2` (0 without key track); common-block word 1 = the base word | VERIFIED listing; the sender reads `[layer]` (index 0..5) and the float fields `+0x8C / +0x108 / +0x10C` only, **no filter type** |
| Common word 1 (`DM(0xE02E9)`, ORG core 2) for knob 0..127 | top byte `i = knob - 52` (knob 0..117), steeper above, `0x6A000000` at 127 | VERIFIED run, [data/synth-filter-freq-law.csv](data/synth-filter-freq-law.csv), `tools/arm_synth_freq_law.py` |
| Pass control block: word -> `R` table `DM(0xB0BA2 + i)` -> frame `DM(-30)` (both lanes) | `R = 0.36083 + 0.0079301 i`, clamp `0..1.0111` | [synth-adr-and-strip-law](synth-adr-and-strip-law.md) s4 |
| Stage `0x12549D` reads `DM(-30)` as the position `p` | coefficients `-18..-12` = rows at `p = R` | **VERIFIED: 14 of 14 lane sets word for word, pass control block left in place, strip word swept to `R = 0.36 .. 0.86` (`tools/synth_filter_ladder_strip.py`, [data/synth-ladder-strip-cases.json](data/synth-ladder-strip-cases.json))** |

So the ladder position is the same `R` that the other filter stages read as the tangent coefficient; there is no second law for type 2. Resulting section pole frequency (`fc = -ln(pole) fs / 2 pi`, 48 kHz ASSUMED rate): knob 40 = 74 Hz, 52 = 149 Hz, 64 = 302 Hz, 80 = 772 Hz, 100 = 2.5 kHz, 117 = 7.8 kHz, i.e. exponential in the knob (2x per about 12 knob steps). For comparison the tangent reading of `R` for Lp12 gives 4.0 / 5.3 / 6.5 / 8.1 / 9.7 / 11.0 kHz at the same knobs: a table that is linear in `R` is a plausible ladder knob but a very flat Lp12 knob, so the `R = tan(pi fc / fs)` reading of the other stages deserves a hardware check (open item below).

## 2. What the Rust engine does

`LadderCoefs::from_strip_r(R)` is the firmware law (tests `strip_cases_match_the_stage` and `pole_frequency_follows_the_knob_exponentially` in `tests_fw_ladder_strip.rs`). The voice chain is fed a cutoff in Hz, not the knob, so it keeps `LadderCoefs::from_r` (host Hz = pole frequency). That agrees with the firmware wherever the host taper is the knob taper; the experiment of feeding the tangent `R` of a 2 kHz host cutoff straight into the position (pole near 20 Hz) broke ten synth tests and is rejected. The ASSUMED tag of `from_r` narrows to "host taper = knob taper".

## 3. SUPER 2..4 (`0x124985`) and SHAPE 3 / 4: findings only

- `0x124985` is a **stage-A wave stage** (frame `I2 = 0xC1F96`, `P = DM(-32, I2)`, the `synth_wave_oracle.py` harness), not a B-stage filter: it clips the pitch word `DM(-29, I2)` to `+-0.5`, builds four phase-weighted terms from `DM(-5, I2)`, selects a PM table row by `EXP` of a clipped word (`R1 = DM(-16, I1)` base), runs a delay-line read with two `MRF` / `MRB` multiply-accumulate loops (PM `I10` / `I11` coefficient tables, memory states at `DM(0xB6F80..)`, `DM(0xB00E6)`) and a three-section polyphase tail (`I9 = 0x95000`). A frame word scan through the B-stage harness (`synth_stage_oracle.run_stage`) is the wrong convention for it (it showed only the lane-interleaved states); the scan has to run through the stage-A harness with the P block and the PM tables set. Not started.
- `0x120F55` SHAPE 3 (listing `0x241EAA..0x242304`, 264 instructions) has a `CALL 0x12522E` (a table helper), a `RECIPS` chain for `1 / x` (two Newton steps, three times), a `LCNTR = 2` loop over four phase words (`DM(-25)`, `DM(-21)`) with clip / shaper terms and a SIMD mix tail. `0x12145E` SHAPE 4 was not read. Not modelled.

## 4. Coverage and what remains

ANALOG layers with a bit-exact stage model: **302 of 312 (96.8 %)**, unchanged. Filter layers with a bit-exact stage: **442 of 442 (100 %)**, unchanged; the Ladder24 cutoff law is now VERIFIED up to the host taper.

| Item | State |
|---|---|
| `0x124985` SUPER 2..4 (8 layers), `0x120F55`, `0x12145E` SHAPE 3 / 4 (1 + 1) | not modelled (s3) |
| `+0x74 -> +0x7C` selector copy | not traced (the harness keeps the word-1 stage at the Lp12 id for every type) |
| `R = tan(pi fc / fs)` reading for Lp12 / Lp24 / BP / HP (s1: the knob would span only 4 to 11 kHz) | needs a hardware sweep clip |
| Hz of the host cutoff taper versus the knob taper | ASSUMED equal |
| ARM writers of `DM(0xC2716)` (note term), the note table `0xB069B` and the oscillator P block knob words | not started in this batch |

## Implications

- The Ladder24 stage is right at its input: a future knob driven strip chain can call `from_strip_r` with the firmware `R`.
- The ARM frequency law is type independent; the capture tooling must feed Q16.16 values (`knob << 16`), which probably explains other "value independent" ids of [data/synth-edit-messages.csv](data/synth-edit-messages.csv) that take integer knobs.

## Follow-ups

1. Done ([synth-knob-laws](synth-knob-laws.md), D54): `tools/arm_edit_knobs.py` re-ran the synth ids with `value << 16`; [data/synth-knob-laws.csv](data/synth-knob-laws.csv) replaces the value dependence column of `synth-edit-messages.csv`.
2. SUPER 2..4 through the stage-A harness, then SHAPE 3 / 4.
3. The osc-ratio and P block knob words (`DM(0xC2716)`, `0xB069B`).

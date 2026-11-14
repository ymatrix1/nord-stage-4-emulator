# FX Amp EQ: words, gain law, the table `0x2f730c` and the compressor status

- **Checked:** 2026-11-14 (OS v1.66).
- **Sources (local, never committed):** [data/fx-param-fits.csv](data/fx-param-fits.csv) (ARM captures of `kAmpTreble`, `kAmpBass`, `kAmpMid`, `kAmpMidFreq` for the organ, piano and synth units), `target/fw_unpacked/bank0_CODE.bin` (table at file offset `0x2f730c`), the FX core 2 image listing of `0x1238FC` (`tools/adi_dis.py`), the ported routine `fx/fw_tone_stack.rs` (oracle golden `tests_resonant_golden.rs`).
- **Question:** which routine and words make the Amp EQ (treble, mid with sweep, bass), is the gain count to dB law the `2^(1/24)` ladder (TD-10), what is the table `0x2f730c` / stage gain table `0x2ae5b4`, and is there a dynamics block with more than `kCmpAmt`?

## 1. Answer

| Item | Result | Status |
|---|---|---|
| EQ routine (piano, synth) | the tone stack `0x1243F4` (module `0xC04438D8#2`): bass feed `A` and pole `a`, state variable mid band `B`, `C`, `D`, treble shelf `E`, `F`. Already ported and oracle verified; now wired (s3) | VERIFIED routine and words |
| EQ gain law | **`0.25 dB` per count on the cut side to 5 digits**, not the `0.250858 dB` ladder; the boost side is smaller: 14.77 dB at +60 (s2) | VERIFIED (words and routine) |
| Mid sweep | `B` (step) and `C` (damping) over the index; centre about `0.2 .. 6.8 kHz` at the firmware rate (s4); not the table `0x2f730c` | VERIFIED words, ASSUMED joint law |
| Organ unit | writes the **same 13 + 11 word values** to its own context (`0xC02BB2C4#1`); its routine layout is not decoded | words VERIFIED, routine ASSUMED equal |
| Synth unit | different word values (`B` and `a` about half: another sample rate or step); not wired | VERIFIED words, open |
| Table `0x2f730c` (18 x 128 `f32`) | **not the Amp EQ**: no row equals a mid word series (s5); form is a 3-set bank of `A, B, A', B', C, C'` rows over a 128 step index, sender `0x1be108` copies words | VERIFIED (negative), role open |
| Table `0x2ae5b4` (19 entries `10^((i-12)/20)`) | consumer still unknown; not the EQ gain (the EQ gain words are continuous in the count) | open |
| Compressor | one dynamics block (`0xC0441558#0`, routine `0x120630`), ARM words only from `kCmpAmt`; `kCmpResponse` re-sends constants with a plain-integer cell (**corrected: it moves the attack / release pairs with the Q16.16 cell**, [synth-knob-laws](synth-knob-laws.md) s2). Already wired as `FirmwareFx::compressor` | closed, see s6 |

## 2. Gain law (the TD-10 question)

The shelf word `F` (treble `c[-0x1B]`, bass `A` `c[-0x20]`, the same 13 words for both ids) gives the shelf's far gain `1 + F`: the routine's output gain is `G = (L - E) / 2` with `L = 1`, so the high shelf at Nyquist is `1 + F (1 - E) / (1 - E) = 1 + F`, and the bass feed at DC is `1 + A` (both asserted on the ported routine in `fx/fw_eq.rs` tests, 0.05 and 0.1 dB). In dB:

| counts | -60 | -45 | -30 | -15 | -1 | 1 | 15 | 30 | 45 | 60 |
|---|---|---|---|---|---|---|---|---|---|---|
| `20 log10(1 + F)` | -15.000 | -11.250 | -7.500 | -3.750 | -0.251 | 0.245 | 3.692 | 7.384 | 11.075 | 14.766 |
| engine (0.25 dB / count) | -15.00 | -11.25 | -7.50 | -3.75 | -0.25 | 0.25 | 3.75 | 7.50 | 11.25 | 15.00 |

The cut side is `0.25 dB` per count to five digits (`1 + F = 10^(c / 80)`), so `EQ_GAIN_DB_PER_COUNT = 0.25` is right and the ladder step `0.250858` is **not** what the EQ uses (it would give -15.05 dB at -60). The boost side is a different law: 0.058 dB low at +15, 0.234 dB low at +60 (the firmware boost is not `10^(c / 80)`). `EQ_GAIN_DB_PER_COUNT_LADDER` stays a named constant for the DSP ladder `DM 0xB05EB` that something else indexes.

## 3. Wiring

`FirmwareFx::eq` (`fx/fw_eq.rs`, `fw_eq_tables.rs`, `amp_eq.rs`): treble, bass, mid gain (counts = dB / 0.25) and mid index (nearest `EQ_FREQ_HZ` step) fill the tone stack context; the routine runs in place on two frames, so the layer gains **2 frames of latency**. Words: treble and bass from `fw_map` (13 points, linear between), mid from `fw_eq_tables` (gain at the default frequency: `B`, `C`, `D`; frequency at gain 0: `B`, `C`). Context constants: `k = 0.99078`, `L = 1.0` (`ASSUMED`, the oracle golden context holds both). LP24 / HP24 keep the Mid knob as resonance, so the mid band is neutral there. Default off; `ALL` includes it; `render_hash` unchanged.

## 4. Mid sweep: the firmware centre against the display table

`B` rises from 0.02866 (index 0) to 0.8526 (127). Solving the recurrence's poles (`(1 - q)(1 - q (1 - B C)) + B^2 q = 0`, `q = z^-1`) and taking the peak of `|W / X|` at the **DSP frame rate** 43,402.34 Hz ([dsp-frame-rate](dsp-frame-rate.md)) gives 199, 204, 289, 431, 641, 951 Hz at indices 0, 1, 15, 31, 47, 63 against the display steps 200, 205, 291, 436, 652, 975 (within 2.5 %), then 1576, 2604, 4250, 6845 Hz at 79, 95, 111, 127 against 1700, 2800, 4700, 8000 (7 to 14 % low; a peaking band's own peak moves with its gain, so this is a bound, not a mismatch). The damping `C` falls from 0.70 to 0.46 over the sweep, so the band gets broader (pole radius 0.99 to 0.78): the engine's fixed `Q` (`EQ_MID_Q_IDX`) is wrong in kind. The engine runs the recurrence at its own rate, so the firmware's frequencies come out `fs / 43402` higher (1.106 at 48 kHz); the words are not rescaled.

## 5. The table `0x2f730c` is not the EQ

`bank0_CODE.bin` at `0x2f730c`: 18 rows of 128 `f32`. Rows 0 .. 5 start `1.9496, 0.3493, 1.9440, 0.3437, 0.00933, 0.00940` and end (index 127) `1.0525, 0.2841, 1.0815, 0.3281, 0.654, 0.9927`; rows 6 .. 11 and 12 .. 17 repeat the pattern with other levels. The captured mid words do not match any row at any index (closest relative error 0.4 to 9 % for `C` against rows 7 and 9, 4 to 67 % for `B` against rows 5, 11, 17, and two orders of magnitude for the third word against row 10), the shapes differ (the table's `C` rows start at 0.0093, the captured `B` at 0.0287), and the same word series are written by the ids of the organ, piano and synth units with their own captured values. The three 6-row sets read as three biquad stage sets of one swept filter (the `LP24` / `HP24` amps and the filter effects), not the EQ. Unchanged: the sender `0x1be108` only copies words to the SHARC; no consumer routine was identified in this change.

## 6. Compressor

Findings checked again against the code: the ARM writes the compressor words only for `kCmpAmt` (threshold `-36 v / 127`, makeup `1 + 5.72 (v / 127)^2`); `kCmpResponse` re-sends the attack `(0.102071, 1.0)` and release `(4.607e-4, 0.04)` pairs for both values ([fx-reverb-leslie](fx-reverb-leslie.md) s4), so it selects nothing on the FX core. There is one dynamics block (module `0xC0441558#0`, routine `0x120630`, gain computer ported as `FwCompressor`); `0xC0441558#4` (runs 13, 14) is the Fx2 types 8, 9 and feedback fx types 2, 3 routine pair, not a second compressor. `FirmwareFx::compressor` already uses these words; a test (`tests_fw_eq_chain.rs`) pins that the firmware path gives bit-identical output for `comp_fast` false and true. The engine path keeps its `comp_fast` meaning (1.5 ms / 12 ms attack), which the firmware does not have. Still `ASSUMED`: the slope word (`0.05`, [fx-contexts](fx-contexts.md) s7), the log gain scale and the enable path, so the compressor stays opt-in.

## 7. Hardware comparison

No clip in `.reference/` isolates an Amp EQ setting (the video presets use the EQ at or near default), so no end-to-end level or frequency check against hardware was possible; `tools/emu_vs_hw.py` has nothing to score. The switch therefore stays opt-in (D22).

## Implications

- TD-10: the EQ gain count to dB link is settled for the cut side (`0.25 dB`, VERIFIED words); keep `EQ_GAIN_DB_PER_COUNT`, do not switch to the ladder.
- The Amp EQ of the piano and organ units has a verified implementation path (`FirmwareFx::eq`); the joint mid law and the organ layout are what remain.
- OQ21's table question splits: `0x2f730c` is a swept filter bank, not the EQ.

## Follow-ups

1. Run `tools/fx_eq_capture.py` (every mid index, the joint gain by frequency grid, treble and bass at every count) and replace the interpolation and the product form in `fw_eq_tables.rs`; then re-measure the response against the engine EQ.
2. Capture the organ unit's context layout (`0xC02BB2C4#1` words other than the 13 + 11 above) and its routine; the synth unit's rate.
3. Find the consumer of `0x2f730c` rows (the `LP24` / `HP24` amp types are the candidate) and of `0x2ae5b4`.
4. A hardware clip with a boosted mid or treble to score `FirmwareFx::eq`.

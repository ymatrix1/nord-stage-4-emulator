# Research: the B3 tone generator `0x121BBE` and why the processor output stayed 0

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the PNO image `SRAM/core1_sram.bin` (OS v1.66) and its ADI `elfdump` listing of `0x121BBE..0x121E2A` (generator) and `0x121E2B..0x122293` (processor) (`tools/adi_dis.py`, byte = 2 pc), the oracle `sharc-chain` (`--script`, `--step`, `--trace`, `--dump`), the booted-ARM captures of `tools/organ_b3_pass.py` (`boot-messages`, `keys`; 21 keys 21..108, drawbars `888888888`, velocity 100). Builds on [b3-tone-source](b3-tone-source.md) (PR #122). Derived data: [data/b3-generator-gains.csv](data/b3-generator-gains.csv) (notes, gains, frequencies; no firmware bytes).
- **Question:** what exactly does `0x121BBE` generate per key (frequency law, waveform, amplitude), is it equal temperament, and why did the summed processor output `DM 0xB46A8` stay 0 in the replay of b3-tone-source?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = fitted or read from data, **ASSUMED** = a reading.

## 1. Answer in short

| Question | Result | Tag |
|---|---|---|
| What is a ring word? | One **tone wheel per note**: `ring[note - 12] = G * WHEEL(phase_note)` at `DM 0xB32E8 + note - 12`, notes 12..115 (the dot product of `0x121E2B` reads notes 36..115 = 80 taps from `0xB3300`). A chord shares one ring word per note (the bank is indexed by note, not by key). Only the notes the ARM selected are written (nine per key). | VERIFIED (81 fits, residual 3e-8 of the rms) |
| Frequency law | 12 base oscillators from the **phase-step ladder** `PHASE_INC` (`DM 0x95172..0x9517D`, one octave) times the fine tune word `DM 0xB33B8 = 0x3F000000` (`r8 = r0 * r7 (ssfr)`, factor 63/128, rounded to nearest) per call; each octave doubles the step (`<< (note / 12)`, separate accumulators, `u32` phase, 1 cycle = 2^32). `step(note) = ((PHASE_INC[note % 12] * 0x3F000000 + 2^30) >> 31) << (note / 12)`. Note 60: 51,275,456 units = 0.011938497 cycles per call. | VERIFIED (max relative error of 81 fitted frequencies 2.5e-11; `floor` instead of `round` fails by 5e-7) |
| Equal temperament? | **No, but within 0.94 cent**: the 12 ladder words deviate from `PHASE_INC[0] * 2^(k/12)` by 0.00 / -0.16 / -0.66 / -0.81 / -0.94 / -0.53 / -0.77 / -0.33 / -0.36 / -0.41 / -0.77 / -0.50 cent (`phase_inc_cents()`). The apparent "2.04 : 3.04 : 4.07" ratios of b3-tone-source are the 1.8 % bin quantisation of that run; the true partial ratios are the ladder words (12-TET to 0.94 cent): 16' / 5 1/3' / 8' / 4' / 2 2/3' / 2' / 1 3/5' / 1 1/3' / 1' = notes `key + (-12, 7, 0, 12, 19, 24, 28, 31, 36)`, so 5 1/3' is `2^(7/12) = 1.4983` (not 1.5) and 1 3/5' is 5.04 (not 5). | VERIFIED |
| Absolute pitch | Per call phase step only; the Hz anchor needs the DSP frame rate. With 2 frames per call and note 60 = 261.6256 Hz the frame rate is 43,829 Hz; the bank-derived rate of [organ-tonewheel-bank](organ-tonewheel-bank.md) is 43,156.4 Hz (1.6 % = 27 cent apart). **Settled in [dsp-frame-rate](dsp-frame-rate.md): `F` = 43,402.34 Hz; the replay used the image's fine tune word `0x3F000000`, the ARM writes `0x3FA02A00` (+17.11 cent), which gives note 60 = 261.652 Hz.** | VERIFIED |
| Waveform | Linear interpolation (`fext`/`fdep` 7 + 25 bit phase, 128 + 1 point cyclic tables) in tables chosen **per note**: notes 36..39 `DM 0xB314B`, 40..47 `0xB31CC`, 48..115 `0xB324D` (notes below 36 probably use `0xB3049`, ASSUMED; the processor does not sum them). `0xB324D` = `0.9825 sin + 0.0175 (-cos 2th) + 0.00175 (-cos 3th)` (residual 7e-7), `0xB31CC` and `0xB314B` carry 10 % and 15 % second harmonic (more at 3rd, 4th): these are tonewheel shapes, the low wheels are not sinusoids. The oracle series equal `A * table(phase)` to 3e-8 relative. | VERIFIED |
| Amplitude `A` | The ARM key-down writes the float at `DM 0x95122 + (note - 36)` (0.29 .. 0.76 over the range, e.g. `0x9513A` = note 60 = 0.294, `0x95141` = note 67 = 0.3427, `0x95152` = note 84 = 0.5129, `0x9515E` = note 96 = 0.4718); `A` of the ring word equals that float. The percussion key-down writes the same words with a lower value for the 4' and 2 2/3' notes (0.2945 instead of 0.3704 for note 72), so the last write wins in a replay. The values vary a little with the key (note 36: 0.462, 0.486, 0.507 for keys 24, 36, 48): an ARM table, not a DSP law. | VERIFIED (equality), role EMPIRICAL |
| Why `DM 0xB46A8` stayed 0 | The summed output is not `0xB46A8` (a filter state). The processor's output sample is the store `dm(i0, m2) = r0` at the end of the frame loop into the **slot stream** (`0xC00EA..` for slot 4), and `r0` is zero in the replay because three groups of ARM-written words are never written by the boot, install, drawbar or key-down bursts: the 33 FIR taps `DM 0x95471..0x95491`, the second stage `0x9549D..0x954D6` / `0x954D9..`, and the dry mix gain `DM(0xB44C0)` (`dm(i3 - 0xE)`, `0xB44CE - 0xE`). With `DM 0x95471 = 1.0` and `DM 0xB44C0 = 1.0` poked, the stream is non-zero (rms 0.0021, peak 0.0051 for key 60, drawbars 888000000); without them 400 calls give exactly 0. | VERIFIED (run) for the cause, EMPIRICAL for the roles |

## 2. Method and commands

```bash
uv run --with unicorn python tools/organ_b3_pass.py boot-messages CODE SRAM boot.jsonl
for k in 21 24 36 37 ... 108; do
  uv run --with unicorn python tools/organ_b3_pass.py keys CODE SRAM k$k.json --key $k \
      --drawbars 888888888 --cache settled.snap
done
python tools/organ_b3_gen.py scan SRAM/core1_sram.bin boot.jsonl k61.json k62.json ... \
    --exe sharc-chain --json scan.json
```

`scan` runs 2,400 calls of the six slot routines, fits every active ring word with `gain * table(phase0 + t f)` (golden-section search on `f` and `phase0`, linear least squares on the gain) and prints the ratio of the fitted frequency to the bit-exact model `note_step` for the rounding `round` and `floor`. Output rows: [data/b3-generator-gains.csv](data/b3-generator-gains.csv) (136 rows, 81 with a frequency fit; columns without a frequency are notes with too few cycles in the window, 36..45).

| Check | Result |
|---|---|
| frequency, 81 words, pitch classes 0..11, octaves 3..9 | max `|f / model - 1|` = 2.5e-11 (`round`); `floor` 5.9e-7 on classes 1, 3, 5, 6, 9, 11 |
| waveform, 9 words of key 60 against the five tables | `0xB324D` 7e-9 residual, next best 2e-2 |
| waveform, notes 36..47 (keys 24, 37..47) | 36..39 `0xB314B`, 40..47 `0xB31CC`, residual 2.5e-8 |
| waveform, notes 48..112 (20 keys) | `0xB324D` everywhere, residual 3e-8 |

The slot routine order of the pass and the ARM replay come from [b3-tone-source](b3-tone-source.md) section 4.2; nothing of the ARM side changed.

## 3. The generator `0x121BBE` read from the listing

| Part (listing offset) | What it does | Tag |
|---|---|---|
| `+0x00..+0x78` | `r7 = DM(0xB33B8)` (fine tune); three SIMD iterations: `r8 = pm(i12) * r7` (`i12 = 0x95172`, ssfr), `r14 = r8 + phase` (`b6 = 0xB32DA`, circular 12): six phase accumulators, `fext` 7 bits at bit 25 = table index, `fdep` 25 bits = fraction, linear interpolation in the table `0xB3049`-based set (`i2`) | VERIFIED (listing), consistent with the oracle frequencies |
| `+0x7D..+0xF9` | the same for the rest of the 12-word ladder with `pm(i12 = 0x9517E)` ( octave shifts from `DM 0xB33C6` `r7 = dm(i1)` via `ashift`) and the table pointers `0x951DA` (`0xB3049 x3, 0xB314B, 0xB31CC x2, 0xB324D ...`), 22 loop iterations | VERIFIED (listing) |
| `+0xFF..+0x1BE` | ten **items** (the voice block `0x954E6..0x954F9`, `0x9559C..`, `0xB4246..`): a clipped note offset (`0x9559C` -37, -62, -85 for the key-60 items) selects a wheel through a pointer (`0xB3428`, `0xB3436`, `0xB34F0`), a mask / shift extracts two 7-bit fields and the item adds `coefficient * field * oscillator` into its ring destination (`0xB331F`, `0xB3318`, ...) | EMPIRICAL (the measured rings equal `A * table`; the item arithmetic is not transcribed bit for bit) |
| `+0x1C1..+0x20A` | a `MRF` multiply-accumulate pass over 84 words at `0xB445A` (`l2 = 0x54`) and the stream pointer bookkeeping `0xB32D0..D8`; role not identified | ASSUMED |

A line-by-line Python transcription of the item arithmetic (the SIMD, `fext`, `fdep` and `clip` chain) is **not** done: the ring words equal the model above to 3e-8 and the amplitude `A` is an ARM word, so the unverified part is the ARM item table, not the generator.

## 4. The processor `0x121E2B` tail

The processor is `ring . weights` (2 x 80 taps, `lcntr 0x13` x 4 MACs, weights `pm(i10 = 0x951F1)`), **followed by** three stages that all start with zero coefficients after the model install:

1. FIR 1 (`+0x266..+0x295`): the dot product `s0` is written into a circular delay line (`i6 = 0xB4283 + ..., l6 = 0x21`) and convolved with `DM 0x95471` (33 taps). Zero taps give 0.
2. A short second filter (`DM 0x95491`, two passes of three coefficients) and the scaling `f0 = (f0 + fir) * 0.7`.
3. FIR 2 (`+0x3A1..+0x3C2`, `l6 = 0x3D`) with `DM 0x9549D` (61 taps), `0x954D9` (14 taps), then `f0 = (f0 + f5) * 8` and `f0 += s * DM(0xB44C0)`; the final gain is `DM(0xB44EC) = 1.0` (written by the install).

The ARM writes none of `0x95471..0x954E5` and `0xB44C0` in the captured bursts (`arm-edit-capture-detail.csv` has no write to `0x95471..0x954E5` for any `Edt_Org*` id). Probable role (ASSUMED): the Hammond scanner vibrato / chorus line box (taps and per-model mix), programmed by the vibrato and model parameters through a handler that the ARM harness has not reached (as for `Edt_Org_Preset_kVibratoEnable` in [arm-organ-messages](arm-organ-messages.md)). The dot-product weights are the nine drawbar partials of the keys held: `W = d7 * taper` (e.g. 0.0122, 0.0129, 0.0143 for key 60 footages 16', 8', 5 1/3' at velocity 100 and drawbar 8; they range 0.0036 .. 0.038 over the keys because the record's taper bytes are `0x25..0xFF`).

## 5. Percussion

The percussion items (`[record + 0x2C]`, two items) write the 4' and 2 2/3' ring notes of the key with the lower amplitude `A` (0.795 x) and a weight in array B (`PM 0x95241`) at the 2nd-harmonic note; the same ring words feed the processor, so percussion is **extra weight on the shared wheels**, not another oscillator. VERIFIED for the words, ROLE EMPIRICAL.

## Implications

- The B3 tone source is a **tonewheel bank**: one wheel per note (ring word), the same structure as the Rust 91-wheel bank, with (a) an octave-locked 12-word ladder (not the Hammond gear ratios of the `Legacy` source: the deviations from 12-TET are 0.94 cent at most), (b) measured wheel shapes (`0xB314B`, `0xB31CC`, `0xB324D`) instead of `sin` plus ad hoc harmonics, (c) ARM-supplied per-note amplitudes `A` and per-key weights.
- `docs/research/b3-tone-source.md` section 6 item 1 is closed: `0xB46A8` is a filter state; the zero output is the unprogrammed vibrato/line-box block.
- The 2nd harmonic of the percussion item is a wheel at `key + 12` summed into the same bus (no separate oscillator).

## Follow-ups

1. Find the ARM writer of `DM 0xB44C0` (the taps `DM 0x95471..` come from the engine constructor `0xC002187C`, [organ-farfisa-bass](organ-farfisa-bass.md) s2) (vibrato / chorus type handler), then replay a B3 key with the tail programmed and compare the stream with the Rust drawbar bus.
2. Tabulate the per-key weights `W(key, footage)` (taper bytes) over all keys (the ARM capture of 21 keys is in this PR's data; 79 keys needed) and the key dependence of `A`.
3. ~~Settle the DSP frame rate~~ done: [dsp-frame-rate](dsp-frame-rate.md) (43,402.34 Hz).
4. ~~B3 Bass (`0x122296`) and Farfisa (`0x122B85`) replays~~ done: [organ-farfisa-bass](organ-farfisa-bass.md).

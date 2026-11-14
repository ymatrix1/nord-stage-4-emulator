# Research: the DSP frame rate of the PNO / ORG / FX / SY images (43,402 Hz, not 48 kHz)

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the OS v1.66 ARM image `target/fw_unpacked/bank0_CODE.bin` (loads at `0xC0000000`; `tools/arm_xref.py`, `capstone`), the five DSP images (`python tools/ns4_dsp_images.py ...`, `tools/ns4_dsp_ldr.py` for `core1_sram.bin`, `core1_l2.bin`), the oracle `sharc-run --sc57x` for the ORG / PNO Core1 init and `sharc-disasm`, the CCES 2.12.1 headers `ADSP_21571.h` / `ADSP_21571_device.h` (register names only), derived data [data/fx-delay-time.csv](data/fx-delay-time.csv). Builds on [b3-generator](b3-generator.md), [organ-tonewheel-bank](organ-tonewheel-bank.md), [organ-tone-source](organ-tone-source.md), [organ-post-stage](organ-post-stage.md), [fx-contexts](fx-contexts.md), [sy-dsp-map](sy-dsp-map.md), [sharc-chip-identity](sharc-chip-identity.md). Script: `tools/dsp_frame_rate.py`; Rust tests listed in section 5.
- **Question:** what is the frame rate of the DSP images (the rate at which one `render_frame` of the firmware advances), given that [b3-generator](b3-generator.md) read 43,829 Hz, the bank derivation read 43,156.4 Hz, and the SY rate (48 kHz) and ARM tick (1 kHz) were ASSUMED (OQ16, OQ22)?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command or test below, **EMPIRICAL** = fitted, **ASSUMED** = a reading.

## 1. Answer in short

| Item | Result | Tag |
|---|---|---|
| DSP frame rate `F` | **43,402.34375 Hz = 44,444,000 / 1024**. The ARM holds it as the float literal `0x47298A58` (pool `0xC01BF74C`, `0xC0221E44`, `0xC027A6C0`), with `F / 2` (`0x46A98A58`) and `2 F` (`0x47A98A58`) beside it. Five independent readouts agree (section 3): FX delay words, SY fade constants, Pipe bank ladder, B3 generator note 60, and the Vox rate word (to 0.0001 cent). | VERIFIED |
| Not 48 kHz | At 48 kHz wheel 0 of the bank would be 36.37 Hz at unity tune (a quarter tone off any note), the SY fade knee 5.53 kHz, and none of the FX delay words match. The "48 kHz native" statement of [hardware-architecture](hardware-architecture.md) section 3.3 (a crystal derivation, no firmware) is wrong for the DSP images. | VERIFIED (negative) |
| Why 43,829 (b3-generator) | The oracle replays ran with the **image's default fine tune word** `DM(0xB33B8) = 0x3F000000` (0.4922). The ARM overwrites it with `table[0x1F3] << 8 = 0x3FA02A00` (0.4970, +17.11 cent). Note 60 = C4 at `F` needs the ARM word; with the image word it needs 43,828.9 Hz. | VERIFIED |
| Why 43,156.4 (bank derivation) | Wheel 0 = C1 was solved at the **unity rate factor** `DM(0xC27C5) = 0x40000000`. The ARM writes the same table entry `0x3FA02A00` there (-10.16 cent). `43,402.34 * 2^(-10.16/1200) = 43,148.5`; the remaining 0.3 cent is the tune-table granularity (0.78 cent steps) seen in the ladder (table B sits 0.35 cent below the grid). | VERIFIED |
| The two readings were 27 cent apart | They err in opposite directions: 43,829 is 17.1 cent above `F` (image word below the ARM word, so the solved rate is too high), 43,156.4 is 9.8 cent below `F` (unity word above the ARM word, so it is too low). 17.1 + 9.8 = 26.9 cent = the 1.6 % of [b3-generator](b3-generator.md). The 12-TET anchor (A4 = 440) is the same in both. | VERIFIED (arithmetic) |
| SY frame rate (OQ22) | The SY saw fade words are `B = 5000 / 43,402.0`, `A = 15000 / 43,402.0` to float32 precision (section 3.4). The SY runs at the same rate. | VERIFIED |
| FX frame rate | The ARM converts delay times with `samples = trunc(ms * F / 1000)`: six exact words reproduced (section 3.2). | VERIFIED |
| SMP image | Not measured on its own: the ARM keeps `F` in one global (`0xC01BF43C` stores the literal into `[0xC05A5FD4]`), not one per DSP; no SMP-specific measurement. | ASSUMED |
| ARM control tick | The tick routine `0xC005019C` loads `24,000 = 0x5DC0` counts per tick; 1 kHz needs a 24 MHz count clock, but the AM1705 PLL / crystal are not programmed by this image (the PLL0 literal is only read). Unchanged: **1 kHz ASSUMED** (OQ22). One tick is `F / 1000 = 43.4` DSP frames. | ASSUMED |
| Origin of the number | The DSP images slave to an external codec clock (section 4): the SPORTs run with `ICLK = 0`, `IFS = 0`; the rate is not programmed in the DSP. `CGU0_CTL = 0x1400` (`MSEL = 20`), so `CCLK = 20 * CLKIN`; `CCLK / F = 10,240` for `CLKIN = 22.222 MHz` (= 44.444 / 2). | ASSUMED (crystal) |

Spec line: the DSP frame rate is `FIRMWARE_FRAME_RATE_HZ = 43,402.34375` (`crates/nord-core/src/organ/lane_mix.rs`).

## 2. Why the old numbers disagree, in one table

Everything below is `f = F * cycles_per_frame`, with `cycles_per_frame = step / 2^32 / frames_per_call`.

| Source | step word | Frames per call | Anchor | `F` that satisfies the anchor | At `F = 43,402.34` |
|---|---|---|---|---|---|
| B3 generator, note 60, image word `0x3F000000` | 51,275,456 | 2 | C4 = 261.6256 Hz | 43,828.9 Hz (the "43,829") | 259.079 Hz, -16.93 cent |
| B3 generator, note 60, **ARM word `0x3FA02A00`** | 51,784,672 | 2 | C4 | **43,397.9 Hz** | **261.652 Hz, +0.18 cent** |
| Pipe bank wheel 0, table B element 0, unity `0x40000000` | `0x635300 * 0.5` | 1 | C1 = 32.7032 Hz | 43,156.4 Hz (the "43,156.4") | 32.886 Hz, +9.6 cent |
| Pipe bank wheel 0, table B element 0, **ARM word `0x3FA02A00`** | `fracmul(0x635300, w)` | 1 | C1 | **43,410.3 Hz** (+0.32 cent) | **C1 -0.35 cent (whole ladder)** |
| Vox note 0, `0x62C2E7`, `m = 0.5` | `0.5 * 0x62C2E7` | 1 | C1 (key map `note - 24`, ASSUMED) | **43,402.341 Hz** (+0.0001 cent) | 32.7032 Hz, +0.0001 cent |

So neither old value was a frame rate: each was the frame rate multiplied by the (unknown) fine tune word the ARM writes. The ARM table `0xC027D9D8` has a step of 0.78125 cent (1/128 semitone) and index `cents * 64 / 100 + base + 0x1F3`, so the default (cents 0, base 0 because `DM(0xC05A0464)` is BSS) is `0x1F3`, 13 steps (10.16 cent) below the unity index `0x200`. The same index feeds the B3 generator (handler `0xC011D1F4`), but the replay harness never ran the Fine Tune handler, so the B3 word stayed at the image value.

## 3. Evidence

### 3.1 The ARM literal (VERIFIED)

```bash
python tools/dsp_frame_rate.py literal target/fw_unpacked/bank0_CODE.bin
# fs    pool 0xc01bf74c     (loaded at 0xC01BF43C: global sample-rate word, first use)
# 2 fs  pool 0xc0221db8     (0xC02212B4: object field [sb+0x48] = 2 fs when the mode is 2)
# fs    pool 0xc0221e44
# fs/2  pool 0xc027a6bc
# fs    pool 0xc027a6c0     (a 3-entry table fs/2, fs, 2 fs)
# 2 fs  pool 0xc027a6c4
```

`0x47298A58` is exactly `43,402.34375 = 44,444,000 / 1024` (a constant written as decimal `44.444 MHz / 1024`). The ARM also carries `fs / 2 = 21,701.0` as `0x46A98A00` (pool `0xC01BD060`) and `1 / 21,701 = 4.6081e-5` (`0x384146E9`, pool `0xC01BD05C`), used in `0xC01BCD2C`: a frequency divided by the Nyquist frequency. That variant implies `F = 43,402.00`; the two differ by 0.014 cent.

### 3.2 FX delay words (VERIFIED)

`Edt_Effects_kDlyRateClkDiv` writes the context word `c[0x15]` as a float of integer value ([fx-contexts](fx-contexts.md) section 5.1). The Delay Time display table (`DELAY_TIME_MS`, `crates/nord-core/src/fx/tables.rs`) gives the milliseconds at `step = v >> 7`.

```bash
python tools/dsp_frame_rate.py delay
#   20.0 ms  ARM      868  model      868  ok
#  150.0 ms  ARM     6510  model     6510  ok
#  625.0 ms  ARM    27126  model    27126  ok
#  750.0 ms  ARM    32551  model    32551  ok
# 1250.0 ms  ARM    54252  model    54252  ok
#  250.0 ms  ARM    10850  model    10850  ok     (the constant c[0x16])
```

`trunc(ms * 43,402.34375 / 1000)` reproduces all six; 43,400 Hz misses three (54,250, 32,550, 27,125) and 48 kHz all of them. The other rows differ because the display table is rounded to about 1 % (for example 411 ms is 17,836 words = 410.97 ms). The word saturates at `61,438` for `v <= 128` (1,415.6 ms, the buffer limit; the display says 1500 ms).

### 3.3 Pipe bank at the ARM Fine Tune word (VERIFIED)

```bash
python tools/dsp_frame_rate.py bank target/fw_unpacked/bank0_CODE.bin
# table A (detuned pair) ARM 0x3FA02A00    mean -0.24 sd 1.04 cent from C1 12-TET
# table A (detuned pair) unity 0x40000000  mean +9.92 sd 1.04 cent from C1 12-TET
# table B (exact)        ARM 0x3FA02A00    mean -0.35 sd 0.01 cent from C1 12-TET
# table B (exact)        unity 0x40000000  mean +9.81 sd 0.01 cent from C1 12-TET
```

All 92 wheels, not only wheel 0: with `F` and the ARM word, wheel `k` is `C1 * 2^(k/12)` (A4 = wheel 45 = 439.95 Hz for table A). The 0.78 cent step of the tune table accounts for the -0.35 cent offset (half a step is 0.39 cent). Unity tune would put every wheel 9.8 cent sharp.

### 3.4 SY saw fade constants (VERIFIED)

`B = 0x3DEBEF11`, `A = 0x3EB0F34D`, `C = 0x408AE2EA` ([synth-engine](../specs/synth-engine.md) section on the saw; `crates/nord-core/src/synth/hw_saw.rs`):

```bash
python tools/dsp_frame_rate.py sy
# from_B: 43401.995 Hz     5000 / B
# from_A: 43401.994 Hz    15000 / A
# from_C: 43401.995 Hz    10000 * C
```

The fade is unity up to 5.0 kHz and silent at 10.0 kHz (`g = 1` for `i <= B`, zero at `(A + B) / 2`, increments `i` in cycles per frame). Both round numbers appear only for `F = 43,402`; at 48 kHz the knee would be 5.53 kHz. float32 resolution at this size is 0.003 Hz, so the SY design value is 43,402.00 (rounded to an integer) and the ARM's literal is 43,402.34; the Vox rate word (section 3.6) matches the literal, hence the constant used in Rust.

### 3.5 B3 generator (VERIFIED)

```bash
python tools/dsp_frame_rate.py b3
# image  implied fs 43828.9 Hz, note 60 at FS 259.079 Hz (-16.93 cent)
# arm    implied fs 43397.9 Hz, note 60 at FS 261.652 Hz (+0.18 cent)
# unity  implied fs 43144.0 Hz, note 60 at FS 263.192 Hz (+10.33 cent)
```

`step(note) = ((PHASE_INC[note % 12] * w + 2^30) >> 31) << (note / 12)` with the fine tune word `w` ([b3-generator](b3-generator.md) section 1). Two frames per call is VERIFIED by agreement: one frame per call would put the 8' of key 60 an octave above C4 (523.3 Hz) while the bank, the SY and the FX all give the same `F`. The ladder keeps its 0.94 cent deviations from 12-TET: over notes 24..115 `firmware_note_hz` stays within 1.3 cent of A4 = 440.

### 3.6 Vox post stage (VERIFIED arithmetic, ASSUMED key map)

`rate_0 = 0x62C2E7` at `m = 0.5` is `0.5 * 0x62C2E7 / 2^32 * F = 32.7032 Hz`, **C1 + 0.0001 cent** (the implied rate is 43,402.341 Hz, 6e-8 from the ARM literal: the word was built as `round(C1 / F * 2^33)`). This is the sharpest readout: it separates 43,402.34 from the 43,402.00 of the SY constants (0.014 cent) and rules out the unity-tune 43,156.4. At 43,156.4 Vox was 9.8 cent flat: that is the "why Vox sits 9.9 cents below the Pipe bank" of OQ17. The Pipe bank was compared at unity tune (+9.6 cent), so the 9.9 cent was the Fine Tune word, not a Vox detune. Whether the ARM's default Fine Tune write to `DM(0xE20E0)` leaves `m = 0.5` (so that Vox sits on C1 together with the Pipe bank) is not read: ASSUMED.

## 4. DSP-side clock facts (VERIFIED, bearing on what cannot be derived)

`sharc-run --sc57x 1` over the init of PNO Core1 (entry `0x121007`) and ORG Core1 (`0x125C88`), writes read with `--dump`:

| Register | Value | Meaning |
|---|---|---|
| `CGU0_CTL` (`0x3108D000`) | `0x1400` | `MSEL = 20`, `DF = 0`; the same `BIT SET USTAT1 0x1400` write appears in FX, PNO, ORG (`0x240BD4`, `0x24081C`, `0x24B884`) |
| `CGU0_DIV` (`0x3108D00C`) | `0x4241` | `CSEL = 1`, `S0SEL = 2`, `SYSSEL = 2`, `S1SEL = 2`: `CCLK = VCO`, `SYSCLK = VCO / 2`, `SCLK0 = SCLK1 = VCO / 4` |
| `SPORT0` / `SPORT1` `CTL_A`, `CTL_B` (`0x31002000`, `0x31002080`, `0x31002100`, `0x31002180`; which halves are armed differs per image) | `0x020821F1` or `0x010821F1` | `SPEN = 1`, `SLEN = 31` (32-bit words), `FSR = 1`, `FSED = 1`, `SPTRAN = 1` (transmit) or `SPENSEC = 1` (receive); **`ICLK = 0`, `IFS = 0`**: clock and frame sync come from outside |
| `SPORT_DIV`, `PCG0_*` (`0x310C9300..`) | 0 | no divider, no precision clock generator, no ASRC set up |

So the images **never program the audio rate**: the SPORTs are slaves of an external codec whose clock the DSP cannot see, and the PLL multiplier does not give an audio rate without `CLKIN`. The frame rate therefore has to come from the ARM, whose conversions assume 43,402.34 Hz (section 3.1), and from the pitch and time constants the Clavia engineers designed into the DSP images for that rate (sections 3.2 to 3.6). ASSUMED: `CLKIN = 22.222 MHz` and `CCLK = 444.44 MHz = 10,240 * F` (the numerology of `44.444 MHz / 1024`); no board photo of the crystal was read.

## 5. Reproduction and tests

```bash
python tools/dsp_frame_rate.py {literal,bank} target/fw_unpacked/bank0_CODE.bin
python tools/dsp_frame_rate.py {delay,sy,b3}
uv run pytest tools/tests/test_dsp_frame_rate.py
cargo test -p nord-core --lib frame_rate   # lane_mix / firmware_bank / b3_wheels / vox_bank / hw_saw / fx::tables
cargo run --release -p nord-core --example render_hash -- out.bin && sha256sum out.bin
# 25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a, unchanged before and after
```

Rust tests: `firmware_bank::tests::frame_rate_follows_from_the_arm_fine_tune_word`, `b3_wheels::tests::frame_rate_follows_from_the_arm_fine_tune_word`, `vox_bank::tests::frame_rate_puts_note_zero_on_c1`, `hw_saw::tests::fade_constants_are_5_and_15_khz_at_the_dsp_frame_rate`, `fx::tables::tests::delay_time_words_of_the_arm_are_ms_times_the_dsp_frame_rate`, `synth::gain_dezip::tests::pole_is_the_hardware_value_at_the_frame_rate_and_keeps_the_time_constant`.

## Implications

- `FIRMWARE_FRAME_RATE_HZ` is VERIFIED at 43,402.34375 Hz (was 43,156.4, ASSUMED). The engine paths that convert per frame to per second change slightly: the Vox bank pitch (+9.8 cent, now on C1), the Vox LFO (5.941 to 5.975 Hz), the lane-mix and wheel-weight smoothers (100 frames = 2.30 ms), the Vox output stage. `SY_FRAME_RATE_HZ` follows (the opt-in gain de-zipper pole is 1.08 ms per 47.1 frames). Default paths are untouched: `render_hash` stays `25baa387...7b59a`.
- Oracle comparisons that match a Rust engine to the oracle at a free sample rate keep their rate parameter: `organ_ref` defaults to the unity-tune match rate (43,156 Hz), but Vox runs (`organ_ref --mode vox`) need `--rate 43402.34375` now, so that `scale = F / fs = 1`.
- The pitch anchor of the `Firmware`, `B3Wheels` and `Vox` sources is now measured: wheel 0 = C1 and note 60 = C4 hold within 0.4 cent. The engine's exact-C anchor of `B3Wheels` is kept (`note_hz`); `firmware_note_hz` is the firmware's own pitch (at most 1.3 cent from 12-TET).
- Every "48 kHz" statement about firmware time constants is a unit error of 10 %: the SY gain smoother is 1.08 ms, not 0.98 ms; the `0.9908` / `0.99078` FX per-call smoothers are 2.30 ms per 100 frames (the "83 us at 48 kHz" per 4-frame block is 92 us).
- The sample-pitch paths (piano `.npno`, SMP) keep their own conversion from the file's sample rate; nothing here changes the Rust playback ratio, but a ratio hidden in the ARM (file rate over `F`) must be read against 43,402 Hz, not 48 kHz.

## Follow-ups

1. Find the ARM write of `DM(0xE20E0)` (Vox Fine Tune) and the default value of `m`: does Vox sit on C1 together with the Pipe bank (OQ17)?
2. Apply the SY fade knee in Hz: `hw_saw::firmware_saw_fade` uses the increment at the engine rate (5.53 kHz at 48 kHz); the hardware knee is 5.0 kHz at any rate, so pass `inc * fs / F` (opt-in `SawModel::Firmware`; not changed here).
3. Check the SMP image's sample pitch against `F` (ARM step from root note and file rate).
4. Read the board crystal (photo) and the ARM PLL (`PLL0` at `0x01C11000` is only read in this image: the bootloader programs it) to turn the 1 kHz ARM tick from ASSUMED into VERIFIED, and to confirm `CCLK / F = 10,240`.
5. Rescale the per-block smoothing constants quoted in microseconds at 48 kHz in [fx-type-map](fx-type-map.md) and [fx-routine-models](fx-routine-models.md) when those paths are wired.

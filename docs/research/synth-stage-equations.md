# Synth stage equations: filter modules of the Core2 stage engine, envelope rate tables, strip control chain

- **Checked:** 2026-11-13
- **Sources (local, never committed):** ORG core 2 image (OS v1.66, `python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram DIR`), ADI listing (`tools/adi_dis.py`), `sharc-org` complete passes (`crates/nord-sharc`), `tools/synth_stage_oracle.py` (driver), `tools/synth_filter_model.py` (models). Builds on [synth-voice-control](synth-voice-control.md) (PR #121), [org-stage-map](org-stage-map.md), [pno-module-chain](pno-module-chain.md), [pno-full-pass](pno-full-pass.md).
- **Question:** What are the filter and envelope equations of the synth strip stages (`0x12566E`, `0x1257B2`, `0x125913`, `0x125A50`, `0x1215FB`), what do the strip words mean, and what does `crates/nord-core/src/synth/` need to match them?

Tags (`.agents/rules/provenance-in-code.md`): **VERIFIED** = reproduced by a command below (oracle or bit-exact model), **EMPIRICAL** = fitted or read from a listing, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Calling convention of the word-1 stages | The pass calls the B stage of channel `c` in SIMD mode with `I8 = DM(0xC0B8C + 0x26 c)`, `I3 = 0xC2942 + 0x34 c` (frame), `I4 = 0xC1F96 + 0x36 c` (A record: 4 samples at `I4-13, -11, -9, -7`, PEy lane at +1, filtered in place), `I5 = 0xC28FA` (constants). Section 2. | VERIFIED |
| `0x12566E` | **4-pole low pass**: two identical sections, each the bilinear transform of `1 / (s^2 + P s + 1)` (cutoff `R = tan(pi fc / fs)`, `Q = 1/P`), trapezoidal input, per-section clip of the band state at +-1. Bit-exact Python model (coefficients and output words equal to the oracle). | VERIFIED |
| `0x125913` | The same sections, output = clipped band state: **4-pole band pass** (two cascaded band-pass sections), input gain from table `0xB0D90` (unclipped). Bit-exact model. | VERIFIED |
| `0x125A50` | The same coefficients, no input averaging, output = `u h - c' - k a`, `h = 1 - g^2/4 - g k/2`: **4-pole high pass**. Bit-exact model. | VERIFIED |
| `0x1257B2` | A different topology (coupled form, 6 coefficient fields, tables `0xB0A9F`, `0xB0C0D`, `0xB0C8E`; the cutoff index is `R/2`): DC gain 1, no resonance peak at index >= 100, -3 dB at 0.036 cycles/sample for `R = 0.3`. Equations **not** extracted; impulse words in section 5. | VERIFIED (behaviour), open (equations) |
| Cutoff law | `fc = fs atan(R) / pi` (bilinear warp). `R` is the float in `DM(I3-30)`; the strip -> `R` law is table driven (section 6) and **not** closed. | VERIFIED (R -> fc), open (strip -> R) |
| Resonance law | table position word `DM(I3-28)`: byte 3 = index 0..127, low 24 bits = fraction; `P = B[idx]` interpolated, `B = 0.70710678 (128 - idx) / 64` for `idx >= 38`, `B[0] = 2`; `Q = 1/P` runs 0.5 .. 90.5, input gain `A = min(1, 0.99 (128 - idx) / 64)`. | VERIFIED (tables), EMPIRICAL (closed forms) |
| Envelopes | The 9 ADR strip words (`w26..w34`) **are read** by the pass: the per-channel installer (`0x12720F..0x127281`) turns each into a per-pass rate coefficient with a table (index = top byte of the word clipped to +-0.5, i.e. -64..64) and stores 9 coefficients at `DM(0xC0E21 + ...)`. The segment generator that consumes them is not in the ORG core 2 pass. Rate tables measured (CSV). | VERIFIED (words -> coefficients), open (generator) |
| Oscillator | unchanged from [synth-voice-control](synth-voice-control.md) s5 (`0x12501F`, VERIFIED saw). | VERIFIED |

## 2. Stage frame and calling convention (VERIFIED, listing `0x127B01..0x127BA8` and runs)

```text
bit set mode1 0x2000                          SIMD on (PEx = even word, PEy = word + 1, oracle model)
channel c = 0..7:  I8 = DM(0xC0B8C + 0x26 c)  B stage entry         I2 = DM(0xC0B86 + 0x26 c)
                   I3 = 0xC2942 + 0x34 c      frame, fields I3-32 .. I3+18
                   I5 = 0xC28FA               constants: I5-32 = 0.25, -30 = 0.5, -28 = 1.0, -26 = 2.0, -24 = 4.0
                   I4 = 0xC1F96 + 0x36 c      A record, I0 = 0xC0B88 + 0x26 c
```

The A stage (osc or any stage in `DM(0xC10B8 + 0x18 s)`) runs first (8 scalar calls) and writes its 4 samples to the A record; the B stage reads them from `I4-13, I4-11, I4-9, I4-7` and writes the result back to the same words. The frame words are x/y pairs (even word PEx, odd word PEy); both lanes carry the same value in normal use.

| Frame offset | Written by | Meaning |
|---|---|---|
| `I3-30` | pass control code (`0x1262E2`) | `R`, cutoff coefficient (float; default `0x3EB8BE4E` = 0.36083) |
| `I3-28` | pass control code (`0x12696B`) | table position word `W28` (default `0x40000000` = index 64) |
| `I3-26`, `-22`, `-20` | the stage prologue | `cin`, `g`, `k` |
| `I3-10 .. I3` | the stage | section states `a1 b1 c1 | a2 b2 c2` (word 0 of each pair at even offsets) |
| `I3+16`, `+18` | the stage prologue | copies of `DM(I5-28) = 1.0` |

The control stores rewrite `-32`, `-30`, `-28` in **every** pass, so the oracle runs the stage with these three stores replaced by NOPs (`patch_controls`, 3 short words each at pc `0x1262E2`, `0x1262E5`, `0x12696B`) and pokes `R` and `W28` once. VERIFIED effect: the poked words survive all passes. Other pass code is untouched; the A stage of slot 0 is replaced by the null stage `0x125B79`.

## 3. Filter family (VERIFIED, bit-exact models in `tools/synth_filter_model.py`)

### 3.1 Prologue (shared by `0x12566E`, `0x125913`, `0x125A50`)

```text
idx = W28 >> 24                fr = (W28 & 0xFFFFFF) / 2^24
cin = T1[idx] + (T1[idx+1] - T1[idx]) fr        T1 = A at DM(0xB0D0F) (0x12566E, 0x125A50), C at DM(0xB0D90) (0x125913)
P   = B[idx]  + (B[idx+1]  - B[idx])  fr        B at DM(0xB1015)
N   = 1 / sqrt(1 + R P + R^2)                   RSQRTS seed (8 bit) + two Newton steps y <- 0.5 y (3 - x y^2), 40-bit registers
g   = 2 R N  -> DM(I3-22)     k = P N  -> DM(I3-20)     cin -> DM(I3-26)     (stores truncate to 32 bit)
```

Checked on 9 (index, fraction, R, amplitude) rows for `0x12566E` and 6 rows each for `0x125913`, `0x125A50`: the three coefficient words and 64 output words per row equal the oracle (`0` mismatches).

Tables (CSV [data/synth-filter-tables.csv](data/synth-filter-tables.csv), 129 rows, float32 values of the image):

| Table | Closed form | Tag |
|---|---|---|
| `A` (`0xB0D0F`) | `min(1, 0.99 (128 - i) / 64)` (`A[64] = 0.99`) | EMPIRICAL (max error 2e-7) |
| `C` (`0xB0D90`) | `0.99 (128 - i) / 64`, not clipped (`C[0] = 1.98`) | EMPIRICAL |
| `B` (`0xB1015`) | `0.70710678 (128 - i) / 64` for `i >= 38`; `B[0] = 2.0` falling to `B[37] = 1.01086` (no closed form) | EMPIRICAL |

### 3.2 `0x12566E`: 4-pole low pass

Two sections in series (states `a, b, c` at `I3-10, -8, -6` and `I3-4, -2, 0`; `L = DM(I5-28) = 1.0`):

```text
section 1: u = cin x          section 2: u = output of section 1 (no cin)
  m  = (u + b) / 2            b' = u
  c' = c + g a
  v  = m - c' - k a
  a' = clip(a + g v, +-L)
  out = (c' + c) / 2
```

Closed form without clip: `H1(z) = g^2 (z+1)^2 / (4 z (z^2 - (2 - g^2 - g k) z + 1 - g k))`, DC gain 1, so `|H| = cin |H1|^2` and the poles of a section are the bilinear image of `s^2 + P s + 1` with `s = (z-1) / (R (z+1))`: **analog natural frequency 1 maps to `fc = fs atan(R)/pi`, `Q = 1/P`**, both sections identical. Both zeros at Nyquist.

Measured with impulse responses (oracle, 2,048 samples, FFT) and reproduced by `lp4_features`:

| `R` | idx | cin | oracle peak (abs / cin) at f/fs | model peak at f/fs | oracle -3 dB (f/fs) | model |
|---|---|---|---|---|---|---|
| 0.30 | 64 | 0.99 | 2.2629 (2.2858) at 0.0809 | 2.2857 at 0.0809 | 0.1199 | 0.1199 |
| 0.10 | 64 | 0.99 | 2.2628 at 0.0275 | 2.2857 at 0.0275 | 0.0417 | 0.04175 |

Resonance law at `R = 0.3` (nominal `fc = 0.0928 fs`; model `lp4_features`, relative to DC, per 4-pole cascade):

| idx | `A` | `P` | `Q` | peak | peak f / nominal | -3 dB f / nominal |
|---|---|---|---|---|---|---|
| 0 | 1.000 | 2.0000 | 0.50 | none | - | 0.445 |
| 32 | 1.000 | 1.1250 | 0.89 | 1.3 dB | 0.62 | 1.05 |
| 64 | 0.990 | 0.7071 | 1.41 | 7.2 dB | 0.87 | 1.29 |
| 96 | 0.495 | 0.3536 | 2.83 | 18.3 dB | 0.97 | 1.40 |
| 120 | 0.124 | 0.0884 | 11.3 | 42.2 dB | 1.00 | 1.43 |
| 127 | 0.015 | 0.0110 | 90.5 | 78.3 dB | 1.00 | 1.43 |

The input gain `A` falls with the index so the peak stays bounded; the **clip of the band states at +-1** is the only non-linearity (drive appears as clipping, no saturating curve). The slope above the cutoff is -24 dB per octave in the s domain (two sections of 12 dB), falling faster near Nyquist (zeros).

### 3.3 `0x125913`: 4-pole band pass

Same coefficients with `C` instead of `A`. Per section (layout: previous input `x` at `I3-6` / `I3`, low state `c` at `-8` / `-2`, band state `a` at `-10` / `-4`):

```text
m = (u + x) / 2     c' = a g + c     v = m - c' - k a     a' = clip(a + g v, +-L)     x' = u
out = a'            (the second section takes the first section's a' as u)
```

DC gain 0; oracle peak 2.578 at 0.0928 (`R = 0.3`, idx 64), i.e. at the nominal cutoff. Output words of the oracle for a unit impulse (`R = 0.3`, idx 64): `3D8C230E 3E43DED8 3E300A5D BCB4C2FE BE6D1417 BEACB125 BE98A25C BE1571AF` (test `test_band_pass_and_high_pass_stages_match_the_oracle_impulse_words`).

### 3.4 `0x125A50`: 4-pole high pass

`h = 1 - g^2/4 - g k / 2` (register `f4`, 40 bit). Per section `c' = a g + c; v = u h - c' - k a; a' = clip(a + g v, +-L); out = v`; two sections, the first fed with `cin x`. DC gain 0, no input averaging; impulse words `3F15794D BF340DC7 BEC81292 BBA7E32B 3E89B484 3EB438AA 3E871768 3DAD12F1`.

### 3.5 `0x1257B2` (not decoded)

Prologue: `R/2` is converted to fixed point (`FIX BY 31`), its byte 3 and 24-bit fraction interpolate the tables `0xB0C0D` and `0xB0C8E` (a cutoff-dependent correction), `W28` interpolates `0xB0A9F`; six coefficient fields are stored (`I3-24`, `-22`, `-20`, `+10`, `+12`, `+14`). The loop is a coupled two-resonator form with cross terms. Frame after the prologue (`R = 0.3`, idx 64): `-26 = 1.0`, `-24 = 0x3F71B3C4`, `-22 = 0x3EFC7E77`, `-20 = 0x3FD26E96`, `+10 = 1.0`, `+12 = 0x3D64ED8B`, `+14 = 0x3FFC3D7C`. Impulse words: `3F71B3C4 BD3CFACA 3DA6984A 3E048AA5 3DBB7A57 3D44A442 3C7AA96B BBCB55BB`. Measured: DC gain 1.0000; `R = 0.3`: idx 64 peak 1.397 at 0.0267, -3 dB at 0.1123; idx 100 -3 dB at 0.0356, no peak; idx 120 -3 dB at 0.0552; `R = 0.1`, idx 64: -3 dB 0.0298. It is a low pass of lower effective cutoff than `0x12566E` at the same `R`, with resonance at low index only: the ladder-like variant. Model open.

Another member of the family sits at `0x1253BD` (listing `0x1253BD..`): the `0x12566E` prologue with table `0xB0F94` instead of `B`, same loop as `0x12566E` (ASSUMED, not run).

## 4. How the frame words are made from strip words (partly open)

The pass control block (`0x126000..0x127300`) runs per channel before the stages:

| Step | Code | Reading | Tag |
|---|---|---|---|
| Parameter smoothers | `0x1260BC..0x1261CC`, `0x1261CD..0x12627D` | strip word `w <- 0.995 w + 0.005 common` (float, words 8, ..., `0xE0008 + 0x36 m`, partner `DM(0xE02EE + ...)`) and `w <- 0.99609375 w + 0.0048828 common` (Q31, words 2, 3, ..., partner `DM(0xE02E8 + ...)`): the strip word chases the per-layer common block, so the ARM-written common block is the *target* | VERIFIED (listing) |
| Pitch sum -> tables | `0x126280..0x1262EE` (channel 0), then one copy per channel | `r0 = w2 + C2716 + (w4 + C2718) LFO ssf << 2 + w6 ...` (strip words `2, 4, 6` of the strip pointer `[0xC0B83 + 0x26 c] - 30`), clipped to `+-DM(0xC1478) = 0x58000000`, index = 9-bit field at bit 23 (`DM(0xC147A) = 0x257`), 23-bit fraction (`DM(0xC147C) = 0x5C8`); two interpolated tables `DM(0xC1476 / 0xC1477) = 0xB069B / 0xB0BA2` give the oscillator ratio (`0xC1F79`, 24 steps per octave, `T6[i + 1] / T6[i] = 2^(1/24)`) and `R` (`DM(I3-30)`) | EMPIRICAL (read), T6 law VERIFIED (sweep) |
| Resonance word | `0x126926..0x12696B` | `W28 = FIX((w10 + C271C) x lfo)`, `lfo = DM(0xC1E34 + ...)`, i.e. strip words 10, 20 and the LFO output (default LFO 0.5, default `W28 = 0x40000000`) | EMPIRICAL (read) |
| Sweep | `E0002` of strip 0 swept over +-0.59 Q31 | `T6` follows the exponential law, but `DM(I3-30)` of channel 0 stayed 0.36083 (= `B069B`-side table entry 0 of the second table) and `W28` stayed `0x40000000`: channel 0 is the oscillator-side chain; the channel whose B stage is the filter was not isolated. Sweeping strip 6 word 2 (channel 1, frame `0xC2976`) did not move `R` or `W28` either | VERIFIED (negative) |

So the end-to-end law strip word -> `R` is **open**; what is closed is `R -> fc` (section 3.2), `W28 -> A, P, Q` and the code path. The strip words the ARM writes for the synth cutoff (strip 6 words 2/3, a PEx/PEy pair) feed the channel-1 control block at `0x126932` (`i4 = DM(0xC0BA9) = 0xE0194`).

## 5. Strip word meaning (what is established)

| Strip word (`DM(0xE0000 + 62 k + w)`) | Meaning | Tag |
|---|---|---|
| 0 | A stage id (`0x1215FB` oscillator for synth); low 24 bits compared with `0x12082F` by the installer | VERIFIED (listing) |
| 1 | B stage id (`0x12566E` filter); low 24 bits compared with `0x120C68`, any other value leaves `DM(0xC0B8C + 0x26 c)` unchanged: **the oracle installs the stage by writing that word directly** | VERIFIED (run) |
| 2, 3 | Q31 pitch / cutoff word pair, smoothed toward common block word 0 (0.996 / 0.0049); summed into the pitch word that indexes `T6`/`T7` | EMPIRICAL |
| 4..7 | further Q31 modulation inputs (word 7 = `0x7F000000` idle) | EMPIRICAL |
| 8 | float, smoothed toward common block word 6 (`0.995 w + 0.005 c`) | VERIFIED (listing) |
| 10, 20 | enter the `W28` formula (resonance index, envelope amount) | EMPIRICAL |
| 26..28 | amp envelope attack, decay, release: Q31 words, top byte -> rate table (section 7) | VERIFIED |
| 29..31 | filter envelope attack, decay, release | VERIFIED |
| 32..34 | osc (pitch) envelope attack, decay, release | VERIFIED |

## 6. Oracle runs behind the numbers

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram SRAM_ORG
cargo build --release -p nord-sharc
export NS4_SHARC_ORG=target/release/sharc-org NS4_ORG_IMAGE=SRAM_ORG/core2_sram.bin
python - <<'PY'                                 # patched image: the three control stores become NOPs
import synth_stage_oracle as o, pathlib
pathlib.Path("core2_patched.bin").write_bytes(o.patch_controls(pathlib.Path("SRAM_ORG/core2_sram.bin").read_bytes()))
PY
# run a stage over samples, read the filtered samples and frames back (4 samples per pass):
#   o.run_stage(org, "core2_patched.bin", 0x12566E, {-30: (R, R), -28: (W28, W28), -10: (0, 0), ...}, xs, xs)
```

`tools/synth_stage_oracle.py` writes a `--msg-script` with `PASS ADDR VALUE` lines (`--msg-base 0`), so a pass can poke the 8 input words and read them back through `--buf` / `--csv`; one pass takes about 4,400 instructions, 2,048 samples about 0.5 s.

## 7. Envelope rate tables (VERIFIED words, open generator)

The per-channel installer of the pass (`0x12720F..0x127281`, listing) reads the nine ADR words, adds a modulation word, clips to `+-DM(0xC135A) = 0x40000000`, takes byte 3 as a signed index (`fext 24:8 (se)`) and loads a coefficient from tables selected by `DM(0xC1358/0xC1359) = 0xB080D / 0xB090F` (and `0xB088E` when the voice-mode word is positive). Nine coefficients are stored (when the apply flag `DM(0xC136C) > 0`, two passes after the strip write) at `DM(0xC0E21, 0E24, 0E28 | 0E35, 0E38, 0E3C | 0E41, 0E44, 0E48)` (amp A/D/R, filter A/D/R, osc A/D/R) with the second copy at +0x2C. Poking each of `E001A..E0022` changes exactly these words (VERIFIED, 12-pass diff of all `DM(0..0xDFFFF)`).

Sweep of the index -64..64 (3 passes per value): [data/synth-env-rate-table.csv](data/synth-env-rate-table.csv). Findings:

| Fact | Value | Tag |
|---|---|---|
| Attack coefficient `cA(idx)` | 0.09216 at idx <= -64, 4.505e-5 at 0, 1.24e-6 at 63, 1.0e-6 at 64 (one table for all three attack words) | VERIFIED |
| Decay / release coefficient `cD(idx)` | `1 - cD` = 0.06829 at -64, 1.4257e-4 at 0, 5.66e-6 at 63, 5.0e-6 at 64 (one table for decay and release) | VERIFIED |
| Law | monotonic, neither a pure exponential nor a power law: `1/cA` = 10.9, 158.6, 1178, 5847, 22198, 69638, 189394, 460830, 1.0e6 samples at idx -64, -48, -32, -16, 0, 16, 32, 48, 64 (ratio per 16 steps falls from 7.4 to 2.2). Use the CSV | VERIFIED (values), closed form open |
| Meaning | `cA` reads as the per-pass rate of a segment moving toward the target, `1 - cD` as the per-pass decay of a one-pole; time constants 1 / cA passes: 0.2 ms .. 21 s at 48 kHz if one pass is one sample (**ASSUMED**: a pass is 2 frames) | ASSUMED |
| Consumer | no store or load site of `DM(0xC0E21 + ...)` outside the installer was found in the ORG core 2 pass; the ADSR runtime is not in the code the pass reaches (installer slot modules `0x12082F` / `0x1208D6` ramps are the nearest candidates, not run with real data) | open |

## 8. Rust to-do (`crates/nord-core/src/synth/`)

Status: items 2 to 5 and the rate-table lookups of item 6 are ported as opt-in paths (`synth/fw_filter.rs`, `synth/fw_filter_tables.rs`, `synth/fw_env_rates.rs`, spec [synth-engine](../specs/synth-engine.md) 4.1); the strip word to `R` law (item 3, cutoff in Hz), the `kFltType` selection (item 4) and the ADR generator (item 6) stay open. Float32 Rust against this document's bit-exact model: coefficients 1.1e-7 relative, output words 6.0e-7 absolute, low-pass impulse spectrum against the closed form 6.3e-5 relative.


1. **Oscillator**: VA saw of [synth-voice-control](synth-voice-control.md) s5 (`g(inc)`, two-increment falling PolyBLEP), VERIFIED.
2. **Filter core**: replace the ZDF ladder by the **two-section trapezoidal resonator** of section 3.2: per section `c' = c + g a; v = (u + b)/2 - c' - k a; a' = clip(a + g v, +-1); b' = u; out = (c' + c)/2`, two sections in series, coefficients `g = 2 R N`, `k = P N`, `N = 1/sqrt(1 + R P + R^2)`, input gain `cin`. 40-bit intermediates are not needed (float32 differs by 1e-7 relative). Tag the constants `VERIFIED` against `tools/synth_filter_model.py` (output words) and keep the clip limit 1.0 (`DM(I5-28)`).
3. **Cutoff and resonance laws**: `R = tan(pi fc / fs)` (fc in cycles per sample, no further warp); resonance from the 128-entry tables `A`, `B` (CSV) with linear interpolation of a 7.24-bit position (`Q = 1/B`, `cin = A`); add `C` for the band-pass type. The cutoff-in-Hz of a parameter value goes through the pitch tables `0xB069B / 0xB0BA2` (`R` and the oscillator ratio share the pitch index); extract them once the strip -> `R` path is closed (section 4).
4. **Filter types**: `kFltType` selects among `0x12566E` (LP4), `0x125913` (BP4), `0x125A50` (HP4) and `0x1257B2` (ladder-like LP, open); the selection id -> stage was not observed.
5. **Clip** is the only non-linearity (band state limited to +-1 inside each section, no saturator, no extra drive term found).
6. **Envelopes**: keep the display tables; the DSP-side rate tables are in the CSV (time words -> per-pass coefficients); the segment shape (linear or exponential) is open until the generator is located.

## 9. Open points

> Update: points 1 (strip word to `R`: word 3, closed) and 3 (the ADR generator: record chain in the pass) are answered in [synth-adr-and-strip-law](synth-adr-and-strip-law.md); point 2 has the `0x1257B2` pole table; point 5 (`kFltType`) stays open.

1. Strip word -> `R` / `W28` for the filter channel (needs a state where the channel is active; the sweeps of strip 0 and 6 word 2 did not move `R`).
2. Decode `0x1257B2` and `0x1253BD` (run them with the same driver; 6 fields, 3 tables).
3. The ADSR generator and its time base (which routine reads `DM(0xC0E21..)`).
4. Closed forms of `B[0..37]` and of the rate tables; key tracking and velocity scaling words.
5. `kFltType` -> stage id and the drive (`kFltDrive`) path.

## Implications

- The synth filter of the firmware is a four-pole (two resonant sections) bilinear low pass with a resonance range `Q = 0.5 .. 90`, a built-in input gain taper that bounds the peak, and a clip of +-1 on the internal band states; a Rust ladder with a tanh stage is not the same filter.
- The same prologue drives the band-pass and high-pass variants, so the type switch costs one table and one output tap.
- The ADR words are table lookups, not time values: a Rust model must apply the table, not a log law.

## Follow-ups

1. Close strip -> `R` with a loaded preset capture (`tools/arm_edit_capture.py` plus a key-down).
2. Port items 1 to 5 of section 8 with `VERIFIED` tags and the golden words of this document as unit tests.
3. Decode `0x1257B2`.

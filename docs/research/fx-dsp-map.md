# FX DSP image map (effect rack): pass structure, routine classes and a verified biquad

- **Checked:** 2026-11-12
- **Sources (local, never committed):** FX image of OS v1.66 (`dsp_image_0_FX.bin`, `python tools/ns4_dsp_ldr.py ... --sram DIR` gives `core1_sram.bin`, `core2_sram.bin`, entry `0x1211E3` for both); ADI `elfdump` listings of both cores (`python tools/adi_dis.py core2_sram.bin 0x240000 0x24A800`, CCES 2.12.1); `tools/sharc_routines.py` census; `sharc-org` (pass runs) and the new `sharc-fx` (routine calls); [dsp-image-hunt](dsp-image-hunt.md) 5.18 to 5.21, [org-dsp-map](org-dsp-map.md), [dsp-rx-protocol](dsp-rx-protocol.md), [sharc-sequencer](sharc-sequencer.md), [hardware-audio-reference-fx](hardware-audio-reference-fx.md), `crates/nord-core/src/fx.rs`.
- **See also:** [fx-chain-install](fx-chain-install.md) (ARM routine directory, stage rows, Amp type switch).
- **Question:** How is the FX DSP pass organised (cores, buffers, effect chain, parameter and instance layout), which kinds of effect routine does the image contain, how do they get installed, and can one routine be run in the oracle and compared with a reference model?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = read in the ADI listing or reproduced by the command shown; **EMPIRICAL** = textual shape of the code (counts of instruction patterns, not behaviour); **ASSUMED** = reading of the purpose.

## 1. Verdict

| Question | Result | Tag |
|---|---|---|
| Image layout | Two sections (Core1, Core2) with the same entry `0x1211E3`. VISA code at short-word `0x120000..0x124A00` (the same bytes are PM `0x90000..0x92500`), PM data `0x94378..0x94610` (coefficients, instance pointers), DM `0xB0100..0xB6A00`, `0xC0200..0xC2900`, `0xE0C00..0xE3FFF`. 7,438 (Core1) and 6,451 (Core2) decoded instructions ([dsp-image-hunt](dsp-image-hunt.md) 5.19). | VERIFIED |
| Same program in both cores? | One effect library (`0x120528..0x1211DE`, identical bytes except `0x120734`, `0x120933`, `0x120D50`, `0x120D63`) and one pass routine per core with the same shape. Core1 additionally owns the serial-port/DMA set-up (`0x122FEF`, `0x1230CD`, `0x123122`), Core2 does not touch those registers. The effect routines after the pass routine are the same code at Core1 = Core2 + `0x974`. | VERIFIED |
| Pass | Wake by mailbox (`DM(0xC02C2)` Core2, `DM(0xC02C1)` Core1, set by the P4I ISR `0x122FC1`); then NaN-sanitise the 260-word parameter strip, run **35 indirect calls** `CALL (M8, I8)` through an element chain (35 in both cores), seven mixing blocks, a 4-biquad IIR, write 8 output words, loop. | VERIFIED |
| Does the oracle make audio? | **No, same as ORG.** All element records hold the null stub (`nop; RTS` at `0x1249D2` Core2, `0x125346` Core1) and a zero state pointer, so after the first null call the dispatcher continues with `I1 = 0` and `I8 = 0`. Routines are therefore called directly (section 6). | VERIFIED (negative) |
| Which effect is where | Shape classes in section 5. The mapping routine to effect name (Mod 1/2, Amp, Delay, Reverb, Rotary, Compressor) is **not** proven; the names are readings. | EMPIRICAL (shapes), ASSUMED (names) |
| Verified numerically | Routine `0x120FDB` (identical in both cores): three cascaded transposed direct form II biquads over 4 stereo frames. 16/16 output words and 24/24 state words bit-identical to an exact-arithmetic model with 40-bit registers and truncating stores; coefficients in PM are a 2nd-order Butterworth low pass at fs/4. | VERIFIED (section 6) |
| FX EQ coefficients | No Bass/Mid/Treble shelf or peak coefficient table found. 128 + 1 entry float maps exist (section 7) but none fits `EQ_FREQ_HZ`; the EQ stays `ASSUMED` in `fx.rs`. | open |
| SDRAM | Loader zero blocks: Core1 `0x20010000..0x20074000`, Core2 `0x20080000..0x200E4000`, 409,600 B each (102,400 words, 2.13 s at 48 kHz mono). No FX instruction names these addresses; delay pointers come from instance data. | VERIFIED (blocks), ASSUMED (use as delay RAM) |

## 2. Harnesses

```bash
python tools/ns4_dsp_ldr.py dsp_image_0_FX.bin --sram SRAM          # core1_sram.bin, core2_sram.bin
python tools/ns4_dsp_ldr.py dsp_image_0_FX.bin                      # block summary (external windows)
export NS4_CCES='C:\Analog Devices\CrossCore Embedded Studio 2.12.1'
python tools/adi_dis.py SRAM/core2_sram.bin 0x240000 0x24A800 > c2.txt
python tools/sharc_routines.py c2.txt 0x120000 --min 1
# Core2 pass (the first spin at 0x12121B needs Core1's DM(0xC0B00), faked by the device value)
sharc-org SRAM/core2_sram.bin --entry 0x1211E3 --trigger 0xC02C2 --idle-pc 0x12125B \
  --dev 0x28300b00=1 --dev 0x3108d008=5 --passes 1 --max-pass-steps 20000 --coverage
# Core1 pass (P4I ISR starts it; gates from sharc-sequencer)
sharc-org SRAM/core1_sram.bin --entry 0x1211E3 --trigger 0xC02C1 --idle-pc 0x12128B --rx-irq P4I:0x41 \
  --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 --dev 0x28b00b00=1 --dev 0x3108d008=5 --flag 1=1 --boot 3000000
# routine call: pass context up to the pc after MODE1 is set, then call and dump
sharc-fx SRAM/core2_sram.bin --dev 0x28300b00=1 --dev 0x3108d008=5 --trigger 0xC02C2 --stop-at 0x12139b \
  --call 0x120fdb --until 0x1210b0 --set I0=0xb1000 --set I1=0xb2000 --set I9=0x9437a --set M2=2 --set M9=1 \
  --fill 0xb1000=f1.0,f0.5 --dump 0xb200a:8
```

`sharc-fx` (`crates/nord-sharc/src/bin/sharc_fx.rs`, 4 unit tests) is new: boot, optional pass context (`--trigger`/`--stop-at`), register and data presets (`--set`, `--poke`, `--fill`, per call index), `--call PC --until PC` repeated `--calls N` times with state kept, `--dump`. Oracle core files are untouched.

## 3. Pass structure (VERIFIED from the Core2 listing; step counts from `sharc-fx --stop-at`)

| Short-word | Steps after trigger | What |
|---|---|---|
| `0x1211E3..0x12121B` | boot ~740 | `MODE1` clear, call `0x120528` (library init) and clear four areas: `DM(0xC01BC)` and `DM(0xB003C)` 260 words each, `DM(0xC0180)` and `DM(0xB0000)` 60 words each; `DM(0xC02C0) = 1`; **spin at `0x12121B`** until Core1's `DM(0xC0B00)` (alias `0x28300B00`) is non-zero |
| `0x12125B` | 0 | poll `DM(0xC02C2)` (Core2) / `DM(0xC02C1)` (Core1); R0..R2 are saved to `DM(0xB62A3/A5/A7)` and `EMUCLK` deltas are kept as min/max at `DM(0xB6294..0xB629E)`, `DM(0xB6518..0xB651C)` (pass-time statistics for the ARM) |
| `0x1212A0..0x1212DC` | 20 | buffer pair select: `DM(0xC03D3)` compared with `0x28300830`; equal gives input `0xC0180`, parameter strip `0xC01BC`, output `0xA0B00C8`, else `0xB0000`, `0xB003C`, `0xA0C0248` (ping-pong with the DMA of section 2 of [dsp-rx-protocol](dsp-rx-protocol.md)); words stored at `DM(0xC02C4/5)`, `DM(0xC03D4..D7)` |
| `0x1212E4..0x121338` | | wait until `DM(0x282C0504) & 7` equals `DM(0xB0141) - 1` (a counter in Core1's data RAM); first pass only (`& 7 == 0`): rebase three pointer triples at `DM(0xB0868/B0A78/B0C88)` (subtract `0x0A200000`) |
| `0x121394..0x121694` | 94 to 402 | `MODE1 \|= 0xE00000` (PEY on), `ALUSAT` off. Copy parameters from the strip `I3 = DM(0xC02C5)` at offsets `0x68 0x6C 0x70 0x78 0x7A 0x00 0x28 0x50 0x08 0x30 ...` (4 words each) into `0xC03D8..0xC03E8`: `PASS` + `BIT TST ASTATx 0x20` replaces a NaN/Inf by 0, then a running sum with the previous value |
| `0x121696..0x1217A7` | ~402 to 450 | five loops (`LCNTR=1`) over the **PM pointer table `0x945EB..0x945EF`** (blocks `0xC2700 + 15 k`): `x = max(0, a*r4 + b*(1-r4))`, a two-term crossfade written back into the block |
| `0x1217AA..0x122539` | 1249 to the first call | **35 stages**: `gain = clip(r10 + r11, +-f7) + f7`; scratch `0xB62BE` = input x gain; `CALL (M8, I8)` with `I0` = scratch and `I1` = the element's state pointer; afterwards `I1 = DM(I1 - 0x1D)` (next element) and the stage output is mixed into the bus with `r15` |
| `0x12257B..0x122618` | | clear `DM(0xC27CA/CC)`; two-pass mixing (`LCNTR=2`) over the **PM pointers `0x945F0..0x945F1`** (blocks `0xC2812`, `0xC2821`) into `DM(0xC27C2..0xC27EA)` |
| `0x122623..0x122720` | | **master IIR**: `LCNTR=2` x `LCNTR=4` TDF II biquads, then `LCNTR=3` more; coefficients PM `0x945F2..0x94605` (4 sets of 5), input `0xC27D2`/`0xC27DA`, states `0xC2810`/`0xC2820` |
| `0x122735..0x1227B1` | | add the master buffers and store 8 output words through `I2 = DM(0xC03D7)`; time stamp (`DM(0xB6518..651C)`); `BIT CLR MODE1 0x202000`; `JUMP 0x12122D` (back to the poll) |

Block size: every routine unrolls **4 stereo frames** (8 words: `dm(i, m2)` with `M2 = 2`, PEx left, PEy right). The 60-word input is `7 * 8 + 4` words (ASSUMED reading: seven layer stereo pairs plus a 4-word extra), the strip offsets read are listed above (largest `0x7A`, so the 260-word area is only partly read by this section). Neither size is proven by a DMA decode.

Core1 differs only in the prologue: it waits for Core2's flag `DM(0x28B00B00)` at `0x121237`, sets `FLAGS` bit 0, waits on `FLAGS` bit 2 (`0x121241`), runs the DMA set-up calls, then polls `DM(0xC02C1)`; the pass body (35 calls, `0x945EB` blocks, master IIR) has the same shape (the Core1 pass routine is 784 instructions longer; PM coefficients identical, pointer table `0xC27A0 ...`). The ISR `0x122FC1` stores the SEC source id to `DM(0xC02C1)`, clears `DM(0x31022330)` and acknowledges via `DM(0x3108900C)`.

Oracle runs (strict, 0 skipped operations): both cores reach their poll loops, start the pass, execute the first `CALL (M8,I8)` (target the null stub), continue with `I1 = 0` (the record's state pointer is 0 and the stub does not touch `I1`; `DM(0xFFFFFFE3)` reads as 0 from the device store), and call `I8 = 0`; the run then falls into the zero-filled vector table at `0x120000`. `sharc-org ... --coverage` prints `calls (target<return): 1249d2<121803 0<121869` (Core2) and `125346<121895 0<1218fb` (Core1).

## 4. Instance records and what they say (VERIFIED fields, ASSUMED meaning)

Dispatcher code `0x1217CB..0x1217FB`, relative to the instance pointer `I1`:

| Offset | Use in the dispatcher | Image default |
|---|---|---|
| `-0x20` | `I8`: effect routine | null stub `0x1249D2` |
| `-0x1F` | `I0`: input block pointer | `0xB62AA` |
| `-0x1E` | `r12`, becomes the callee's `I1` (its private state block) | `0` |
| `-0x1B`, `-0x1C` | two level terms (r10, r11) | `-0.5`, `-0.01` |
| `-0x19` | clip limit f7 | `0.5` |
| `-0x18` | mix coefficient read after the call | |
| callee `I1 - 0x1D` | **next element**, read after the call with the callee's `I1` (`I1 = DM(I1 - 0x1D)` at `0x12182D`), so every state block carries the link to the next element | the null chain never reaches it |

The image holds about 36 such 8-word records at `DM(0xB62CE)` and `0xB63EE ... 0xB6510`, separated by two-word headers `{0xB62EE, 5}` and `{0xB62EE, 2}` in the sequence 5, 2, 5, 2, ... (scan: 53 null-stub words in total, so the exact record count is not settled). `gain = clip(-0.5 - 0.01, +-0.5) + 0.5 = 0`: every stage is muted until the ARM writes levels. 7 layers x 5 effects = 35 matches the stage count, but the groups of 2 are unexplained (ASSUMED reading: per layer Mod 1, Mod 2, Amp/EQ, Delay, Reverb).

How routines are installed: no word in `DM 0xB0000..0xE3FFF` points to an FX routine other than the null stubs (scan of `0x90000..0xFFFFF`: 53 words with the Core2 stub, 5 words `0x120D12` at `DM(0xC1DA8..AC)` (Core1 `0xC1E48..4C`), the `RTS` of a second 5-slot table, not used by the pass routine). Therefore the ARM writes the function pointers (and levels, state pointers) into the records: the DSP-side receive path is the DMA write port of [dsp-rx-protocol](dsp-rx-protocol.md). Which addresses it writes is open, as is whether the ARM sends routine addresses or an index (the library is part of the DSP image, so addresses suffice). ASSUMED.

## 5. Routine classes by instruction shape (Core2 addresses, Core1 + `0x974` after the pass routine)

Counts from `tools/sharc_routines.py` and a keyword census; "mul" counts multiplies. EMPIRICAL for the shape; the effect name column is ASSUMED.

| Routine | Instr | Shape | Reading |
|---|---|---|---|
| `0x120630` | 54 | `ABS`+`MAX` over 4 lanes, `LOGB`/`MANT` log2 estimate, `COMP` threshold, `FIX` and a two-point fixed-point interpolation (`R = R*R (SSF)`), gain applied to 4 frames | compressor gain computer |
| `0x1206B0`, `0x1208AF`, `0x120AB5` | 237, 235, 287 | `RECIPS`+Newton (division), `FPACK/FUNPACK` 16-bit packed floats, `FEXT/FDEP`, smoothing 0.9908 (`0x3F7DA3C2`, the ORG smoothing constant), pass pointers `DM(0xC03D4/5)`, three variants | tap reader / smoothed delay with 16-bit float storage |
| `0x120D8A` | 240 | packed 16-bit float circular buffer (`B/L/I`, 10 `FPACK`, 12 `FUNPACK`), fractional index from `TRUNC`/`FLOAT`, `BTGL`, 21 PM fetches | modulated allpass/diffuser or delay with fractional read |
| `0x120FDB`, `0x12417E`, `0x1247CE` | 201, 196, 216 | three 4-frame biquads (TDF II) with PM coefficients `0x9437A`, then the same with `0x9437F`; `0x120FDB` calls `0x120D8A` in the middle | **fs/4 low-pass bank around a delay stage** (resampling around a modulated delay; section 6) |
| `0x1228CF` | 147 | integer index `FEXT`/`FDEP`, `FIX`, linear interpolation in the `-cos` table (`DM 0xC0448`), smoothing 0.9908 | LFO oscillator (table) |
| `0x122A1D`, `0x122B1D` | 104, 111 | circular `B/L/I` write, FIR with `R = DM(..) , R = PM(I9,M9)` coefficient pairs, then a loop of 2nd-order sections whose **state lives in PM** (`PM(I8, M11) = R`) | decimating/interpolating FIR + IIR (oversampling filters) |
| `0x122C39` | 99 | integer phase accumulator `R0*R7 (UUI)`, `2|x|-1` triangle, odd polynomial `x, x^3, x^5` (sine), two lattice allpass pairs per channel, wet/dry `f0 - g f0 + g f8` | LFO-modulated allpass (phaser/vibe type) |
| `0x122D2C..0x123556` (9 routines, 50 to 252 instr) | | `ABS`, `TRUNC`+`FLOAT` fraction, linear tap interpolation, `CLIP` | LFO-modulated delay (chorus/flanger/delay time) |
| `0x12314A`, `0x12329B` | 155, 207 | 8 and 16 `FPACK/FUNPACK` pairs, `AND/OR` field merges | multi-tap packed delay memory (reverb lines) |
| `0x12379B` | 160 | table-interpolated LFO, **three `RECIPS` Newton divisions** for coefficients, `LCNTR = 2 n` loop of first-order allpass sections with a feedback term | phaser |
| `0x1243F4`, `0x124497`, `0x1245FB`, `0x124637`, `0x124696` | 61, 89, 25, 31, 112 | 4 frames unrolled, `CLIP` inside the feedback path, `(a+b)/2`, no loop | nonlinear resonant filter (state-variable/ladder): amp LP24/HP24 and EQ filter resonance of `fx.rs` |
| `0x123BEE..0x123F60` | 63 to 129 | `CLIP`, `FLOAT`, `LSHIFT` | waveshapers/limiters |
| `0x122623` (inline in the pass) | | `LCNTR=4` TDF II biquads, PM coefficients | master IIR (4 sections, palindromic numerators) |

Delay lines with feedback, allpass chains, LFO-modulated delays, envelope detector, waveshaper/clip and interpolation tables are all present as classes; there is no standalone Rotary loudspeaker, tape/analog delay or spring reverb signature to name. The Delay "Chorus/Ensemble/Vibe" variations from the manual correspond to the modulated-delay and allpass classes.

## 6. Routine check: `0x120FDB` against a reference biquad (VERIFIED)

```bash
cargo build -p nord-sharc --release
python tools/fx_biquad_check.py SRAM/core2_sram.bin
# coefficients b0 b1 b2 a1 a2 = 0.292893201 0.585786402 0.292892992 0 0.171572998
# output words bit-identical 16/16; state words bit-identical 24/24, worst 0 ulp
```

Setup: pass context to `0x12139B` (`MODE1` has PEY on), `I0 = 0xB1000` (4 stereo frames, left at even words), `I1 = 0xB2000`, `I9 = 0x9437A`, `M0 = 0 M2 = 2 M9 = 1`; run to `0x1210B0` (just before the nested `CALL 0x120D8A`, 87 instructions). The three stages are chained in place at `I1 + 0xA` (8 words), states `s1L s1R s2L s2R` at `I1 + 0x12 + 4k`. Two calls with different inputs and non-zero initial states; the second call starts from the states the first one stored.

| Item | Result |
|---|---|
| Difference equation | `y = b0 x + s1; s1 = (b1 x + s2) - a1 y; s2 = b2 x - a2 y` (TDF II, same as `Biquad` in `fx.rs`) |
| Coefficients (PM `0x9437A..7E`, `0x9437F..83`) | `0x3E95F619 0x3F15F619 0x3E95F612 0 0x3E2FB0D5` = 0.292893, 0.585786, 0.292893, 0, 0.171573: Butterworth low pass at **fs/4** (`b0 = (2 - sqrt 2)/2`, `a2 = (2 - sqrt 2)/(2 + sqrt 2)`, error below 1e-6); unity at DC, zero at Nyquist, -3 dB at fs/4 |
| Arithmetic | exact rationals rounded to a 32-bit mantissa (40-bit register, round to nearest even) per operation; a store drops the 8 extension bits by **truncation toward zero**. Models with float32 or double rounding at every step and with round-to-nearest stores matched 13 to 29 of the 40 words; the register/truncation model matches all 40. This also fits `float.rs`. |
| Tests | `tools/tests/test_fx_biquad_check.py` (8 tests, coefficients, tie rounding, state carry, parser); `sharc_fx.rs` (4 tests) |

Core1 contains the same bytes at `0x120FDB..0x1211C1` (VERIFIED by byte comparison); the Core1 pass context was not set up for this check (its gates differ), so only Core2 was run.

Consequence for `fx.rs`: the filter form (TDF II) is VERIFIED; these coefficients are fixed anti-alias/resampling filters, not the EQ, so the EQ parameters stay ASSUMED.

## 7. Tables and PM data (VERIFIED contents, ASSUMED meaning)

`python tools/fx_dm_tables.py SRAM/core2_sram.bin --min 100` (tests: `tools/tests/test_fx_dm_tables.py`):

| DM address | Length | Values | Reading |
|---|---|---|---|
| `0xC0448`, `0xC0649` | 512, 404 (+) | `-cos` quarter wave `-1 .. -0.0031`, then sine start `0.0031 ..` | LFO / phase tables, interpolated by `0x1228CF` |
| `0xC1DAD`, `0xC1FAE` (module area) | 512, 512 | `-cos` / sine quarter waves again | second copy |
| `0xC0849` | 128 | rising `0.003665 .. 0.697956` (second value `0.0038137`) | control map, 7-bit index |
| `0xC08CA` | 128 | falling `0.126 .. 0.0033` | control map |
| `0xC094C`, `0xC09CB` | 127/129 | rising `0.00278 .. 1.0`, falling `1.0 .. 0.002` | control maps |
| `0xC0A4C`, `0xC0ACD`, `0xC0B4E` | 129 each | `0.002 .. 44.67`, `44.67 .. 0.0527`, `0.0527 .. 4.032` | control maps (rate/time like ranges) |
| `0xE3D81`, `0xE3E02` | 129 | `0.000758 .. 1.1859`, `0.000379 .. 0.4648` | control maps |
| PM `0x9437A`, `0x9437F` | 2 x 5 | Butterworth fs/4 (section 6) | fixed filters |
| PM `0x945F2..0x94605` | 4 x 5 | `0x3F0570C0 0x3E8659B2 0x3F0570C0 0xBE3FF6D3 0x3F414CF5` and three more sets (palindromic `b0, b1, b2`) | master IIR |
| PM `0x945EB..0x945F1` | 7 | pointers to instance blocks `0xC2700 + 15 k` (5), `0xC2812`, `0xC2821` (Core1: `0xC27A0 ...`, `0xC28B2`, `0xC28C1`) | mixing sections |

EQ test: the first map (`0xC0849`, 128 values) was compared with `EQ_FREQ_HZ` (the OLED frequencies in `fx.rs`) under nine closed forms (`tan`, `sin`, `sin^2`, `tan^2`, `1 - cos`, ... of `pi f / fs`) with a free scale and `fs` from 9 to 200 kHz: best residual 0.33 in log (no fit). So the maps are **not** a simple function of the displayed Hz; the 7-bit control to coefficient mapping is not recovered, and `EQ_FREQ_HZ` remains `EMPIRICAL` (display values).

## 8. Cross-reference with `fx.rs`

| `fx.rs` item | DSP evidence |
|---|---|
| `Biquad` TDF II, RBJ designs | TDF II form VERIFIED (section 6); coefficient design is not the RBJ cookbook in the routines inspected (fixed tables or `RECIPS`-based design in the phaser). Bass/Treble shelf corners (100/4000 Hz) and `Q` remain `ASSUMED`. |
| Amp `LP24/HP24`, filter resonance | `CLIP` in the feedback of the 4-frame filter routines (section 5) supports a saturating resonant filter; not matched numerically. |
| Delay chorus/ensemble | packed 16-bit float delay buffers with fractional taps (`0x120D8A`, `0x12314A`, `0x12329B`). The hardware stores delay samples as 16-bit floats (`FPACK`), which `fx.rs` does not model. |
| Reverb FDN | 8 and 16 packed taps per routine (`0x12314A`, `0x12329B`) fits a multi-line reverb; line lengths not decoded. |
| Block size | all routines are 4 frames; `fx.rs` is sample-wise (no impact). |
| Master | a 4-section IIR on the summed bus (`0x945F2`), not in `fx.rs`. |

## Implications

- FX replication needs the data side: the instance records, the 7-bit map tables and the ARM messages. The DSP code is a library of ~30 routines plus the generic 35-stage pass.
- Truncating stores and 40-bit registers matter for bit-exactness (section 6); `fx.rs` uses float32 everywhere, which is fine for listening tests.
- 16-bit packed delay memory is a distinctive feature to model if delay noise floor matters.

## Follow-ups

1. Install a routine in a record in the oracle (`--poke` on `0xB62CE..`) and run the full pass: `sharc-org` needs a stage-install option; then compare `0x12379B` (phaser) and the compressor `0x120630` with reference models.
2. Recover the strip offsets (`0x68`, `0x6C`, ...) from the ARM senders ([dsp-rx-protocol](dsp-rx-protocol.md) follow-up 3) to name parameters.
3. Decode the 128-entry maps against the OLED displays (rates, times) to settle `EQ_FREQ_HZ`.
4. Find what the groups of 2 instances (headers `{0xB62EE, 2}`) are.
5. Check the Core1 copy of `0x120FDB` with the P4I context.

# Research: the PNO Core1 post stage `0x12272D` and the output stage `0x1229B3`

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the PNO image `SRAM/core1_sram.bin` (OS v1.66, `tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`), ADI `elfdump` listings of both routines (`tools/adi_dis.py`, pc `0x12272D..0x1229B2` and `0x1229B3..0x122A70`), the SHARC core model `crates/nord-sharc` with the new harness `sharc-chain` (`crates/nord-sharc/src/bin/sharc_chain.rs`), the Python models `tools/organ_post_ref.py` and `tools/sharc40.py`, the driver `tools/organ_post_oracle.py`, ARM edit capture `data/arm-edit-capture-summary.csv`. Builds on [organ-oracle-audio](organ-oracle-audio.md), [organ-tonewheel-bank](organ-tonewheel-bank.md), [organ-tonegen](organ-tonegen.md), [organ-tone-source](organ-tone-source.md), [pno-module-chain](pno-module-chain.md), [pno-dsp-map](pno-dsp-map.md).
- **Question:** What does the PNO Core1 post stage `0x12272D` compute (is it the 19-tap triangle-LFO scanner / delay-tap stage hypothesised in [organ-tonegen](organ-tonegen.md)), which buffers does it read and write, and where does the final audio of the chain come out?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command in this note, **EMPIRICAL** = structure or fit read from listings or pokes, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is `0x12272D` a vibrato scanner (19-tap delay line)? | **No.** It reads no audio. It is a **bank of 84 + 36 oscillators with a pitch LFO**: a triangle LFO (`phase += 0x905A3` per frame, 6.6 Hz at 48 kHz) plus a fine-tune word produce a pitch factor `m`; 84 note oscillators (rate ladder `0x62C2E7 * 2^(k/12)`) fold their phase into a triangle, clip it with a per-note steepness (`128.8 * 2^(-k/12)`), look it up in a 1,025-point waveshaper table, run it through a note-tracking one-pole low-pass, and 36 polynomial-sine oscillators are summed into 12 buckets. The "19" was the loop count of a software-pipelined loop over the 84 oscillators (`LCNTR = 0x13`, 2 pairs per iteration). | VERIFIED (bit-exact model) |
| Algorithm exact? | The Python model reproduces **every word** the routine writes (LFO, `mod`, 84 phase states, both 84-word scanner buffers, both filtered buffers, 36 sine phases, both 12-word bucket sets) on random scenarios: 8 + 8 seeds, 0 differing words (`python tools/organ_post_oracle.py verify ... --seeds 8`). | VERIFIED |
| What does it output? | Not an audio sample. It fills DM `0xE2110..0xE21D0` (two blocks of 96 words: 84 filtered note signals + 12 sine buckets, one block per frame). The audio comes out of the **next routine `0x1229B3`**. | VERIFIED |
| Where does the final audio come out? | `0x1229B3` (**output stage**): a 96-word weighted sum (weights from a record the ARM builds) of the post-stage block, three 2-tap sections, two smoothed gains, a fixed `0.090075` and a gain word; one `DM(I0, M2) = R0` per frame (two words, lanes x and y) at pc `0x122A68` (byte `0x2454D0`). Bit-exact model, 8 random scenarios, 0 differences. | VERIFIED |
| Is the post stage fed by the tonewheel bank/mix stage? | **No.** With bank and mix removed from the pass the final lanes are bit-identical; with the post stage removed the mix lanes are bit-identical (section 5). The two paths are independent generators: bank + mix (92 tonewheel-like wheels, [organ-oracle-audio](organ-oracle-audio.md)) and post stage + output stage (84 note oscillators). Their outputs only meet if the ARM points them at the same `I0`; `0x1229B3` overwrites (does not accumulate). | VERIFIED (dependency test), wiring EMPIRICAL |
| What do the words `0xE20E0`, `0xE20E3` do? | `DM(0xE20E3)` is the **vibrato depth** as a pitch fraction (`mod = depth * triangle`), `DM(0xE20E0)` the **fine-tune offset** (`mod += 2 (word - 0.5)`). The ARM Vox vibrato values `0xE464<<8, 0x1782D<<8, 0x240B7<<8` are 0.70 %, 1.15 %, 1.76 % peak pitch deviation (12, 20, 30 cent). | VERIFIED (poke + sidebands), mapping to ids EMPIRICAL |
| Vibrato spectrum | Sidebands at carrier +- LFO frequency: with the Vox depths the first sideband is **0.16 / 0.21 / 0.25** of the carrier (C4 / E4 / G4, depth `0xE46400`) and **0.45 / 0.60 / 0.77** (depth `0x240B700`); without depth below **0.0005** (section 5). | VERIFIED |
| Still unknown | The ARM code that builds the `0x1229B3` record (weights, section coefficients, gains) and the base word `DM(0xE2BE0)` of the sine bank; the audio frame rate (absolute pitch of both banks); the role of the 36 sines (0.1 to 0.45 cycles per frame at the image default of `DM(0xE2BE0)`, above 4 kHz at 48 kHz); which B3 tone path, if any, uses this stage. | open |

## 2. Method

```text
pokes (synthetic scenario) --> sharc-chain --step 0x12272D:0x1229B2 --range 0xE0000:0x3000 ... --> DM words
                                  \--> organ_post_ref.py (Fraction arithmetic, SHARC 40-bit floats) --> compare word by word
```

| Piece | File | Role |
|---|---|---|
| Harness | `sharc_chain.rs` (`sharc-chain`) | Boots the PNO section, then per call runs any list of `--step ENTRY:RET[:REG=V,..]` routines with register presets, scripted pokes, `--range`/`--dump` capture and `--trace LO:HI FILE` (all registers after each instruction). Generalises `sharc-organ`. |
| Arithmetic | `sharc40.py` | Exact rationals rounded to a 32-bit significand after every float operation (`MODE1 RND32 = 0`, the firmware state), denormals flushed, stores keep the top 32 bits (truncation), `SSF`/`SSFR` fractional multiplies, `FIX` round to nearest even. |
| Models | `organ_post_ref.py` | `post_stage`, `output_stage`, `synthetic_inputs(seed)`, `output_inputs(seed)`. |
| Driver | `organ_post_oracle.py` | `verify` (random scenarios against the oracle), `render` (whole pass: bank, mix, post stage, output stage to WAV and `.npy`), `report` (carrier and sideband levels). |

Three facts about the oracle that mattered (all VERIFIED by the bit-exact comparison):

1. The firmware runs with `RND32 = 0`: float results stay in the 40-bit format between operations (32-bit significand) and a store to DM truncates to 24 bits. A model that rounds to float32 after every operation is wrong by one ulp in the low-pass outputs.
2. SIMD pairs: a direct or offset load gives PEy the word at `addr + 1`, **except** loads through `I1` and `I9` (`BDCST1`/`BDCST9`): there PEy receives the same word as PEx. The base word of the sine bank (`DM(I1)`, `dm(m0,i1)`) is therefore shared by both lanes, while the LFO phase `DM(0xE21F0 + lane)` and the constants read through `I5` are per lane.
3. The 36 sine increments of the first four oscillators (prologue pairs 0 and 1) use the unrounded `SSF` product, the other 32 the rounded `SSFR` (software-pipeline peeling).

## 3. Post stage `0x12272D`

pc = `0x12272D` + listing offset; `RTS` at `0x1229B2` (byte `0x245364`), 1,833 steps per call. One call = **two frames** (`LCNTR = 2` loop, ping-pong buffers), then the sine bank (`LCNTR = 2` again, one bucket set per frame).

| Offsets | pc | Section | Computes |
|---|---|---|---|
| `0x00..0x0E` | `0x12272D` | setup | saves `I0` to `DM(0xE210E)`; `I9, I13, I14, I15 = 0xE21F2, 0xE2246, 0xE2170, 0xE2110` |
| `0x0F..0x5B` | `0x12273C` | **LFO** | `phase += 0x905A3`, triangle, depth, fine tune, `m` |
| `0x5C..0x125` | `0x122789` | **note oscillators** | 84 phases, fold, clip, table lookup, 2 pairs per iteration, `LCNTR = 0x13` |
| `0x126..0x17B` | `0x122853` | asymmetry | notes 12..47 only, `LCNTR = 8` |
| `0x17C..0x1C1` | `0x1228A9` | **low-pass** | `Y = A cA + A' cB + Y' cC`, 4 notes per 12 coefficients, `LCNTR = 0x14` |
| `0x1C2..0x1CB` | `0x1228EF` | buffer swap | `I9<->I13`, `I14<->I15` (end of the frame loop) |
| `0x1CE..0x285` | `0x1228FB` | **sine bank** | 36 polynomial sines, 12 buckets per frame, `LCNTR = 2` x `LCNTR = 9` |

### 3.1 LFO and pitch factor (per lane `l`, PEx = element 2p, PEy = 2p + 1)

```text
phase_l  += 0x905A3                     (DM(0xE21F0 + l), wraps; 2^32 / 0x905A3 = 7,430 frames per cycle)
tri       = clip(4 (|phase_l 2^-31| - 0.5), 1)               (triangle -1..1)
tri'      = clip(tri - 0.08, 0.91) + 0.08                     (-0.83 .. 0.99)
mod_l     = ssf(fix(tri' 2^31), DM(0xE20E3)) + 2 (DM(0xE20E0) - 0x40000000)     -> DM(0xE20E4 + l)
m_l       = 0x40000000 + ssf(0x40000000, mod_l)                (0.5 + mod / 2)
```

`DM(0xE20E3)` and `DM(0xE20E0)` are broadcast to both lanes by the `Rx<->Sy` swap idiom (partner words `0xE20E4`, `0xE20E1` ignored). Poke test: `DM(0xE20E0) = 0x50000000` gives `mod = 0x20000000` (+25 %), `DM(0xE20E3) = 0x08000000` gives `0x07EB8522`. The LFO rate in Hz is `0x905A3 / 2^32 * fs`: 6.63 Hz at 48 kHz, 5.96 Hz at 43,156 Hz (both inside the 5 to 7 Hz of a vibrato).

### 3.2 Note oscillators (84 elements `k`, frame buffer `OUT`)

```text
ph_k     += ssfr(m_(k mod 2), rate_k)               state DM(0xE2B14 + k), rate_k = DM(0xE22B2 + k)
x         = clip((|ph_k 2^-31| - 0.5) * coef_k, 1)    coef_k = float DM(0x95CC6 + k)
OUT[k]    = T[fix(x * 2^9)]                           T = DM(0xE2512 + i), i = -512..512
k in 12..47:  OUT[k] *= (OUT[k] > 0 ? 1 : 0.2 ph_k 2^-31 + 0.9)
```

Data, read from the image (`tools/organ_post_oracle.py` snapshot, script in section 7): `rate_k = 0x62C2E7 * 2^(k/12)` (max relative error 1.8e-6 over 84 entries), `coef_k = 128.8 * 2^(-k/12)` (128.8 at k = 0, 4.025 at k = 60, 1.066 at k = 83: steeper clipping, i.e. squarer waves, for low notes), `T(-512) = -1`, `T(512) = 1`, `T(0) = -0.0053`, overshoot `T(128) = 1.041` (an edge shaper, not a tanh: best-fit `tanh(2x)` leaves 0.25 rms). Cycles per frame of note `k` at `mod = 0` (`m = 1/2`): `rate_k / 4 / 2^31 = 7.56e-4 * 2^(k/12)`, 36.3 Hz to 4.4 kHz at 48 kHz (D1 minus 17 cent for k = 0; the same number as wheel 0 of the tonewheel bank, so the absolute pitch is tied to the frame rate (ASSUMED: 43,156 Hz makes k = 0 equal to C1 minus 10 cent)). The asymmetry stage multiplies the negative half of notes 12..47 by `0.9 + 0.2 ph`, the phase of the oscillator (VERIFIED, purpose unknown).

### 3.3 Note-tracking low-pass (per frame `f`, `A = OUT_f`, `A' = OUT_(f-1)`, `Y' = Y_(f-1)`)

```text
Y_f[k] = RND(RND(A[k] cA_k) + RND(A'[k] cB_k)) + RND(Y'[k] cC_k)         (40-bit rounding, stored truncated)
coefficients at DM(0x95D26 + 12 g + {0..3: cB, 4..7: cA, 8..11: cC} + j)    for k = 4 g + j
```

Frame 0 reads the other scanner buffer (`0xE2246`, last call's frame 1) and `Y1` (`0xE2170`); frame 1 the swapped pair, so the filter state is the buffers themselves. Image values: `cA = cB = c_k` from 0.0045 (k = 0) to 0.304 (k = 83) and `cC = 1 - 2 c_k` (DC gain 1.0000): a **bilinear one-pole low-pass** with cutoff `atan(c/(1-c)) / pi * fs` = 1.9 f0 (k = 0), 1.05 f0 (k = 36), 1.44 f0 (k = 83). Harmonic measurements (section 5) agree: odd harmonics 3, 5, 7, 9 of a single note at -13.9, -21.9, -27.4, -31.7 dB.

### 3.4 Sine bank (per frame, 36 oscillators `j`, buckets 12 words)

```text
base_l    = DM(0xE2BE0 + l) + ssf(DM(0xE2BE0 + l), mod_l) -> DM(0xE2CB2 + l);   both lanes use lane 0 (broadcast)
ph_j     += base * DM(0xE2C8E + j)                      (SSF for j < 4, SSFR otherwise), state DM(0xE2B98 + j)
u         = |ph_j 2^-30| - 1;   p = ((c7 u^2 + c5) u^2 + c3) u^2 + c1;   y_j = (u g_j) p
bucket[BI(j>>1) + (j&1)] += y_j        BI = 0,2,4,6,8,10,0,2,4,6,8,10,0,2,4,6,0,2 (DM 0x95E22, one word per pair)
```

`p` is the sine polynomial of `x pi/2` with `c1 = pi/2 = 0x3FC90FAC`, `c3 = -0.645891`, `c5 = 0.0794309`, `c7 = -0.0043311` (DM `0xE2CBE, 0xE2CBC, 0xE2CBA, 0xE2CB8`, per lane), `u` offset `-1.0` (`0xE2CB6`), float scale `-30` (`0xE2CB4`); `y_j = sin(pi u / 2) g_j` is a sine of the phase (one cycle per wrap of `ph`). Gains `g_j` (`DM(0xE2BBC + j)`): 1.0 for j < 12, 0.1718 for 12..23, 0.166 for 24..35. Ladder (`DM(0xE2C8E + j)`): `0x1AE89F99 * 2^(j/12)` for j < 24 exactly, then x0.75 for j = 24..31 and x0.63 for j = 32..35 (fixed-point headroom: the entries are folded down). Frame 0 sums into `0xE2164..0xE216F`, frame 1 into `0xE21C4..0xE21CF` (0x60 words further, the bucket pointer `I4` is set to `0xE21C4` at the end of the first pass). With the image defaults (`DM(0xE2BE0) = 0x7572967D`, 0.917) the sines run at 0.096 (j = 0) to 0.45 cycles per frame: above 4 kHz at 48 kHz, so either `DM(0xE2BE0)` is overwritten at run time (the ARM edit capture never writes it) or these are not pitch carriers (open).

### 3.5 Buffers

| DM | Words | Content | Written by |
|---|---|---|---|
| `0xE20E0`, `0xE20E3` | 1 + 1 | fine-tune offset, vibrato depth | ARM (Fine Tune handler, organ vibrato type) |
| `0xE20E4` | 2 | `mod` of the last frame | post stage |
| `0xE21F0` | 2 | LFO phase (lane x, lane y) | post stage |
| `0xE2B14` | 84 | note phases | post stage |
| `0xE21F2`, `0xE2246` | 84 + 84 | shaped notes, frame 0 / frame 1 (state) | post stage |
| `0xE2110`, `0xE2164` | 84 + 12 | Y frame 0, bucket set 0 (the first 96-word block) | post stage |
| `0xE2170`, `0xE21C4` | 84 + 12 | Y frame 1, bucket set 1 (the second block) | post stage |
| `0xE2B98` | 36 | sine phases | post stage |
| `0xE22B2`, `0x95CC6`, `0x95D26`, `0xE2512` | 84, 84, 252, 1,025 | rates, steepness, low-pass coefficients, waveshaper table | image data |
| `0xE2BE0`, `0xE2C8E`, `0xE2BBC`, `0xE2CB4..0xE2CBF`, `0x95E22` | 2, 36, 36, 12, 36 | sine base, ladder, gains, constants, bucket pairs | image data |
| `0xB33C0` etc. | | **not a ring**: every PNO routine saves/restores `I0` there (`dm(0xb33c0)=i0`), as the post stage does at `0xE210E` | correction of [organ-tonegen](organ-tonegen.md) |

## 4. Output stage `0x1229B3`

`RTS` at `0x122A70`, 254 steps per call, called with `I0` = output pointer and `I1` = record (the slot call convention of [pno-dsp-map](pno-dsp-map.md) s6). Record fields: `DM(I1 - 0x1F)` = weight pointer (PM, 96 words), `DM(I1 - 0x1E)` = parameter block `G`.

```text
per frame f = 0, 1 (src = 0xE2110, then 0xE2170; weights restart):
  lane sums  S_l = ( sum_(even pairs n) d[2n+l] w[2n+l] ) + ( sum_(odd pairs n) d[2n+l] w[2n+l] ),   n = 0..47   (two accumulators)
  T          = S_y + S_x                                   (the two lanes combine into one value)
  section i  (state block P_i, 8 words): out = (T b0 + P[1] a0) + (T b1 + P[2] a1),   b0 = P[6], b1 = P[7], a0 = P[4], a1 = P[5]
             P[2] = P[3] = T,  P[0] = P[1] = out          chain: P_1 = DM(G+8), P_2 = DM(G+2), P_3 = DM(G+3)
  gains      g1 += 0.001 (DM(G+0) - g1) [state G+4/5],  g2 += 0.001 (DM(G+1) - g2) [state G+6/7]
  output     y = (out_3 g1 + out_1 g2) * 0.090075 * DM(G+0xB);  DM(I0) = DM(I0+1) = y;  I0 += 2
```

The 96 data words are the 84 filtered notes followed by the 12 sine buckets, so a weight vector is "note `k` level (key x drawbar)" for `k < 84` and a bucket level for `k = 84..95`. Both output lanes carry the same value unless the lane constants differ (the PEy half of each coefficient pair); the image ships all pairs duplicated. The ARM install of [pno-module-chain](pno-module-chain.md) s3 writes `DM(0x95FC6)` = 1.0 and `DM(0x95FC7)` = 0, which read as the gain targets `DM(G+0)` and `DM(G+1)` if `G = 0x95FC6` (ASSUMED), so that at steady state `y = out_3 * 0.090075 * gain` and the first section's tap is bypassed. The coefficients of the three sections and the weights are ARM-written and **not known**; `organ_post_oracle.py` uses identity sections (`b0 = 1`) and weight 1 on the note oscillators of a chord.

## 5. The chain: bank, mix, post stage, output stage

`python tools/organ_post_oracle.py render ... --notes 60,64,67` runs per call `0x122620` (bank), `0x12268F` (mix, set 0, `I0 = 0xB0220`), `0x12272D` (post), `0x1229B3` (`I0 = 0xB0200`), 32,768 calls = 65,536 frames in 27 s. Note oscillator = `note - 24` (ASSUMED, as for wheel 0).

| Check | Result |
|---|---|
| Final lanes with bank + mix removed from the pass | bit-identical (2,000 calls) |
| Mix lanes with the post stage removed | bit-identical; the final lanes are all zero without the post stage (the output stage reads stale zeros) |
| Mix lanes | peak 8.903, same as the standalone [organ-oracle-audio](organ-oracle-audio.md) chord |
| Final lanes, chord C4 E4 G4, weights 1, no vibrato | peak 2.783, rms 1.108; carriers 0.878 / 0.933 / 0.863 (lowpass and table shape), sidebands 0.0005 relative |

Vibrato (frame rate 48 kHz is only the unit for the frequency column; `--frames 65536`, Hann window, 61,440 frames analysed):

| Depth word | Peak deviation | Note | Carrier | -1st sideband | +1st sideband | Small-index estimate `J1/J0` |
|---|---:|---|---:|---:|---:|---:|
| `0xE46400` (Vox 1) | 0.70 % | C4 / E4 / G4 | 0.855 / 0.895 / 0.813 | 0.166 / 0.211 / 0.253 | 0.161 / 0.205 / 0.246 | 0.15 / 0.19 / 0.23 |
| `0x240B700` (Vox 3) | 1.76 % | C4 / E4 / G4 | 0.737 / 0.702 / 0.570 | 0.454 / 0.605 / 0.774 | 0.436 / 0.581 / 0.739 | beyond the small-index range |

(sideband = amplitude at carrier -+ LFO frequency `0.000138` cycles/frame relative to the carrier; the estimate is `beta/2` with `beta = deviation * f_note / f_LFO` for a sinusoidal LFO, so the triangle LFO gives slightly stronger sidebands). The spectrum is what a **pitch** vibrato produces; there is no delay line in this stage. Single-note harmonics (weight 1 on one oscillator, no vibrato, harmonic levels relative to the fundamental, `h = 2..9`):

| MIDI | cycles/frame | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 (dB) |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 24 | 0.000753 | -67.1 | -13.9 | -80.8 | -21.9 | -87.7 | -27.4 | -92.0 | -31.7 |
| 36 | 0.001507 | -33.8 | -13.9 | -43.9 | -21.9 | -50.5 | -27.4 | -55.3 | -31.7 |
| 60 | 0.006028 | -35.4 | -16.4 | -46.7 | -24.9 | -53.6 | -30.7 | -58.5 | -35.1 |
| 96 | 0.048223 | -60.0 | -16.4 | -67.8 | -28.6 | -78.8 | -44.2 | -97.3 | -70.0 |
| 107 | 0.091033 | -56.4 | -21.6 | -79.3 | -70.7 | -113.9 | -66.5 | -100.8 | -60.8 |

Odd harmonics from the steep clip; the even harmonics at -34 to -35 dB in the octaves of notes 12..47 (MIDI 36..71) are the asymmetry stage (absent at MIDI 24 and above 72).

Where the pieces meet in a pass is still the ARM's decision (slot table `DM(0xC01D6)`, [pno-module-chain](pno-module-chain.md) s3): state 1 kind 4 installs the post stage (`event 1`), `0x122A71` and the bank (`events 3, 4`); state 2 installs `0x1229B3` (`event 1`) and the mix stage (`events 3, 4`). A program of kind 4 therefore holds both generators, the output stage and the mix stage; whether the two outputs are summed, selected by organ model, or used one per layer is not determined by the code (open). The rest of the call table (`0x121E2B`, `0x122296`, `0x122B85`, `0x122A71`) reads none of the buffers above (grep of the full core 1 listing for `0xE2110`, `0xE2164`, `0xE21C4`: no other reader).

## 6. Corrections to earlier notes

| Earlier statement | Now |
|---|---|
| [organ-tonegen](organ-tonegen.md) s3, [organ-tonewheel-bank](organ-tonewheel-bank.md) s5: "triangle LFO, 65-point lookups, 19-tap delay loops, scanner-like" | pitch-LFO oscillator bank; the 65-point table is a 1,025-point waveshaper; the 19 is a loop count (`LCNTR = 0x13` over 84 oscillators) |
| [organ-tonegen](organ-tonegen.md): mix stage "`dm(i0) = r0` to the ring `DM(0xB33C0)`" | `DM(0xB33C0)` is the saved `I0` of the routine family, not an audio ring |
| [organ-oracle-audio](organ-oracle-audio.md) s1/s7: "post stage input unknown; `DM(0xE2BE0)`, buffers `0xE2110..`" | the post stage has no audio input; `0xE2110` is its output block; `0xE2BE0` is the base word of its sine bank |
| [organ-tone-source](organ-tone-source.md) s2.3: Fine Tune `DM(0xE20E0)` and Vox type `DM(0xE20E3)` feed the post stage | confirmed and decoded: pitch offset and vibrato depth (section 3.1) |
| [organ-tone-source](organ-tone-source.md): Farfisa vibrato type into `DM(0xC27D6/7)` | not read by the post stage (those words belong to the LFO delay `0x122A71`) |

## 7. Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_1_PNO.bin --sram SRAM
cargo build --release -p nord-sharc --bins
python tools/organ_post_oracle.py verify SRAM/core1_sram.bin --exe target/release/sharc-chain --seeds 8
python tools/organ_post_oracle.py render SRAM/core1_sram.bin --exe target/release/sharc-chain --notes 60,64,67 --depth 0 --out chain_novib
python tools/organ_post_oracle.py render SRAM/core1_sram.bin --exe target/release/sharc-chain --notes 60,64,67 --depth 0x240B700 --out chain_vib
python tools/organ_post_oracle.py report chain_vib.f32.npy --notes 60,64,67
python tools/adi_dis.py SRAM/core1_sram.bin 0x244E5A 0x245370      # post stage (byte = 2 pc)
python tools/adi_dis.py SRAM/core1_sram.bin 0x245366 0x245560      # output stage
target/release/sharc-chain SRAM/core1_sram.bin --step 0x12272D:0x1229B2 --trace 0x1228FB:0x1229B3 trace.txt --dump 0xE2164:12
```

## 8. Rust port and comparison with the oracle chord

`ToneWheelSource::Vox` (`crates/nord-core/src/organ/vox_*.rs`, spec [organ-engine](../specs/organ-engine.md) section 3.1.7) ports sections 3 and 4 (the sine bank stays out: weight 0 in the assumed map). Chord C4 E4 G4, weight 1 on oscillators 36, 40, 43, 20,000 passes of `sharc-pass` against `organ_ref --mode vox --rate 43156.4` (mono), `python tools/organ_vox_compare.py`:

| Run | Fundamental level, Rust against oracle (C4 / E4 / G4) | Harmonics 2..9 above -60 dB (max / rms deviation) | First sidebands (oracle, Rust) |
|---|---|---|---|
| no vibrato | -0.0008 / -0.0016 / -0.0020 dB | 1.0 dB / 0.23 dB (20 partials) | none |
| V1, depth `0xE46400` | -0.0011 / -0.0016 / -0.0020 dB | 4.0 dB / 1.1 dB (13) | C4 0.168 / 0.160, 0.167 / 0.161; E4 0.213 / 0.203, 0.212 / 0.204; G4 0.256 / 0.244, 0.255 / 0.244 |
| V3, depth `0x240B700` | -0.0019 / -0.0021 / -0.0025 dB | 0.65 dB / 0.23 dB (10) | C4 0.458 / 0.432, 0.457 / 0.433; E4 0.611 / 0.576, 0.610 / 0.577; G4 0.781 / 0.732, 0.780 / 0.733 |

Harmonics relative to the fundamental (oracle, no vibrato, dB, h = 2..9): C4 -38.9 -22.7 -56.6 -37.6 -63.6 -48.0 -77.7 -56.1; E4 -39.5 -24.3 -58.1 -39.2 -70.5 -50.0 -79.8 -58.4; G4 -21.1 -26.2 -59.6 -41.7 -69.6 -52.7 -82.6 -61.3. The Rust deviations are 0.0 to 0.2 dB except C4 h = 6 (-3.9 dB, level -64 dB), G4 h = 4 (-1.0 dB) and h = 6 (-2.0 dB). The first sideband of the oracle is the unmodified LFO of section 3.1 (5.94 Hz at 43,156.4 Hz; the 5.96 Hz of section 3.1 is a rounding of `0x905A3 / 2^32 * 43,156`, the exact value is 5.941). Differences left: the waveshaper table is the regenerated closed form (rms 1.7e-4 against the image), the one-pole cutoffs are 4-digit ratios, and the f32 arithmetic flips a table index now and then (golden `vox_golden.rs`: max 2.1e-5 without, 2.6e-4 with vibrato against the bit-exact model).

## Implications

- The organ-capable PNO Core1 code has **two independent tone paths**: the 92-wheel bank + mix stage (stereo detune, [organ-oracle-audio](organ-oracle-audio.md)) and the 84-note oscillator bank with a built-in pitch vibrato and fine tune, whose audio leaves through `0x1229B3`. Vibrato depth and Fine Tune act as **pitch** modulation on the second path; no delay-line vibrato exists in either routine.
- The Hammond scanner (`organ/scanner.rs`, 6.85 Hz delay line) is not what this firmware stage implements; if the B3 vibrato is a scanner it lives elsewhere (Core2/ORG strip, open). The Vox/Farfisa vibrato ids reach `0xE20E3` and `0xC27D6/7` (a pitch LFO and an LFO-modulated delay respectively).
- A bit-exact model exists for both routines, so any ARM-side writer of the output-stage record (weights, section coefficients, gains) can be validated by running its words through `sharc-chain`.

## Follow-ups

1. Find the ARM code that builds the `0x1229B3` record (weights `DM(I1 - 0x1F)`, parameter block, three state blocks) and the writer of `DM(0xE2BE0)` (sine base); run its words through `organ_post_oracle.py`. This decides the key to oscillator map (ASSUMED `note - 24`), the drawbar/footage use and the section coefficients (identity here).
2. Decide whether the post stage serves Vox/Farfisa (or Pipe) and the bank + mix the B3: compare per-model install sequences in the ARM replay scripts (`data/arm-replay-organ-model3.script`).
3. Measure the real frame rate (absolute pitch of both banks) and the sine bank's audible role.
4. ~~Rust to-do~~ done in section 8 (`vox_bank.rs`, `vox_shaper.rs`, `vox_output.rs`, `vox_source.rs`, `vox_layer.rs`; sine bank and the 12 buckets not ported). The original note: (`crates/nord-core/src/organ/`, not touched here): a `note_bank.rs` module (84 oscillators: `ph += m rate`, fold, clip by `128.8 * 2^(-k/12)`, 1,025-point waveshaper, asymmetry for notes 12..47, bilinear one-pole `c_k`), a `post_lfo.rs` (triangle `phase += 0x905A3`, depth, fine tune, `m = 0.5 + mod/2`), the sine bank with 12 buckets, and the `output_stage` (96-word weighted sum, three sections, `0.090075`). `scanner.rs` stays the ASSUMED Hammond model; golden vectors come from `organ_post_ref.py` the way `bank_mix_golden.rs` uses `organ_oracle_audio.py golden`. The waveshaper table `T` has to be fitted or regenerated from the oracle (1,025 points, not committable as raw firmware data).

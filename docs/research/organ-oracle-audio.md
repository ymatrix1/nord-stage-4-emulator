# Research: organ audio from the real PNO Core1 DSP code, and the Rust engine against it

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66), the PNO image `SRAM/core1_sram.bin` (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`), the SHARC core model `crates/nord-sharc` with the new harness `sharc-organ` (`crates/nord-sharc/src/bin/sharc_organ.rs`, shared helpers in `crates/nord-sharc/src/harness.rs`), `tools/organ_oracle_audio.py` (new), `crates/nord-core/examples/organ_ref.rs` (new), `tools/ns4_oracle_compare.py`, `docs/research/data/arm-bank-increments.csv`. Builds on [organ-tone-source](organ-tone-source.md), [organ-tonewheel-bank](organ-tonewheel-bank.md), [arm-organ-messages](arm-organ-messages.md), [oracle-vs-engine](oracle-vs-engine.md).
- **Question:** Can the organ's audio be produced by the real DSP code (bank `0x122620` + mix stage `0x12268F`, post stage `0x12272D`) from parameters, and how does the Rust engine (`crates/nord-core/src/organ/`) compare, partial by partial?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command in this note, **EMPIRICAL** = structure or fit read from poking or listings, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Can the oracle produce the organ audio end to end? | Yes for **bank + mix stage**: ARM increments (184 words) + weights + group gains + level written into DM, then per 2-frame call `0x122620` and `0x12268F` (`sharc-organ`), 16,384 frames (3 s wall time) of a C4 E4 G4 chord with drawbars 888000000 to WAV. | VERIFIED |
| What does the mix stage compute? | `weight[i] = a[i] * wB[i] + b[i] * wA[i]` for wheel `i` = 0..**91** (all **92** wheels, element pair `2i, 2i+1`); `tx = sum_i weight[i] x[2i]`, `ty = sum_i weight[i] x[2i+1]`; `out_x = L (g5 tx + g8 ty)`, `out_y = L (g6 ty + g7 tx)`; gains `g5..g8` and level `L` one-pole smoothed (`s = 0.01 t + 0.99 s` per frame). The ARM group gains are exactly this 2x2 matrix: exact table `(1,0,1,0)` = both lanes carry copy 0; detuned `(0.75,0.75,0.375,0.375)` = a stereo blend of the two detuned copies. | VERIFIED (poking) |
| Does a Python model reproduce it? | Yes: 16,384 frames, relative rms error **1.6e-5** (exact) and **1.8e-5** (detuned), max abs 1.1e-4 / 1.4e-4 on peaks of 7.6 / 8.6 (`python tools/organ_oracle_audio.py verify`). The residue is the float fixed point of the smoothers (steady gain 0.999983). | VERIFIED |
| Spectrum | Lone wheel: harmonics 0, -19.0, -36.0, -44.0, -48.0, -106.7 dB. Chord partials equal the weights (1.000 to 1.012, the excess is the quadrature sum with the -19 dB second harmonic of the octave below). Drawbar levels pass through at the weights (8..1 -> 0, -3.3, -6.2, -10.3, -15.2, -19.8, -26.5, -32.5 dB, section 4). | VERIFIED |
| Rust engine against the oracle | Per-partial level within **-0.19..+0.11 dB** of the oracle (spread = the ASSUMED pickup calibration), time-domain SNR **37.7 dB** (exact) and **37.6 dB** (detuned) after one best-fit gain (was 37.7 / **22.6 dB**: the detuned mix weights were wrong). Overall level 0.0792x of the oracle (the ASSUMED `0.08` bus scale times the pickup). | VERIFIED (numbers), causes of the residue EMPIRICAL |
| Post stage `0x12272D` | Runs in the oracle (1,832 steps), updates its LFO phase `DM(0xE21F0)` and delay words `DM(0xE2110..)`, but those words **do not depend on the mix output** (identical with a chord and with zero weights), and the routine writes no audio through `I0`; the final `DM(I0, M2) = R0` at byte `0x2454D0` belongs to the next routine `0x1229B3` (starts at byte `0x245366`, right after the `RTS` at `0x245364`). So the post stage is not downstream of the mix stage in this harness; its audio input is not known. | VERIFIED (dependency test), wiring open |
| Still unknown | Which ARM code writes the weights (so the footage-to-wheel offsets and the 7-bit curve are ASSUMED), the absolute pitch (frame rate and tune index), the ARM int-to-float conversion of the gains, what `wA` is, the post-stage input. | open |

## 2. Method

```text
ARM increments (CSV) + weights + gains + level --pokes--> sharc-organ --(bank, mix x set)--> DM(I0 ..) words
                                                                                 \--> tools/organ_oracle_audio.py --> WAV / .npy / spectrum
OrganLayer (organ_ref example) --WAV--------------------------------------------------------------^   compare / ns4_oracle_compare.py
```

| Piece | File | Role |
|---|---|---|
| Harness | `sharc_organ.rs` | Boots the PNO section (documented start-up, `harness::boot_pno`), per call runs the bank `0x122620` (ret `0x12268C`), then the mix stage `0x12268F` (ret `0x12272A`) once per `--sets` entry with `I1 = 0xB473C + 4 s`, `I0 = out-base + 4 k`, optionally the post stage; pokes between calls (`--script`), captures windows (`--range`, default the mix output: 2 frames x lanes x, y). `--no-bank` for mix-only experiments. |
| Shared code | `harness.rs` | `parse_number`, `register_code`, `parse_script`, `boot_pno`, `call_once` (moved out of `sharc_bank.rs`, which now uses it). |
| Driver | `organ_oracle_audio.py` | `render` (pokes, run, WAV + `.f32.npy`), `report` (partial table), `verify` (oracle vs Python model, both tables), `compare` (oracle vs a Rust WAV), `table` (fit of the firmware x table), `golden` (Rust test vectors). |
| Rust reference | `organ_ref.rs` | `OrganLayer` with percussion, crosstalk, rotary and scanner off, same chord and drawbars, sample rate chosen so wheel 0 has the cycles/frame of the firmware increment (43,156.4 Hz, see section 6). |

The record of the mix stage is found at DM `0xB471C + 4 s`: words `I2, I3, I10, I11` = `0x9594F + 0x65 s`, `0x95AE2 + 0x64 s`, `0x9566E + 0x5C s`, `0x957DE + 0x5C s` for set `s` = 0..3 (read from the image; `I1 = record + 0x20`). The word before the `I2` array, `DM(0x9594E + 0x65 s)`, is the pointer to the bank's x array (`0xC24E5` in all four sets), advanced by `0xB8` per frame, so every set mixes the full 184 elements with its own weights (four clients of one shared bank).

## 3. What the mix stage reads (VERIFIED by poking, unit values in `--no-bank` runs)

Experiments (`sharc-organ --no-bank`, x poked at `0xC24E5 + e` and `+ 0xB8 + e`; floats):

| Experiment | Result |
|---|---|
| `a[0..91] = wB[0..91] = 1`, unit `x[e]`, one `e` at a time, gains and level at 1 | `e` even -> lane x = 1, lane y = 0; `e` odd -> lane y = 1, lane x = 0, for **every** `e` = 0..183 (including 182, 183). |
| Weights only at index `i`, distinct `x[e]` | index `i` adds `x[2i]` to lane x and `x[2i+1]` to lane y (checked for i = 0, 1, 2, 45, 46, 47, 90, 91). One weight serves both copies. |
| `a[i] = wB[i] = 1` versus `b[i] = wA[i] = 1` | identical output: `blend = a * wB + b * wA`. Any single array alone gives 0. |
| Sentinel words in `0x959AB..0x959B6` and `0x95B3E..0x95B48`, one call | `0x959B1, 0x959B2` become `t(0x959AF, B0) * word(AB, AC) + state * word(AD, AE)` per lane; `0x95B40, 0x95B41` likewise from `0x95B3E, 0x95B3F`; `0x95B44, 0x95B45` move towards the target `0x95B42` with the factor `0x95B43` (0.99). Constants in the image: `AB..AC = 0x3C23D70A` (0.01), `AD..AE = 0x3F7D70A3` (0.99), `0x95B43 = 0.99`. |
| Steady state (1500 calls), `(g5,g6,g7,g8)` at `(0.75,0.75,0.375,0.375)`, `L = 1` | unit in copy 0 -> `(0.7500, 0.3750)`, unit in copy 1 -> `(0.3750, 0.7500)`. With `(1,0,1,0)`: copy 0 -> `(1, 1)`, copy 1 -> `(0, 0)`. `L = 0.5, 0.25` scale both lanes (0.375 / 0.1875 for the first case). `L = 0` -> silence. |

Resulting map (set 0; set `s` adds `0x65 s` to the `0x959xx` words and `0x64 s` to `0x95Bxx`, `0x5C s` to the arrays):

| Name | DM address | Role | Tag |
|---|---|---|---|
| `a[i]` | `0x9594F + i` | weight factor, wheel `i` | VERIFIED (poke) |
| `b[i]` | `0x95AE2 + i` | weight factor | VERIFIED |
| `wB[i]` | `0x957DE + i` | weight factor paired with `a` | VERIFIED |
| `wA[i]` | `0x9566E + i` | weight factor paired with `b`; **not sent by the ARM**: it is DSP state, `wA <- a wB + b wA` per frame (`a + b = 1`, so `a` is a smoothing coefficient and `wB` the target; [organ-weights](organ-weights.md) s4) | VERIFIED (poke + run) |
| `g5`, `g6` | `0x959AF`, `0x959B0` | diagonal lane gains (x from tx, y from ty) | VERIFIED |
| `g7`, `g8` | `0x95B3F`, `0x95B3E` | cross gains (y from tx, x from ty) | VERIFIED |
| `L` | `0x95B42` | level target, smoothed with 0.99 | VERIFIED |
| smoother states | `0x959B1..B2`, `0x95B40..41`, `0x95B44..45` | updated in place every frame | VERIFIED |

The ARM selector table maps the ARM calls to exactly these addresses (`0xC022A6EC..`: `0x959AF/0x959B0` for selectors 5, 6; `0x95B3F/0x95B3E` for 7, 8; `0x95B42` for 9; `0x9594E` pointer for selector 0), and `0xC022D9F0` sends `(5, 6, 7, 8) = (0x600000, 0x600000, 0x300000, 0x300000)` or `(0x7FFFFF, 0, 0x7FFFFF, 0)`, which in the matrix above is the stereo detuned blend and the mono exact copy respectively. This also explains why the exact table has two identical copies: lane y is fed from copy 0.

**Correction of [organ-tonewheel-bank](organ-tonewheel-bank.md) s5:** "91 pairs are used, the 92nd is a spare" came from a synthetic record whose scalar tail was zero. With the real record all 92 weights (elements 0..183) are used.

## 4. Results (VERIFIED, `target/oa` outputs of the commands in section 7)

Chord C4 E4 G4 (notes 60, 64, 67), drawbars 888000000, firmware curve (`127/127`), wheel 0 = MIDI 24 (ASSUMED), exact table, 16,384 frames, first 2,048 dropped:

| Wheel | Role | Weight | Oracle partial | Rust partial / best-fit gain | Rust vs oracle |
|---:|---|---:|---:|---:|---:|
| 24 | C4 16' | 1 | 1.0000 | 0.08007 | +0.00 dB (reference) |
| 28 | E4 16' | 1 | 1.0001 | 0.07928 | -0.09 dB |
| 31 | G4 16' | 1 | 1.0002 | 0.08105 | +0.10 dB |
| 36 | C4 8' | 1 | 1.0060 | 0.07891 | -0.18 dB |
| 40 | E4 8' | 1 | 1.0062 | 0.07906 | -0.16 dB |
| 43 | G4 8' + C4 5 1/3' | 2 | 2.0125 | 0.15863 | -0.14 dB |
| 47 | E4 5 1/3' | 1 | 1.0106 | 0.07927 | -0.18 dB |
| 50 | G4 5 1/3' | 1 | 1.0120 | 0.08069 | -0.04 dB |

Both lanes are identical in the exact mode (peak 8.903, rms 2.333). The excess over the weight (up to 1.2 %) is the second, third ... harmonics of lower wheels falling on the same frequency in quadrature (`sqrt(1 + 0.1122^2) = 1.0063` for wheel 36 against wheel 24). Detuned table: peak 9.018 / 8.722, rms 2.585 / 2.574 (lanes differ), oracle partials 1.12 on the 16' wheels (`0.75 + 0.375 = 1.125` into one lane), and lane differences up to 5 % where the two copies beat (wheel 40: 1.0494 / 1.0029).

| Check | Result |
|---|---|
| Lone wheel 36 (weight 1), harmonics 1..6 | `0, -19.00, -36.00, -44.00, -48.00, -106.67 dB` |
| Firmware x table fit (`python tools/organ_oracle_audio.py table SRAM/core1_sram.bin`) | `sin t` coefficient 1.0000000; cosine terms `-0.1122018, -0.0158489, -0.0063096, -0.0039811` for h = 2..5 = `-10^(dB/20)` with exactly -19, -36, -44, -48 dB; h = 6..8 and every sine term below 1e-8; max fit error 7.8e-8. The previous 4-digit closed form was within 3.7e-6. |
| Drawbar levels 1..8 on the 8' of C4 (weight `curve/127`) | amplitudes `0.0236, 0.0472, 0.1024, 0.1732, 0.3071, 0.4882, 0.6850, 1.0000` = `3, 6, 13, 22, 39, 62, 87, 127` over 127 (-32.5, -26.5, -19.8, -15.2, -10.3, -6.2, -3.3, 0 dB) |
| Python model against the oracle, exact / detuned | rel rms 1.6e-5 / 1.8e-5 over 16,384 frames |
| Cost | bank 1,513 steps + mix 548 steps per call |

The weights are an input here (ASSUMED footage offsets `-12, 7, 0, 12, 19, 24, 28, 31, 36`, textbook Hammond; firmware-curve level `value/127`), so the drawbar rows prove that the mix passes them through linearly, not that the ARM writes them like this.

## 5. Rust engine against the oracle

`organ_ref` (percussion, crosstalk, rotary, scanner off; `ToneWheelSource::Firmware`, `DrawbarLaw::Firmware`; sample rate 43,156.4 Hz so that wheel 0 has the cycles/frame of the firmware increment; first 2,048 frames dropped to skip the key click) against the oracle (detuned: oracle lane mean):

| | Before this change | After |
|---|---|---|
| Exact table: overall level, SNR after best gain | 0.0792x, 37.7 dB | 0.0792x, 37.7 dB |
| Detuned table: overall level, SNR after best gain | 0.0702x (-1.0 dB against the exact case; oracle +1.02 dB), **22.6 dB** | 0.0792x, **37.6 dB** |
| Per-partial deviation (exact) | -0.18..+0.10 dB | same |
| x table against the firmware table | 3.7e-6 (4-digit coefficients) | 7.8e-8 (exact levels) |
| Wheels mixed | 91 | 92 in the bank (`advance_lanes`), 91 on the drawbar buses |

What still differs (all outside the verified bank + mix path):

| Difference | Size | Cause / status |
|---|---|---|
| Overall level | 0.0792x | `0.08` drawbar bus scale (ASSUMED), pickup calibration (ASSUMED), no mix-stage level |
| Residual 37.7 dB | per-wheel gain error up to 2 % | the ASSUMED pickup calibration tables multiply each wheel (`tonewheel.rs`); the firmware path has no per-wheel calibration in the bank or mix stage |
| Stereo spread of the detuned table | lanes differ by up to 5 % per partial | resolved: the two lanes are routed (spec 3.1.4); per lane SNR 37.7 / 37.6 dB against 22.6 / 23.3 dB for the old mono path |
| Lane smoothing, level ramp | 100-frame time constant | resolved: `lane_mix.rs` ramps gains and level (spec 3.1.4) |
| Weights | none by construction | both use per-wheel sums of `drawbar * wheel`; the footage offsets, the foldback and the curve as final gain are ASSUMED on both sides |
| Frame rate | not comparable | the oracle gives cycles per frame; the Rust sample rate is a free parameter. Matching wheel 0 to the engine's C1 needs 43,156.4 Hz, matching it at 48 kHz means wheel 0 = 36.37 Hz (D1 minus 16 cent) |

## 6. Rust changes (VERIFIED items only, behind the existing switches)

| Change | Evidence | Where |
|---|---|---|
| x table coefficients `-10^(dB/20)` for -19, -36, -44, -48 dB | table fit, 7.8e-8 | `firmware_bank.rs` `X_HARMONIC_DB`, `tools/organ_bank_ref.py` `X_COS_COEFFS`, `bank_golden.rs` regenerated |
| 92 wheels in `ToneBank` (`BANK_WHEELS`), `advance` outputs the first 91, new `advance_lanes` for the two firmware lanes, `set_increment(copy, wheel, inc, tune)` = `fracmul` | mix stage poke (all 92 weights used) | `firmware_bank.rs`, `vibrato_bank.rs` |
| `lane_matrix(gains)` and mono mean `copy_weights`: exact `[1, 0]`, detuned `[0.5625, 0.5625]` (was normalised `[0.5, 0.5]`) | steady-state matrix `[[g5, g8], [g7, g6]]` | `vibrato_bank.rs` |
| `smooth(state, target)`, `SMOOTH_KEEP = 0.99` | sentinel-word run | `vibrato_bank.rs` |
| `cycles_per_frame(inc, tune) = fracmul(inc, tune) / 2^32`, `implied_frame_rate(wheel0_hz)`, `DEFAULT_TUNE_WORD` | `phase += fracmul(inc, tune)` | `firmware_bank.rs` |
| `gain_value(raw) = raw / 2^23` | smoother targets of 0.75 / 0.375 reproduce the ARM ints | tagged ASSUMED (conversion) |

Tests: `vibrato_bank::tests` (`lane_matrix_...`, `smoother_...`, `copy_ratio_and_weights_...`), `firmware_bank::tests` (`increment_words_give_the_firmware_cycles_per_frame`, `set_increment_uses_fracmul_and_the_92nd_wheel_runs`, table harmonics), `bank_mix_golden::tests` (Rust lanes against the Python model on a synthetic table, `python tools/organ_oracle_audio.py golden`), `tools/tests/test_organ_oracle_audio.py`. The default render hash (`render_hash`) moves from `cb8d83b7...aacd0` to `d98808bd...b7b75` (2,240,000 bytes, max sample difference 2.0e-6, all from the table refinement; the detuned path is off by default).

Not changed: the 91-wheel buses, foldback, pickup, `0.08` scale, percussion, scanner and rotary (ASSUMED parameters, no firmware evidence); the stereo routing of the bank lanes.

## 7. Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_1_PNO.bin --sram SRAM
cargo build --release -p nord-sharc --bins -p nord-core --example organ_ref
python tools/organ_oracle_audio.py render  SRAM/core1_sram.bin --exe target/release/sharc-organ --out chord
python tools/organ_oracle_audio.py render  SRAM/core1_sram.bin --exe target/release/sharc-organ --detuned --out chord_det
python tools/organ_oracle_audio.py report  chord.f32.npy
python tools/organ_oracle_audio.py verify  SRAM/core1_sram.bin --exe target/release/sharc-organ
python tools/organ_oracle_audio.py table   SRAM/core1_sram.bin
target/release/examples/organ_ref --out rust_exact ; target/release/examples/organ_ref --detuned --out rust_det
python tools/organ_oracle_audio.py compare chord.f32.npy rust_exact.wav
python tools/organ_oracle_audio.py compare chord_det.f32.npy rust_det.wav --detuned
python tools/ns4_oracle_compare.py chord.wav rust_exact.wav        # raw sample compare (level differs by design)
# mix-stage experiments of section 3
target/release/sharc-organ SRAM/core1_sram.bin --no-bank --calls 1 --script s.txt --dump 0x959AB:11
```

## Implications

- The oracle reproduces the full "parameters -> DSP -> audio" path for the tone source: increments, 92 weights per set, four lane gains and a level go in, two audio lanes come out. Any future ARM-side weight writer can be validated by running its output through `sharc-organ` instead of through listening.
- The firmware's detuned vibrato/chorus table is a **stereo** ensemble (each lane = 0.75 own copy + 0.375 other copy); the engine's mono path now has the right mean level, the stereo spread is available as `advance_lanes`.
- Four sets share one bank: the organ layer can hold up to four independent weight mixes (candidates: parts A0/A1/B0/B1 or layer x manual). With the exact table the group gains of every set would mute copy 1.

## Follow-ups

1. Done in [organ-weights](organ-weights.md): the ARM code is the bank engine key-down / update pair (`0xC022C3A8`, `0xC022AFB8`); footage offsets, foldback (wheels 0..90) and the 7-bit curve are VERIFIED and the ARM words replay through `sharc-organ` (`tools/organ_weights_replay.py`).
2. Done: `wA` (`0x9566E`) is DSP smoothing state, never written by the ARM.
3. ~~Find the input of the post stage `0x12272D`~~ Answered in [organ-post-stage](organ-post-stage.md): the post stage has no audio input (it is an 84-note oscillator bank with a pitch LFO); `0x1229B3` weights its output block and writes the lanes through `I0`. Open there: the ARM-built record of `0x1229B3`.
4. Frame rate and tune index (absolute pitch): needs a running boot capture or a hardware recording; until then the engine keeps wheel 0 = C1 (ASSUMED) and `implied_frame_rate` documents the 43,156.4 Hz that makes the ladder equal at unity tune; the frame rate is 43,402.34 Hz and the ARM's tune word puts wheel 0 on C1 ([dsp-frame-rate](dsp-frame-rate.md)).
5. ARM int-to-float conversion of the gains (`0xC001FF18`): confirm `raw / 2^23` by running the sender.
6. Route `advance_lanes` into the stereo path of the layer (before the rotary) if the detuned ensemble should keep its spread.

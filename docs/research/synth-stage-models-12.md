# Synth stage models, batch 12: SUPER 2..4 (`0x124985`) and where its wavetable comes from

- **Checked:** 2026-11-14
- **Sources (local, never committed):** PNO / ORG core 2 images (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`), `sharc-org` (`--msg-script`, `--trace`, `--buf`), `sharc-disasm` (listing `0x24930A..0x249872`), the ARM boot capture of [arm-bulk-tables](arm-bulk-tables.md) (`tools/arm_bulk_tables.py capture`: `pno_core2.script`, `pno_l2.script`, `org_l2.script`), `capstone` on `bank0_CODE.bin` (`0xC01ACBF4`, literal pools). Builds on [synth-stage-models-11](synth-stage-models-11.md) s3.
- **Question:** where does the ARM get the wavetable at SDRAM `0x08030000` that the stage `0x124985` (SUPER 2..4, 8 ANALOG layers) reads, and can the stage be modelled word for word?

Tags: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. The wavetable is in the boot capture (answer to s3 of batch 11)

Batch 11 counted 0 of 13,509 `org_core2.script` lines touching `0x803xxxx` and concluded the table was not in the boot log. It is, in the other file: the boot copier `0xC01C0B3C` writes **5,910 words to the DSP L2 byte window `0x200C0000..`** (PNO once, ORG twice), and `tools/arm_bulk_tables.py` keeps those stores in `pno_l2.script` / `org_l2.script` as byte addresses ([arm-bulk-tables](arm-bulk-tables.md) s2; [arm-boot-state](arm-boot-state.md) s1 noted that the L2 stores are `byte >> 2` word addresses `0x0803_0000..` for the runners). So `DM(0xC15A5) = 0x08030000` (word address) is exactly the first L2 byte `0x200C0000`.

| Check | Result |
|---|---|
| Poke `pno_l2.script` as `byte >> 2` DM words into `sharc-org` and run the stage | the stage outputs non-zero words and the model below matches them (s4) |
| Content | 6 segments of 985 words at `+10,000` words (`0x200C0000 + 40,000 k` bytes) are **identical** (VERIFIED: `tools/synth_super24_tables.py extract` raises otherwise); ORG's two copies and PNO's copy are equal. One segment is kept |
| Source on the ARM side | the copier reads int16 pairs from a segment list through `[object + 0x44]` = `0xC043FB54` in `bank0_CODE` (firmware rodata, [arm-bulk-tables](arm-bulk-tables.md) s2); it does not read NAND, the `.ns4b` backup, SPI flash or the sample library. It is **fixed firmware data, not user or sample data** |
| Content | 985 words = 1,970 int16 samples: the band-limited single-cycle tables of all octave bands back to back (two int16 per word, low half first); the bands are addressed by the PM row table (offset, count) |
| Selection of the three waves | `0xC01ACBF4(this, id, aux)` only builds the message list `{id << 16}`, `{4, aux + base}`, `{0xA, flag}`; the DSP stage does not depend on `aux` through the table address (the table is the same for all six segments). What separates `Organ`, `Saw Bright` and `Square Bright` is therefore not the table (open: P block / row words) |

The WAVE (wavetable) bank layers (14 in 12 programs) do **not** read this table: it holds 985 words of fixed band-limited cycles, and the ARM boot writes no other SDRAM-window data (`pno_l2.script` / `org_l2.script` are the only L2 stores). Their data path stays open (OQ22).

## 2. Tables of the stage

| Table | Address | Words | Origin |
|---|---|---|---|
| `rows` | PM `0x9631F` | 3,072 | ARM boot log block `0x96xxx` of `pno_core2.script`; 4 words per band: `offset`, `count`, 2 spare (band 0: 1956, 4; band 54 and up: 0, 128) |
| `wave` | DM `0x08030000` | 985 | L2 stores above |
| `fir` | PM `0x9531F` | 4,096 | static data of the DSP image: 512 phases x 8 taps, 24-bit values in the top bits |
| `lattice` | PM `0x95000` | 5 | static data of the image: `b0 = b2 = 0.2928`, `b1 = 0.5855`, `a1 = 0`, `a2 = 0.1713` (a second-order low pass at a quarter of the rate, unity DC gain) |

They are committed as numbers only in `docs/research/data/synth-super24-tables.txt` (derived tables, 73 kB; D56); `tools/synth_super24_tables.py check` prints the SHA-256, the Rust test compares FNV-1a fingerprints.

## 3. The stage (VERIFIED word for word, s4)

Listing `0x24930A..0x249872`, SIMD (lane x / lane y per register pair). `I1` and `I9` loads are broadcast (BDCST1 / BDCST9), so the row words and the table base are the same for both lanes.

1. `f = clip(DM(-29, I2), 0.5)`, `g = C (B + clip(A - f, B))` (the fade of the other unison stages), `t = s5 + 0.06`, `v = 0.75 (t + 2 t^4)`, `inc_j = (rho_j v + 1 - v) 2 f` for `rho_j = P[-32 + 2 j + lane]`, `j = 0..3`; stored as float words at `0xC1F5E + 6 j`.
2. Band row: `u = clip(2 f, 0x3F7FF972)`, `E` = `EXP(FIX(u, 31))`, `top3` = the three mantissa bits after normalising, `idx = max(0, -(8 max(E, -15) + top3))`, row `0x9631F + 4 idx` read from lane x's `R2` (`I9 = R2`): `A` = offset, `B` = count.
3. Per voice `j` and lane: `D = FIX(inc_j, 31)`, `ph1 = ph + D`, `ph2 = ph1 + D`; the phase word becomes `ph2`. Position `i = (ph B >> 32) + A` (`MRF = ph * B`, unsigned fractional, no shift), fraction `(ph B mod 2^32) >> 23` (9 bits) picks the FIR phase `8 *` it from `0x9531F`; `i2 += B` when the second position wrapped below the first. Six words from `0x08030000 + (i >> 1)` are copied to the lane buffer (`0xB6F80` lane x, `0xB6F88` lane y; the `R4` words `DM(0xB00E6 / 7)` are their short-word addresses `0x16DF00 / 0x16DF10`).
4. Per position and lane, four taps of the 16-bit buffer (the short-word window `0x100000..`): lane x pair `MRF = sum 2 d c >> 32` over the samples `s, s+1, s+4, s+5` with the even coefficients of the phase, lane y pair over `s+1, s+2, s+5, s+6` with the odd ones (`SSF`: product doubled, floor shift). `R1 <-> S0` then `R0 = R0 + R1` adds the pairs: `out_x = (MRF_x + MRF_y) 2^-14` from the lane x buffer, `out_y = (MRB_x + MRB_y) 2^-14` from the lane y buffer (`FLOAT R0 BY -14`, stored as a float word).
5. Mix: per voice `own = o_i W[j][i]`, `cross = o_{1-i} W[j][3-i]` with `W[j] = P[-16 + 4 j .. -13 + 4 j]`; `m = cross + own` (j = 0), then `m += own`, `m += cross`; `a = st(m P[16] g)`.
6. The four outputs are `a a b b` (`a` from position 1, `b` from position 2), then three identical biquad sections (transposed form II: `y = b0 x + s1`, `s1' = b1 x + s2 - a1 y`, `s2' = b2 x - a2 y`, states at `DM(I2 - 3 + 4 n)`, stored as float words) in place over `DM(I2 - 13 ..)`.

Every product and sum rounds to the 40-bit register (`m.mul`, `m.add`), memory words truncate (`m.st`).

## 4. Oracle comparison

| Run | Result |
|---|---|
| `python tools/synth_wave_oracle.py --stage 0x124985 --trials 40` (PNO core 2 image; the ORG image gives the same) | **320 of 320** output words equal; the new voice phases (4 per lane) and the 6 biquad state words per lane also equal in the debug run of seed 1 (not part of the 320) |
| 6 table bases (`DM(0xC15A5) = 0x08030000 + 10,000 k`) x 8 trials | 384 of 384 |
| 40 pitch words over -70..+88 semitones (band indices 0..90, includes the `0.5` clip) | 320 of 320 |
| Recorded cases | `data/synth-stage-oracle-cases-rest.json` key `0x124985` (six cases), replayed by `tools/tests/test_synth_stage_rest.py` without the firmware |

Limits: the model reads the 6 buffer words only (a position pair further apart than 12 samples would read stale memory: not reached with ratios `0.8..1.25`, the oracle draw `2^U(-0.3, 0.3)`); the pitch word and `s5` come through the pass as in the other stages.

**Short-word loads (open, ASSUMED):** `sharc-org` loads the short-word window (`I6 = 0x16DF01`) zero-extended (`core/memory.rs`), the model reproduces that (`signed=False`). A negative sample then adds `65536 c` to the sum, a step of 4.0 in the output at every zero crossing of the table, which no shipped sound can have. The Rust port therefore sign-extends (tagged ASSUMED); whether `sharc-org` or the hardware reading is right needs one CCES probe of `R0 = DM(short address)` with a negative value (follow-up 1).

## 5. Rust (opt-in, `--synth-stages`)

`synth/fw_super2.rs`: `FwStage::Super24` for ANALOG `(6, 2..=4)` in `from_category_wave`, the tables parsed once from the committed file (`prepare()` is called on stage selection, outside the audio thread), `Super24State` (4 `u32` phases, the held value, 6 biquad states; no heap). One lane, one output sample per call: a new interpolated value every second call (the `a a b b` hold), phase step `2 dt (rho v + 1 - v)` per value, the four-tap FIR sums exactly as the model (integer), mix with equal weights `0.25` times `g = fade(2 i_sy)`, then the three biquads. Goldens `tools/synth_stage_golden10.py` -> `tests_fw_stage_golden10.rs`: band index of 40 pitches, the FIR sum of 48 buffers in both extension modes (integer equal), FNV-1a fingerprints of the four tables.

ASSUMED: sign extension (s4), the voice ratios `0.97, 0.99, 1.01, 1.03`, the spread word `0.1 ctrl` (as SUPER 0), equal voice weights and `P[16] = 1` (the P block is open, OQ22), a per-value re-read of the table instead of the firmware's buffer re-use for the second position (equal values; the firmware reads past the cycle end at a wrap, the port wraps the position).

## 6. Coverage

ANALOG layers with a bit-exact stage model: 304 + 8 = **312 of 312 (100 %)**. The Rust port covers them all behind `--synth-stages`, with the assumptions of the P block and the sign extension listed above. Filter layers unchanged (442 of 442).

## Implications

- No ANALOG stage is left without a model; what remains for the synth is the P block (ratios, weights, spread scale, per wave), the strip word maps and the SAMPLE / WAVE bank data paths, not stage equations.
- The three SUPER waves 2..4 share this table; their difference is data the ARM sends (P block / row), not table content.
- Boot capture data that lands in the L2 window is real oracle input: the earlier statement "the boot log does not carry the table" came from reading only the DM scripts.

## Follow-ups

1. One CCES probe of a short-word load of a negative sample (zero or sign extension) to settle s4 and the `signed` flag of the port.
2. Find which P-block words differ between `Organ`, `Saw Bright` and `Square Bright` (the table is shared); a hardware clip of the three waves would fit ratios and weights.
3. WAVE bank data path (14 layers): not this table; look for DMA reads of SDRAM `0x08xxxxxx` outside `0x08030000..0x08030000 + 985`.

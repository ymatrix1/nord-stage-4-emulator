# Research: DSP-side peripherals for live audio in the ORG Core2 and PNO core 1 system runs (D120)

- **Checked:** 2026-11-16
- **Sources (local, never committed):** ORG Core2 image `core2_sram.bin`, PNO core 1 image `core1_sram.bin` (see [org-system-run](org-system-run.md) for how they are built), the ARM message streams `docs/research/data/org-system-{load,key1,key8}-pno1.script`, `org-system-load-core2.script`, the PNO boot log `target/bulk/pno_core1.script` (from `tools/arm_bulk_tables.py`, [arm-bulk-tables](arm-bulk-tables.md)), the Part A demo video (hardware clip, segment `organ_b3` of `docs/research/data/emu_vs_hw_segments.json`). Tools: `sharc-sys` (`--epdma`, `--census`, `--sec-id`, `--lane`, `--wav`), `tools/dsp_lane_vs_hw.py`. Background: [org-system-run](org-system-run.md), [pno-full-pass](pno-full-pass.md), [arm-organ-messages](arm-organ-messages.md), [sharc-core](../specs/sharc-core.md) section 7.
- **Question:** Which DSP-side peripheral registers and DMAs do the real ARM-driven ORG Core2 and PNO core 1 passes touch, what is the smallest model that makes those passes produce live (non-silent) audio in the oracle, and how close is the result to a hardware clip?

Provenance tags: `VERIFIED` = reproduced by the command in the same row; `ASSUMED` = chosen without a hardware check.

## 0. Result in one table

| Question | Answer | Tag |
|---|---|---|
| Does the premise "ORG Core2 needs a peripheral model to make sound" hold? | No. ORG Core2 with the real load stream stays silent with and without the model: every stage slot holds the null stub and the inputs are 0. The organ of this program runs on PNO core 1. | VERIFIED (section 3) |
| What kept PNO core 1 silent in D119? | Not a peripheral. The 80,815-message boot log was not replayed; with it (`--script-flat target/bulk/pno_core1.script` first) lane `2:0` is live and follows the keys. | VERIFIED (section 3) |
| Pass-start interrupt | ORG Core2: the real start is the cross-core store `DM(0x28AC0388)` of Core1 into Core2 `DM(0xB00E2)`, already modelled. PNO core 1: P4I vector 15, already modelled; `--sec-id 94` additionally runs the SEC `CSID` / `END` handshake with identical results. | VERIFIED (section 2) |
| External-port block `0x3E0xx` | A range-transfer block, not a plain DMA. Model: `core::periph::epdma`. Replaces `--pre 0x3e021=1` with 0 differing SRAM words. | VERIFIED (section 2.3) |
| Audio buffer pairs between passes | Not needed for the ORG Core2 stage inputs (sources unknown, signal path is null). PNO lane capture stands in for the transmit DMA read. | ASSUMED |

## 1. Method

Every access outside the SRAM windows is counted by `core::periph::AccessLog` (`sharc-sys --census`); SIMD mode (`MODE1.PEYEN`) moves the pair `addr`, `addr + 1` on every DM access, so a pair address next to a real register is the same access, not a second register ([sharc-simd](sharc-simd.md)). Commands (release build, from the repo root, `IMG` the image, `S=docs/research/data`):

```bash
cargo build --release -p nord-sharc --bin sharc-sys
target/release/sharc-sys $ORG2 --passes 300 --warmup 16 --census --epdma --script-flat $S/org-system-load-core2.script
target/release/sharc-sys $PNO --pno --passes 300 --warmup 16 --census --script-flat target/bulk/pno_core1.script \
    --script-flat $S/org-system-load-pno1.script
```

## 2. Census

### 2.1 ORG Core2, passes 0..315 (14,421 messages delivered with the boot log, 912 without)

| Address | Dir | Value | Per pass | Where | Tag |
|---|---|---|---|---|---|
| `DM(0x3E018..0x3E01F)` | W | `20000000 200C0000 200FA984 20100000 28240000 28B90000 200C0000 200FA984` | boot, 1 each | `0x125C8F..0x125CB9` | VERIFIED (`--census`, last write column) |
| `DM(0x3E002)` | W | `0xBF00` at boot, `0xBF03` per range | passes 15..20: 1 R + 1 W (+ pair `0x3E003`) | `0x127145..0x12714B` | VERIFIED |
| `DM(0x3E000)` | W | `0x01414141` | boot, 1 | `0x125D4C` | VERIFIED |
| `DM(0x3E010)` | W | range start, e.g. `0x200C0000` | 6 passes (15..20), 1 W | `0x127150` | VERIFIED |
| `DM(0x3E011)` | W | range end, e.g. `0x200C9C44` | the 6 passes, 2 W (pair) | `0x127155` | VERIFIED |
| `DM(0x3E021)` | R | completion count, expected 1 | the 6 passes, 1 R (pair `0x3E022` reads 0) | `0x12717C` (loop count `LCNTR`) | VERIFIED read, value ASSUMED (section 2.3) |
| `DM(0x00030024)`, `DM(0x00031400)` | R+W | 4 / 0 | boot, 1 each | `0x125B89`, `0x125BE7` | VERIFIED (boot only; values unused by the passes) |
| `DM(0x282C0380)` | R | 1 (core id word) | boot, 1 | `0x125DA1` | VERIFIED |
| Pass start `DM(0xB00E2)` | W (Core1) | 1 | once per pass, cross-core alias `0x28AC0388` | | VERIFIED |

All other ORG Core2 accesses stay inside SRAM. From pass 21 on no register is touched.

### 2.2 PNO core 1, passes 0..315

| Address | Dir | Value | Per pass | Where | Tag |
|---|---|---|---|---|---|
| `DM(0x3108945C)` | R+W | SEC `CSID` read (1), `DM(0xB00E1)` mailbox store; DMA STAT clear | 316 of 316: 1 R, 1 W | ISR, P4I vector 15 | VERIFIED |
| `DM(0x3108900C)` | W | `END` write that closes the interrupt | 316 of 316: 1 W | ISR | VERIFIED |
| `DM(0x31022330)` | W | 1 | 316 of 316: 1 W | `0x1233A9` | VERIFIED |
| `DM(0x28AC0388)` | W | 1 (Core2 mailbox) | 316 of 316: 1 W | `0x1210D6` | VERIFIED |
| `DM(0x310C3000)` | W | pair | 40 passes, 2 W | | VERIFIED |
| `DM(0x31002000..0x31002318)` six channel blocks, `0x31022000..0x31022308`, `0x31004000..0x310041B0`, `0x30FFE100..0x30FFF098`, `0x31044004/8` | R+W | DMA and SPORT/SPI set-up words | boot only (122 addresses) | `0x1231xx..0x1235xx`, `0x122Fxx` | VERIFIED (boot only) |

Per pass the PNO core touches 5 addresses outside SRAM, all start-up handshake; nothing in the pass reads a data register that would have to change for audio. The transmit side (SPORT DMA reading the pass buffers) never appears as a DSP access.

Pass-start interrupt: `sharc-sys --pno` already raises P4I vector 15 with status 1 (D119). `--sec-id 94` instead raises SEC source 94 (found from the DMA6 clear in the boot census, ASSUMED to be the right source) and lets the ISR read it from `CSID` and end it with `END`: the pass hashes are identical to the status-1 path (VERIFIED: pass-buffer hash `da657dfef24bbe91` both ways, 316 passes).

### 2.3 The `0x3E0xx` rule

Reading of the census: a range is opened by `DM(0x3E010) = start` while both low bits of `DM(0x3E002)` are set (the start routine does `BIT SET USTAT1 0x3` first and never clears it); `DM(0x3E011) = end`; the next read of `DM(0x3E021)` returns the number of ranges opened since its last read and clears it. The six started ranges tile `0x200C0000..0x200FA984`. Implementation `core::periph::epdma`, enabled by `sharc-sys --epdma`. ASSUMED: the register names, the unit of the count, "one range, count 1". The old stub `--pre 0x3e021=1` supplied the same value.

| Check | Command | Result | Tag |
|---|---|---|---|
| `--epdma` vs `--pre 0x3e021=1`, state compare over all SRAM regions | `sharc-sys $ORG2 --passes 300 --warmup 16 --epdma --script-flat $S/org-system-load-core2.script --state-out a.state` and the same with `--pre 0x3e021=1 --state-out b.state`, then `--state-cmp` | 0 words differ, steps first 4309 / min 3171 / max 6377 in both | VERIFIED |
| Flags off | `sharc-sys` without `--epdma/--census/--sec-id` | identical to the D119 numbers (hashes in [org-system-run](org-system-run.md)) | VERIFIED |

## 3. Runs

300 passes after 16 warm-up, interpreter, all passes return to the poll loop (no fault). `peak` / `rms` are over the output lane `2:0` (pointer set 2, pair 0 of `DM(0xB0159 + set)`, 2 frames per pass, float words). Boot log: `target/bulk/pno_core1.script`, then the load stream, then the key stream at pass 16.

| Core | Case | Steps first / min / max | Pass buffers changed | Lane `2:0` peak | Lane `2:0` rms |
|---|---|---|---|---|---|
| ORG Core2 | load + idle (= 1 key = 8 keys, the key scripts are empty) | 4309 / 3171 / 6377 | 0 words changed `0xB0000..` (hash `8d6538289032c3c3`) | 0 | 0 |
| PNO core 1 | load + idle | 7880 / 7881 / 7891 | 328 words (hash `da657dfef24bbe91`) | 0.000000 | 0.000000 |
| PNO core 1 | 1 key | 7880 / 7881 / 7891 | 328 words (same hash) | 0.026519 | 0.005851 |
| PNO core 1 | 8 keys | 7880 / 7881 / 7891 | 328 words (same hash) | 0.050618 | 0.009278 |

Findings:

- No fault, no step-limit hit, steps per pass constant for PNO (7,889.7 mean). The signal is zero when idle and grows with the number of keys, so key messages reach the audio path.
- The pass-buffer hash `da657dfef24bbe91` is the same for idle, 1 key and 8 keys: the `0xB0000..` window is not where the voice signal lives. The lane words come from the output pointer sets in the sparse device store, so the signal is found only by `--lane`.
- ORG Core2 stays silent by construction (null stage slots, inputs 0). The key messages of this stream do not target Core2.
- Boot log: the load stream alone is not enough. With the boot log the lane is live; without it (D119 setup) the lane is 0. Counts: `grep -c` of the boot log shows 116 words in `0x95471..0x954E5` (B3 FIR taps) and 3 writes of `0xB44C0` (dry gain), against 0 of both in the load stream (VERIFIED, `grep` over `target/bulk/pno_core1.script` and `docs/research/data/org-system-load-pno1.script`).
- A short replay that keeps only three boot words (`0x95495`, `0x9549B`, `0xB4540`) was not reproduced by a committed command (1500 passes: peak 0.000062, the full boot log gives 0.1727). It is not claimed; which boot words are required stays open (OQ below).

```bash
S=docs/research/data; BL=target/bulk/pno_core1.script
target/release/sharc-sys $PNO --pno --passes 300 --warmup 16 --script-flat $BL --script-flat $S/org-system-load-pno1.script \
    [--script-at 16:$S/org-system-key8-pno1.script] --lane 2:0
```

## 4. What is still missing

| Item | Status |
|---|---|
| ORG Core2 stage inputs (audio buffer pairs between passes) | Not modelled: no source is known. The ORG slots here are the null stub, so the input is never read. |
| PNO to FX hand-over | The lane capture reads the output pointers after every pass, standing in for the TX DMA. The FX chip is not modelled. |
| SPI flash and sample DMA | Out of scope; sample playback needs it. |
| Other cores | Out of scope. |

## 5. Hardware comparison (PNO lane vs the `organ_b3` hardware clip)

The lane is the dry DSP output of PNO core 1 before the FX chip; the hardware clip is the final stereo mix of the demo video, so these numbers are shape and level evidence, not a calibration. Command (run from the repo root):

```bash
PYTHONPATH=tools python tools/dsp_lane_vs_hw.py --video "$VIDEO" --workdir $WORK --slice-s 1.8 key8.wav
sharc-sys $PNO --pno --passes 44032 --warmup 16 --script-flat $BL --script-flat $S/org-system-load-pno1.script \
    --script-at 16:$S/org-system-key8-pno1.script --lane 2:0 --wav key8.wav
```

| Clip (3 s slice of the hardware clip, 8-key lane of about 22,000 passes) | RMS delta (oracle - hw) | Centroid oracle / hw | Gap score (no air band) | Worst band |
|---|---|---|---|---|
| idle | -97.4 dB (oracle silent) | 0 / 485 Hz | 17.3 dB | sub 20-120 +43.5 |
| 1 key | -8.4 dB | 246 / 485 Hz | 19.1 dB | brilliance 3-8k -46.7 |
| 8 keys | -1.9 dB | 325 / 485 Hz | 13.3 dB | sub 20-120 +28.6 |

| Clip (1.8 s slice, 44,048 passes, 8 keys) | Value |
|---|---|
| RMS oracle / hardware | -24.4 / -24.4 dBFS (delta 0.0 dB) |
| Centroid oracle / hardware | 361 / 338 Hz |
| Band delta dB (sub, low, mid, presence, brilliance, air) | +27.5, -3.7, +1.0, +7.4, +15.4, -18.5 |
| Gap score | 11.0 dB |

First difference: the sub 20-120 Hz band is 27.5 dB too strong in the oracle lane against the clip while the 120 Hz to 1.5 kHz bands are within 4 dB, then presence and brilliance are +7 and +15 dB. The level match is partly coincidental (dry lane against final mix, the notes and the velocity of the hardware clip are not the replayed key messages); the spectral shape differences are the finding. Tag EMPIRICAL for the numbers, no `VERIFIED` claim of parity.

## 6. Implications

- The peripheral work needed for PNO core 1 audio is small: the boot log, the P4I start, and a lane read. No data DMA model was needed.
- `0x3E0xx` for ORG Core2 is a start/count handshake; the opt-in `--epdma` model is enough and equals the old preset.
- ORG Core2 audio needs a different program (one that fills the stage slots), not more peripherals.
- A faithful FX hand-over needs the SPORT TX path and the FX chip; out of scope here.

## 7. Follow-ups

1. Find the minimum set of boot words PNO needs (ddmin on the boot log with a committed script); candidates `0x951F1..0xB4540` (B3 preset word family, [arm-organ-messages](arm-organ-messages.md)).
2. Replay a program that activates ORG Core2 stage slots to test the Core2 input path.
3. Explain the +27 dB sub band (DC or low-frequency content in the dry lane) and the missing brilliance of the lane before the FX chip.
4. Hardware check of `DM(0x3E021)` semantics (OQ).

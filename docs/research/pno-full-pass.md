# Research: the complete PNO core 1 pass in the oracle (`sharc-pass`), Vox and Pipe end to end

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the PNO image `SRAM/core1_sram.bin` (OS v1.66, `tools/ns4_dsp_images.py` + `tools/ns4_dsp_ldr.py --sram`), the settled ARM state (`tools/arm_steady.py` snapshot), `capstone` listing of `bank0_CODE.bin`, `unicorn` ARM runs, `crates/nord-sharc` (`sharc-pass`, new), `crates/nord-tools/src/bin/organ_ref.rs`. Builds on [pno-dsp-map](pno-dsp-map.md) s3, [sharc-sequencer](sharc-sequencer.md) s3, [pno-module-chain](pno-module-chain.md), [organ-oracle-audio](organ-oracle-audio.md), [organ-weights](organ-weights.md), [organ-post-stage](organ-post-stage.md) and the organ model map of PR #116 (`organ-model-paths.md`, not yet on `main`: model 1 Vox installs `0x12272D` + `0x1229B3`, models 3/4 Pipe install `0x122620` + `0x12268F`).
- **Question:** Can the oracle run the complete PNO core 1 pass (call table, 18-module chain with ramped gains, ping-pong buffers, P4I start) with the ARM-built state, so that a Vox chord and a Pipe chord come out end to end, and what writes the 96 `0x1229B3` weights?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = reading of data, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Complete pass in the oracle? | Yes. `sharc-pass` boots `0x121007` with the board gates, applies ARM message scripts, starts **every pass with the P4I interrupt** (the ISR stores the status in `DM(0xB00E1)`, the loop runs the 8 slot calls, the chain and the output stage, then clears it). 3,278 steps per pass (Pipe), 3,300 to 3,311 (Vox), no skipped operation, no fault, deterministic. | VERIFIED |
| What is one pass? | **2 frames** of a stereo pair. The output pointers `DM(0xB0159..0xB015E)` (set 0..5) point into the ping-pong windows `0xA2B00xx`; each set holds 3 pairs of 4 words `[L0 R0 L1 R1]` (SIMD register pairs `R/S` of `DM(0, I)` and `DM(2, I)`, the order of `sharc-organ`'s `(out_x, out_y)`). The audio is **set 2, pair 0** (`DM(0xB015B)`); set 0 pair 2 holds start-up residue (`-2574`) and the other sets stay 0. | VERIFIED |
| Chain with empty modules | All 18 chain records are the pass-through stub `0x12637B` (the image ships them so; no ARM burst installs a chain module for the organ). With the ramps settled the chain is **unity gain**: full pass against bank + mix stage `sharc-organ`: lag 0, gain **0.99987**, SNR **72.3 dB** (Pipe chord; the R lane is 0: the ARM group gains of model 3 are `(1, 0, 0, 0)`). | VERIFIED |
| Pipe chord against the Rust engine | `OrganLayer`, `WeightMode::Firmware`, level 1: lag 2 frames (the replay script is poked at pass 1), gain **1.00014**, SNR **45.3 dB**, peak 0.090375 against 0.090382, rms 0.036097 against 0.036101. Absolute scale agrees, no fitted gain needed. | VERIFIED |
| Vox chord | Weights 1.0 on the oscillators `note - 24` of C4 E4 G4 (ASSUMED map): peak 0.117, rms 0.042 (1,500 passes); three fundamentals at the 12-TET ratios (E/C 1.25993, G/C 1.49817 against 1.25992, 1.49831, FFT resolution 1e-4), harmonics at most -25 dB (3rd of C4 0.0028 against 0.0543). Vox is **9.9 cents flat** against the Pipe bank at the same note: cycles/frame ratio 0.99434 for all three notes (30,000 passes). | VERIFIED (run), map ASSUMED |
| Who writes `PM(0x95E46 + 0x60 i)`? | The **ARM**, by a weight sender `0xC0224468` of the voice engine object, not the DSP: no DSP store hits `0x95E00..0x95FFF` in any pass except the section states `0x95FCA..D` of `0x1229B3` itself. The same ARM code zeroes the 96 words on an engine reset (`0xC0225374`), which is the zero burst of a Vox to Vox switch. Section 6. | VERIFIED (negative on the DSP, ARM code read and run), key to flag map open |
| Key-down burst of the second engine family (`0xC00BF890`) | **Not reproducible** in the settled harness: the record objects `[0xC055F4B8 + 4 s]` have an empty key range (`[+0xC] = 0xC027B920`, 0 items), the call returns without a message. | VERIFIED (negative) |

## 2. The harness (`crates/nord-sharc/src/bin/sharc_pass*`)

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_1_PNO.bin --sram SRAM
cargo build --release -p nord-sharc
target/release/sharc-pass SRAM/core1_sram.bin --script docs/research/data/pno-pass-install-vox.script \
    --script docs/research/data/pno-pass-vox-weights-c4e4g4.script --passes 1500 --quiet --lane 2:0 --wav vox.wav
```

| Piece | Role |
|---|---|
| `runner/` (library, moved out of `sharc-org`) | `pass.rs` one pass (start by mailbox or by the receive interrupt, step limit, changed words, store sites, calls), `msg.rs` ARM style `(word 1, word 2)` messages and the `PASS W1 W2` script format, `audio.rs` buffer capture and float WAV. `sharc-org` uses it unchanged (its tests pass). |
| `sharc-pass` | PNO defaults (entry `0x121007`, poll loop `0x121091`, mailbox `0xB00E1`, gates `DM(0x28AC0380)=1`, `DM(0x3108D008)=5`, `DM(0x3108C06C)=0x400`, `DM(0x3108C00C)=4`, FLAG1 high, `P4I` with status 1), `--script FILE[@PASS]`, `--poke`, `--lane SET:PAIR` + `--wav`, `--raw` (all 6 sets x 3 pairs every pass), `--watch LO:LEN` (store sites merged over all passes), `--calls`, `--dump`. The ping-pong word `DM(0xB0158)` is toggled before every pass like the `P0I` ISR would (`0x28AC01E0` / `0x28AC0000`). |
| `tools/arm_pass_script.py` | Drives the booted ARM firmware: `Edt_Org_kModel = N` (`0xC011C9BC`, then the chain step `0xC022F060`) and writes the ordered PNO core 1 messages as a script (`--previous M`, `--ticks N`). |
| `tools/pno_full_pass.py` | `run`, `compare` (full pass against `sharc-organ`), `rust` (against `organ_ref`). |

Scripts (addresses and values only, `docs/research/data/`): `pno-pass-install-vox.script` (343 words), `pno-pass-install-pipe1.script` (339), `pno-pass-keys-c4e4g4.script` (key-down burst of the bank engines for C4 E4 G4, drawbars 888000000, [organ-weights](organ-weights.md)), `pno-pass-vox-weights-c4e4g4.script` (ASSUMED Vox weights), and the existing `arm-replay-organ-model3.script` (bank increments, gains).

State after the install (VERIFIED, `--dump 0xC01D6:32`):

| Slot | Vox | Pipe1 |
|---|---|---|
| 2 (generator, state record `0xC00FA`) | `0x12272D` | `0x122620` |
| 3 | `0x121B33` (idle) | `0x121B33` |
| 4 (processor, state record `0xC00EA`) | `0x1229B3`, `I1 = 0xE2D42` | `0x12268F`, `I1 = 0xB473C` |
| 0, 1, 5, 6, 7 | `0x12637B` (image stub) | `0x12637B` |

## 3. The pass, measured

| Fact | Value | Tag |
|---|---|---|
| Called routines in a pass | Vox: `0x12272D`, `0x121B33`, `0x1229B3`, 23 stub calls; Pipe: `0x122620`, `0x121B33`, `0x12268F`, 23 stub calls (`sharc-pass --calls`) | VERIFIED |
| Chain head | `DM(0xC2F48) = 0xC2E4A` (unchanged by every captured ARM burst) | VERIFIED |
| Final buffers | 3 pairs `0xC014A`, `0xC0152`, `0xC013A` copied to `DM(0xB015B)`, and `0xC015A`, `0xC0162`, `0xC0142` to `DM(0xB015D)` (listing `0x121A21..0x121AE0`, `0x2435D2..0x24364A`) | VERIFIED (listing) |
| Slot outputs | the slot state records `0xC00EA..` hold `[x0 y0 x1 y1]` of the slot 4 routine; the first non-zero values appear after the smoothers (0.01 per frame) have run | VERIFIED |
| Settling | the mix stage smoothers and the 18 chain ramps settle in about 1,500 passes (3,000 frames) | EMPIRICAL |

```bash
python tools/pno_full_pass.py compare SRAM/core1_sram.bin --script docs/research/data/pno-pass-install-pipe1.script \
    --script docs/research/data/arm-replay-organ-model3.script --script docs/research/data/pno-pass-keys-c4e4g4.script --passes 4000
# full pass vs bank+mix: lag 0 frames, gain 0.99987, SNR 72.3 dB
target/release/organ_ref --weights firmware --level 1.0 --notes 60,64,67 --drawbars 888000000 --frames 8000 --out rust_fw
python tools/pno_full_pass.py rust SRAM/core1_sram.bin rust_fw.csv --script <the same three scripts> --passes 4000
# rust vs full pass (lane L): lag 2 frames, gain 1.00014, SNR 45.3 dB
```

(The `rust` run uses the same three script files plus the level script that the tool adds; the replay script pokes at pass 1, hence the 2 frame lag.)

## 4. Vox audio

C4 E4 G4 with weight 1.0 on oscillators 36, 40, 43 (`DM(0x95E6A)`, `0x95E6E`, `0x95E71`, ASSUMED map `note - 24` and weight 1, the level `L = DM(0x95B42)` 1.0 ASSUMED):

| Measurement | Value |
|---|---|
| Output | set 2 pair 0; L rms 0.0525, R rms 0.0572 (3,000 passes): the two lanes differ |
| Fundamentals (cycles per frame, 30,000 passes) | 0.0060282, 0.0075951, 0.0090313 |
| Ratios | 1.25993 (ET 1.25992), 1.49817 (ET 1.49831) |
| Pipe bank at the same notes | 0.0060625, 0.0076380, 0.0090828; Vox / Pipe = 0.99434 for all three |
| Per-oscillator amplitude (unit weight) | 0.0543, 0.0525, 0.0441: falls with pitch (the low pass sections `0.9514`, `0.8348` of [organ-model-paths](organ-model-paths.md) s4 are in the signal path) |

Reading (ASSUMED): the Vox generator tunes its oscillators on the same 12-TET grid as the Pipe bank; the constant 0.99434 (-9.9 cents) is not explained (a different table constant of the Vox generator, or the LFO centre), and the absolute pitch needs the frame rate, still unknown.

## 5. What the DSP does not do (VERIFIED)

`sharc-pass --watch 0x95E00:0x200` over 5 and 3,000 passes (Vox install): the only stores are `pc 0x122A40` (`0x95FCA..B`) and `pc 0x122A4E` (`0x95FCC..D`), 4 per pass each, the section states inside the `0x1229B3` parameter block. Nothing in the pass, the start-up (`--boot-stores`) or the ISRs writes the weights. The weights are inputs delivered by the ARM.

## 6. The ARM weight sender (`0xC0224468`), disassembly and runs

Found by scanning the ARM image for the DM word addresses (the earlier search looked for system addresses `0x28257918` in code and for literals in code only): the **table `{0x95E46, 0x95EA6, 0x95F06, 0x95F66}` at `0xC0282ED0`** (weight bases of records 0..3) and the table `0xC0490CF0` of `(weights base, parameter block, ...)` pairs, with the code pool users at `0xC0224464`, `0xC0224640`, `0xC0225504`, `0xC0225ABC`, `0xC022FFB8`. The code (`capstone`, `0xC0224468`..`0xC022462C`; this pseudo code is EMPIRICAL, a reading of the listing that was not run end to end), for an engine object `this`:

```text
for i in 0 .. 0x5F:                                     # 96 weights
    if byte[this + 0x294 + 0x14 i + id] != 0:           # key flag row i, footage slot id
        s = sum over j < 20 with flag[i][j] != 0 of  int32[this + 0xD18 + 8 j]    # levels
        w = s * (taper from [this + 0xA14 + 8 i] when that word is non-zero)      # 0xC025C2E4
    w = min(w, cap);  if w != cache[this + 0x60A0 + 4 i]:
        cache = w;  send DM( table[this + 0xC][i] ) = float(w)     # 0xC0282ED0 + i, or DM(i) when [this + 0xC] > 0x11
```

| Fact | Evidence | Tag |
|---|---|---|
| `0xC0228E00(this, id, value)` is the setter of the 9 level slots (jump table at `0xC0228E28`, each case stores the level, clears the 8 queue entries, jumps to `0xC0224468`); called by `0xC022C678` (drawbar edit) and by the chain step at `0xC0230444` for ids 0..8 | disassembly, `blscan` | VERIFIED (listing) |
| `0xC0225374(this, ...)` resets the engine: clears the key flag rows and queue and sends 0 to all 96 words | listing `0xC0225374..0xC0225508`; run: Vox after Vox (`--previous 1 --model 1`) sends exactly `DM(0x95E46..0x95EA5) = 0` twice (200 messages, 196 in `0x95E00..0x95FFF`); Vox after B3 sends none | VERIFIED (run) |
| With `[this + 0xC] = 0` the sender writes floats (0.875, 0.5, ...) to `DM(0x95E46 + i)` | `0xC0224468(sub, 0)` on the engine object `[bank engine + 0x7C]` of the settled state (19 messages, `0x95E89 = 0x3F60013C`, `0x95E8B = 0x3F00011E`) | VERIFIED (run), values depend on unbuilt key tables |
| The engine objects of the settled state have `[this + 0xC] = 0x40`, so their weights would go to `DM(i)` (IOP words): the destination index is not set up because the boot is partial | `arm_organ_weights.BankDriver` objects | VERIFIED |
| What sets the key flags (key to oscillator map) and the taper words | not found: the bank engine key-down `0xC022C3A8` writes `a`, `b`, `wB` for the B3/Pipe sets, the second family `0xC00BF890` is inert in the harness | open |

Consequence: the Vox weights are `sum of the drawbar-like levels of the flagged footage slots x taper`, computed by the ARM per key, with the destination chosen by `[this + 0xC]` (record index 0..3 = `0x95E46 + 0x60 i`, indices up to `0x11` map the other tables `0xC0443AF4`, `0xC0443D90`). The oscillator index map `note - 24` stays ASSUMED until the flag writer is found.

## 7. Provenance summary

| Item | Status |
|---|---|
| Complete pass, P4I start, 2 frames per pass, output set 2 pair 0, chain unity | VERIFIED |
| Pipe chord: full pass = bank + mix (72.3 dB), = Rust Firmware mode (45.3 dB, gain 1.00014) | VERIFIED |
| Vox chord pitch ratios, harmonics | VERIFIED (run) |
| Vox weight map `note - 24`, weight 1, level `L` 1.0 | ASSUMED |
| Weights come from ARM `0xC0224468`, reset by `0xC0225374` | VERIFIED (listing and runs) |
| Key flag writer, taper words, `[this + 0xC]` setup | open |
| Absolute pitch (frame rate), Vox 9.9 cents flat reading | ASSUMED |

## Implications

- The oracle now runs the whole PNO core 1 pass; any future ARM capture (a piano program, a second organ layer, a chain module install) is one `--script` away.
- For the Rust engine: the Pipe path reproduces the full pass to 45 dB in absolute scale (the remaining 0.0001 of gain and the residue are the ASSUMED pickup tables of `tonewheel.rs` and the smoother start values). The Vox path is not comparable until the weights are real.
- Chain modules stay stubs for the organ: the ARM burst of the two model installs touches only slots 2 to 4.

## Follow-ups

1. Find the writer of the key flag bytes `this + 0x294 + 0x14 i + id` and of the taper words `this + 0xA14 + 8 i`; the candidates are the virtual methods of the engine vtable `0xC0443890` (`0xC02240F8`, `0xC02251F0`, `0xC02250A4`, `0xC0224014`, `0xC0223F3C`) and the 12 objects with that vtable in the settled heap.
2. Set `[this + 0xC]` the way the model install does (the engine objects show `0x40`: the boot stub) and replay a Vox key through the real sender into `sharc-pass`; replace the ASSUMED weights of section 4.
3. Run the second organ layer (record 2) and the Farfisa / B3 installs (processors `0x122B85`, `0x121E2B`) through `sharc-pass`.
4. Frame rate and absolute pitch (the Vox -9.9 cents).

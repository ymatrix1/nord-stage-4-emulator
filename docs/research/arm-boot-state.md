# Post-boot oracle state: equality for every core, and what it changes

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the boot log of [arm-bulk-tables](arm-bulk-tables.md) (`tools/arm_bulk_tables.py capture`, 96,111 messages), `sharc-pass`, `sharc-org`, `sharc-fx` (`crates/nord-sharc`), the DSP images (`tools/ns4_dsp_images.py`, `ns4_dsp_ldr.py --sram`), `tools/organ_fb_pass.py`, the ARM harness (`arm_edit_capture.py`), disassembly with `capstone`. Derived data only: [data/arm-bulk-tables.json](data/arm-bulk-tables.json).
- **Question:** does the oracle state equal the post-boot device state for every core, and do the open organ gaps (silent Farfisa pass, tail tables, dry gain `0xB44C0`, vibrato gate) change once the full boot log is replayed?

Tags: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. Equality of the oracle with the boot state

Each core's boot log (send order, boot then settle) was replayed, then every distinct DM word the ARM wrote was read back and compared with the last value the ARM sent.

| Core | Runner | Words | Identical | Note |
|---|---|---|---|---|
| PNO core 1 | `sharc-pass --script`, 1 and 40 passes | 8,306 | 8,300 | the 6 others are the mailbox words `0xB00FE..0xB0103` that the pass rewrites |
| PNO core 2 | `sharc-org --msg-base 0 --msg-script` (PNO `core2_sram.bin`), 2 passes | 12,102 | 12,102 | strict, skipped ops 0 |
| ORG core 1, core 2 | `sharc-org`, 2 passes | 12,102 each | 12,102 each | |
| FX core 1, core 2 | `sharc-fx @args` with `--poke` per word, one 1-step `--call` so the dump runs, `--dump LO:N` | 2,560 / 2,555 | 2,560 / 2,555 | delivery check only; no FX routine ran |
| SY (63 words), SMP (12 + 686 words) | path 1 words into the SY1SY2 / SMP images | | not checked | the SPI-slave boot kernel images have no pass runner here; delivery into a DM word is the same store as above |

**L2 / external RAM block (5,910 words per target).** The byte address `0x200C0000` lies in the SC57x L2 window (`core/periph/l2.rs`, byte addressed from `0x20000000`) and the section zero-fills 240,000 bytes there ([dsp-image-hunt](dsp-image-hunt.md) 5.x). The pass runners model data memory as normal words, so the stores were converted to word addresses (`byte >> 2`, `0x0803_0000..`) and delivered by `--msg-script`. A load watch (`sharc-org --rwatch 0x8000000:0x100000` and the byte/word variants of `0x200C0000`) over 3 ORG core 2 passes reports **no read**: the pass chains do not consume this table. The two ORG copies differ word for word (5,910 of 5,910), so they are per core data (ORG core 1 first, core 2 second: ASSUMED). The consumer is not one of the DM pass chains (DMA or a section the pass runners do not run: open). No Rust change was needed: the delivery path is the word conversion above.

## 2. Gaps against the full boot log (VERIFIED, final PNO core 1 state of the log)

| Item | Earlier | With the boot log |
|---|---|---|
| Dry gain `DM 0xB44C0` | 1.0 ASSUMED, writer unknown (OQ16) | **written 3 times in the boot, `0x3F800000`, by the int sender `0xC001FF18` (pc `0xC001FFA0`)**: the writer is the boot, the value is 1.0 |
| `DM 0xB44AE`, `0xB452E` | 0 | 0 (gates off, default program) |
| `DM 0xB44B0`, `0xB4530` | 1.0 | 1.0 (boot), 0 after any vibrato type message ([arm-bulk-tables](arm-bulk-tables.md) s4) |
| Farfisa weights `DM 0x95FFC..0x96173` | zero | **still never written in the boot**: they are key-down words |
| Farfisa lattice table `DM 0x965FC..` | zero | the range `0x965FC..0x966FF` has 204 boot words, 200 non-zero; the first words `0x965FC`, `0x96600` are not among them |
| Taps `DM 0x95471..0x954D8` | constructor | 104 words, all non-zero (as before) |
| B3 key weight arrays `0x951F1..`, `0x95331..` | zero | zero (key-down words) |

## 3. Payload of `0xC0240548` and the vibrato enable byte (VERIFIED)

`0xC0240548(obj, flag)` is **not a table download**. It sends the two words `([[obj + 4] + 0xC], flag)` through comm `[obj]` = 4 (SMP), runs the handshake `0xC006CCBC` and returns. The descriptor `[obj + 4]` = `0xC0444E68` is an address table in `bank0_CODE`: `0x940A1, 0x940A0, 0x9409F, 0x92518, 0x92519, 0x4000`. Word `+0xC` is **`0x92518`**: the call writes the SMP DSP mailbox `DM 0x92518 = flag` (1 at `0xC024888C`, 0 at `0xC02495D8`) and the ARM then reads `DM 0x92519` back with the SPI peek (`0xC0066E64`, 30,003 tries in the harness because the stub answers 0). It is the SMP boot handshake (start / done flags), consistent with the peek port of [sharc-chip-identity](sharc-chip-identity.md).

The vibrato enable byte `0xC059FF5C + [layer + 0x876C] + 0x458` is written by the edit handler for `Edt_Org_Preset_kVibratoEnable` (`0x16C`) at `0xC011CBB8` as `(value == 1)`. The boot writes it (value 0, the default program) for both layers (write watch). A loaded preset sets it by applying its own `0x16C` value through the same handler, so the vibrato words are whatever the program's edit value gives; no separate preset-load code path writes the byte. The harness dispatch of `0x16C` reaches a different client handler that does not store it (s2 of arm-bulk-tables).

## 4. Chords on top of the full boot log

Method: `tools/organ_fb_pass.py` chord capture (keys 60, 64, 67, drawbars 888000000, key-down of the drawbar and percussion items) after the model install, prefixed either by the earlier composition (engine constructor re-run + 10 settle windows, 22,587 PNO core 1 words) or by the **full boot log** (29,760 words: boot + settle). `sharc-pass` PNO core 1, 8,000 passes, lane set 2 pair 0.

| Model | Earlier prefix | Full boot log as is | Full boot log without `DM 0xB2ECF`, `0xB2FF0` |
|---|---|---|---|
| 5 B3 Bass | 262.7 / 328.4 / 391.2 Hz at 0 / -3 / -6 dB, side line 268.4 Hz at -14 dB, peak 0.0083 | **silent** | 262.7 / 328.4 / 391.2 Hz at **0 / -1 / -2 dB**, next line 522.5 Hz at -38 dB, peak **0.0744** (9 x louder, no -14 dB side line) |
| 0 B3 | silent (dry gain 0) | silent | **sounds**: peak 0.0522, 391.2 Hz (0 dB), 131.3, 165.6, 197.0 Hz (-1, -2, -3 dB) |
| 2 Farfisa | silent | silent | silent (weights `0x95FFC..` never written) |
| 1 Vox | silent | silent | silent |

The two words are the only cause of the silence in the full-log replay (a delta search over the 364 boot words that the earlier prefix lacks: removing exactly `DM 0xB2ECF` and `DM 0xB2FF0` restores the sound, 12 rounds). The boot writes them once each, `0xBDA3D70B` and `0xBD851EB8` (about -0.080 and -0.065), from the module parameter sender at `0xC022E3E8` (the `0xC022DDC8` family), and nothing in the replayed model install rewrites them. Either the real model install rewrites them (and the harness' `install_model` does not reach that code: ASSUMED, most likely) or they gate the B3 and Bass chains off until another event. This is the open item.

What the boot log therefore changes:

- **Dry gain `0xB44C0`:** the boot sends 1.0, and the B3 chain then sounds without the poke the earlier notes needed (the "silent, dry gain 0" row is gone). OQ16 item "ARM writer of `0xB44C0`" is answered.
- **Bass chord:** the level is 9 x higher and the tap side lines of [organ-farfisa-bass](organ-farfisa-bass.md) s4.2 (-14 dB at +/- 7 Hz) are absent with the boot's taps and install; the earlier lines were measured with a state the real device never has. Any comparison of a Rust Bass or B3 path against the oracle that used the old prefix needs the new state (and the open `0xB2ECF` / `0xB2FF0` question settled first).
- **Farfisa and Vox:** unchanged (silent): their weights are key-down words.
- **Vibrato gate `0xB44AE`:** 0 in the boot state (default program), as established in [arm-bulk-tables](arm-bulk-tables.md).

Not done in this pass: the line-by-line error table of the Rust paths against the new oracle state, and any change to the ported tail table, weights or FX words. They were not changed because the derived data (tail gains, keydown weights) depend on the open `0xB2ECF` / `0xB2FF0` question; defaults and the render hash are untouched (no Rust or default changes in this PR).


## Implications

- The oracle equals the ARM boot state for PNO, ORG and FX cores; SY / SMP and the L2 table are the open parts of "every core".
- `0xB44C0 = 1.0` now has a firmware writer (the boot): the OQ16 ASSUMED tag can become VERIFIED.
- The Farfisa weights are key-down words, so the silent Farfisa pass is a key-down capture problem, not a boot-state one.
- Replaying the whole boot log changes the Bass and B3 chords materially; two boot words (`DM 0xB2ECF`, `0xB2FF0`) must be explained before the ported tail table and weights are re-derived.

## Follow-ups

1. Find the reader of the L2 table (it is not read by the ORG core 2 pass; check the core 1 / audio IO sections).
2. Capture the Farfisa key-down words `0x95FFC..` (OQ19) and re-run the Farfisa chord.
3. Add SY / SMP to the equality check once a pass runner for those images exists.
4. Find what the real model install does with `DM 0xB2ECF` / `0xB2FF0` (sender `0xC022E3E8`), then re-measure the B3 tail, the Bass lines and the Rust paths line by line against the new state.

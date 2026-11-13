# B3 processor tail `0x121E2B`: the frequency response seen from the ring words

> **Superseded (2026-11-13):** the table, chord and hardware numbers below were measured on an oracle state that lacked the ARM boot messages. The tail is now the table of [b3-tail-bootstate](b3-tail-bootstate.md) (7 to 26 dB higher, smooth); this note keeps the method and the earlier results for the record.

**Checked:** 2026-11-13
**Sources (local, never committed):** the PNO image `SRAM/core1_sram.bin`, `sharc-pass` and `sharc-disasm` (`crates/nord-sharc`; listing of `0x121E2B..0x122293`, 453 VISA instructions), `tools/organ_fb_pass.py script --model 0` (ARM engine constructor, model install and key-down of the keys), [organ-keydown-weights](organ-keydown-weights.md), [keydown-zones](keydown-zones.md), [b3-generator](b3-generator.md). New derived table: [b3-tail-gain.csv](data/b3-tail-gain.csv) (`tools/organ_b3_proc.py`). No firmware bytes are recorded.
**Question:** why was the Rust B3 dry sum 12.7 to 31.8 dB above the oracle chord ([organ-keydown-weights](organ-keydown-weights.md) section 2), and can the tail be modelled well enough to make `B3Wheels` the B3 default?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Answer in short

| Item | Result | Tag |
|---|---|---|
| Source of the 12.7 to 31.8 dB | two causes: (a) **8.2 dB** came from the comparison script: the chord replay `organ_fb_pass.py script --keys` sends the bank engine's direct key-down (weights `0.0125` for C4 8') and not the MIDI key path (`0.0329`, the captured [keydown-b3-c4e4g4.script](data/keydown-b3-c4e4g4.script) words); with the real key-down words the offset is gone; (b) **0.4 to 24 dB** is the tail, a smooth low-pass shaped response | VERIFIED (run) |
| The tail is linear and time invariant | single-wheel runs: halving `W` leaves the gain unchanged (`-4.47 / -4.48` dB at 131 Hz, `-24.40 / -24.17` dB at 1,318 Hz); eight wheels active gives the gain of single wheels within 0.7 dB; lines of different notes and harmonics at one frequency agree within 0.1 dB (131 Hz: wheel 36 h2, wheel 48 h1 `-4.48 / -4.51 / -4.47` dB) | VERIFIED (run) |
| Shape of `G(f)` | 0 dB (+0.4) at 65 Hz, -4.5 dB at 131 Hz, -8.2 at 196 Hz, -12 at 262 Hz, -17.3 at 392 Hz, -21.3 at 494 Hz, -26.3 at 830 Hz, flat about -24 to -25.5 dB from 880 Hz to 1.6 kHz, then rising to -16.8 dB at 5 kHz and -12.7 dB at 7.5 kHz. A dry path plus a filtered branch (a deep dip near 800 Hz and 1.6 kHz), not a single low-pass | VERIFIED (83 points, 65 Hz to 7.9 kHz, lines from 27 oracle runs) |
| Dry gain `DM(0xB44C0)` | the output is `s * g + H(s)`: `g = 2.0` gives 1.63 x the peak of `g = 1.0` (a pure gain would give 2.0): the second branch is of the size of the dry one; `g = 1.0` reproduces the table; the ARM writer was not found (the constructor and the key-down never write it) | VERIFIED (run), writer open |
| Rust model | `b3_tail.rs`: `G(f)` as a 83-point dB table; `B3Wheels` scales every harmonic of every wheel table by `G(h f0)` (amplitudes right, the phase of the response is not modelled) | EMPIRICAL |
| Result against the real key-down chord | C4 E4 G4 at 8888: 10 of 13 line frequencies above `5e-4` within 1 dB, rms 0.75 dB, max 1.5 dB (was +12.7 to +31.8 dB) | VERIFIED (run) |
| Default | **stays opt-in** (section 4): 2 further chords reach 8 of 13 and 6 of 11 lines within 1 dB | decision |

## 2. What the processor does (listing, not transcribed)

The `sharc-disasm` listing of the processor shows the 80-tap dot product `s = sum W ring` (a `DO` loop of 19 iterations x 4 MACs), then repeated SIMD multiply-add blocks with delay lines in `DM` (`0xB46E6`, `0xC0202`) and a parallel MAC structure whose coefficient index words are read from `DM(I2 + 0x10..)`. Its data come from the engine constructor `0xC002187C` (15,289 words) and are not written by any key-down. A word-for-word transcription was not attempted (453 instructions of paired SIMD, not feasible in the time box): the chain is linear in the ring words, so its transfer function is measured instead (section 3). Not found: the ARM writer of `DM 0xB44C0`; the feedback gains 0.99 / 0.9977 named in the brief were not read from the listing.

## 3. Method

1. `organ_fb_pass.py script --model 0 --keys K1,K2,K3` for nine key triples that together reach 70 of the 77 ring notes 36..112 (27 keys, items `key - 12`, `key`, `key + 7`, `key + 12`); one script is 23,255 words (constructor, install, key-down).
2. `organ_b3_proc.py subset` keeps `W` of a clean subset of the active ring notes (no line of one note within 3 bins of a line of another, harmonics 1..4) and zeroes the other `W` words; each subset is one `sharc-pass` run of 12,000 passes with `DM(0xB44C0) = 1.0`.
3. `organ_b3_proc.py measure`: amplitude of every line (Hann window, 16,384 frames after 4,000, search over 0.4 bin) divided by the predicted line `A W h_k` (`A`, `W` the final ARM words, `h_k` the VERIFIED wheel-table harmonic amplitudes): the gain in dB at the frequency of the line (fundamental and harmonics 2..6 of the wheels: 248 lines).
4. `organ_b3_proc.py table`: lines weaker than `2e-5` dropped, half-semitone bins, outliers (more than 1.5 dB from the line between the neighbours) rejected: [b3-tail-gain.csv](data/b3-tail-gain.csv).

```bash
python tools/organ_fb_pass.py script CODE SRAM arm_t0.script --model 0 --keys 48,65,96 --cache settled.snap
python tools/organ_b3_proc.py subset arm_t0.script sub0 --frames 16384
python tools/pno_full_pass.py run SRAM/core1_sram.bin --exe target/release/sharc-pass \
    --script arm_t0.script --script sub0.override.script --script dry_gain.script --passes 12000 --out sub0
python tools/organ_b3_proc.py measure arm_t0.script sub0.override.json sub0.f32.npy > lines0.csv
python tools/organ_b3_proc.py table docs/research/data/b3-tail-gain.csv lines0.csv lines1.csv ...
```

## 4. Chords against the oracle (real key-down words, absolute dB)

Oracle: constructor + install (`organ_fb_pass.py --model 0`), all `W` zeroed, then the key-down lines of the captured table for each key (`keydown-organ-keys.csv`, model 0, velocity 100), `DM(0xB44C0) = 1.0`. Rust: `organ_ref --mode b3 --rate 43402.34375`; `python tools/organ_partials.py b3`. Line frequencies above `5e-4` (unique), deviation Rust minus oracle:

| Chord | Drawbars | Lines | Within 1 dB | rms | Outside 1 dB |
|---|---|---|---|---|---|
| C4 E4 G4 (60, 64, 67; key 67 is not captured) | 888800000 | 13 | 10 | 0.75 dB | 392 Hz (G4 8', +1.5), 784 Hz (+1.3, +1.3) |
| C4 E4 A5 (60, 64, 81) | 888800000 | 13 | 8 | 1.33 dB | 392 Hz -1.3, 440 -1.6, 494 -2.1, 523 -2.2, 659 -2.7 |
| C3 C#4 E6 (48, 61, 88) | 888000000 | 11 | 6 | 2.3 dB | 131 Hz +6.3 (wheel 36 h2 on wheel 48), 139 +2.6, 415 -1.4, 659 -2.6 |
| C2 E4 A6 (36, 64, 93) | 008000000 (8' only) | 2 | 2 | 0.53 dB | none |

- The tail itself is verified (all lines of the first row not involving a W or A uncertainty are within 0.5 dB, the single-wheel runs agree to 0.1 dB).
- The remaining errors are **not the tail**: (a) lines where a harmonic of a lower wheel lands on a higher wheel's fundamental (131 Hz, wheel 36 h2 and wheel 48) depend on the relative start phase of the two free-running wheels, which is not modelled (the same effect as the Vox chord, [organ-keydown-weights](organ-keydown-weights.md) section 3); (b) lines of 5 1/3' and 4' items (494, 523, 659 Hz) sit 2 to 2.7 dB under the oracle: their `A` words are overwritten by later writes of the same burst (the percussion-style item writes a lower `A`, [b3-generator](b3-generator.md) section 5) and `A` is kept as the first write; (c) key 67 is not a captured key (`W` interpolated).
- Drawbar states: only footages up in the captured default program have `W`; the row `008000000` is checked by zeroing the other footages' words (the real drawbar edit ids were not swept).

## 5. Hardware clip (`tools/emu_vs_hw.py --only organ_b3 --confirmed-only`)

Same clip, notes and method as [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md) (emulator minus hardware, dB re RMS):

| Variant | sub | low | mid 0.4-1.5k | pres | brill | centroid | RMS | width hw/emu | score |
|---|---|---|---|---|---|---|---|---|---|
| default (Legacy) | +17.5 | -0.2 | +0.3 | -5.7 | +0.5 | -208 | +14.9 | 0.48/0.25 | 4.83 |
| b3wheels, weights only ([organ-keydown-weights](organ-keydown-weights.md)) | +13.9 | -0.2 | +0.4 | +1.3 | +8.0 | -78 | +3.1 | 0.48/0.44 | 4.76 |
| **b3wheels with the tail** | +16.8 | -3.0 | -11.9 | +1.2 | +7.4 | -91 | -2.6 | 0.48/0.64 | 8.07 |

Peak 0.0 dBFS, 0.021 % of samples at full scale, crest 23.1 dB (hardware 17.7 dB). The tail that matches the oracle chord leaves the 0.4 to 1.5 kHz band 11.9 dB under the hardware recording: the performance's mid band is not reproduced by the chain with the constructor's tables as loaded (the oracle chain with default tables is a dark filter, 24 dB down above 600 Hz), so either the recorded program drives the line box with other tables (vibrato / chorus, rotary) or the Rust rotary and scanner stages already add this part. The hardware comparison therefore argues against flipping the default; no table was tuned to it.

## Implications

- The 12.7 to 31.8 dB gap of [organ-keydown-weights](organ-keydown-weights.md) was a tail (0.4 to 24 dB, now modelled as a frequency response) plus an 8.2 dB script artefact (direct key-down words instead of the MIDI path).
- The B3 chain is now within 1 dB on about 75 % of the strong lines of an unseen chord and within 0.5 dB where `A`, `W` and the phase are not in question. Making it the default needs the multi-write `A` words per key (EMPIRICAL now), a drawbar sweep and the oscillator start phase.

## Follow-ups

1. Per key `A` after the whole key-down burst (last write per wheel, per chord), then `W` of the five footages missing from the capture (drawbars `888888888`).
2. The phase response of the tail (impulse response from a single wheel with the oracle's `--trace`), to place coincident lines (131 Hz) and to model the transients.
3. ARM writer of `DM(0xB44C0)`; start phases of the 91 wheel oscillators in the image.

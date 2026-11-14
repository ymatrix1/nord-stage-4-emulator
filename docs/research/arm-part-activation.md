# ARM part activation: what the harness runs, what it waits for, and the Leslie rotor state

- **Checked:** 2026-11-14
- **Sources (local, never committed):** the settled harness (`tools/fx_clients.py`, `tools/arm_edit_capture.py`, `tools/arm_steady.py`, `unicorn`), the full boot log (`tools/arm_bulk_tables.py capture`, `boot_log.jsonl`, phases `init`, `boot`, `settle 1..7`), the `capstone` listing of `bank0_CODE@0..0x279000`. Builds on [fx-contexts](fx-contexts.md) s7, [fx-leslie](fx-leslie.md), [fx-reverb-leslie](fx-reverb-leslie.md).
- **Question:** why do the module words of the effects and the Leslie (cabinet gain, delays, drive, speed, reverb pointers) look "never reprogrammed", and where is the consumer of the part command list `[part + 0xC8]`?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is there a polled command queue at `[part + 0xC8]`? | **No.** In the settled harness the words at `part + 0xC8 / 0xCC` (`0x9115B450`, `0x9115B460`, organ part `0x911528C0`, shared by FX units 3 and 9) never change when `kRvbEnable` or `kRvbType` is applied and no code reads them in 100 ticks. The enable handler (`0xC010E55C`) stores `-1` in a record (`0xC0220B6C` finds it, `str -1, [r0 + 0x20]` at `0xC010E564`) and calls `0xC0220344 (obj = [part + 0x8C], 1, ...)`, a **sorted insert into an array of edit records** (capacity at `+0xC`, entries from `+0x10`), not a queue with a consumer task | VERIFIED (memory hooks, listing) |
| Does the harness run module activation at all? | **Yes, for the loaded default program.** The boot log has the activation words: the Leslie cabinet gain `c[-30]` (`0xE3580 = 0x3E4E7C20`, `0xE3188 = 0x3C615180`) and delays (`0xE359C`, `0xE31A4`) from `0xC0204FA4` / `0xC0204DD8` in `settle 2`, the reverb words `0x9500A..` and `0x95013 = 0x116` (type 7) in `settle 2`, the chain pointers `0xB0481..` from `0xC021D02C` in `settle 2`, the rotor speed words from `0xC020D7DC` in `settle 1, 4, 7`. So the activation path that sends the words of the **default program's** effects runs; what the harness does not do is run it for a state the user later changes (`kRvbEnable = 1` after boot adds only the type words, no instance pointers, in 420 ticks) | VERIFIED (boot log) |
| Why does the rotor never change speed? | The rotor update (`0xC0209xxx`, called from the periodic update every ~136 harness ticks) first **reads the rotor phase back from the DSP** (`0xC020D41C` reads `DM(0xB2F23)` through the SPI master `0xC0066E64`, which the harness stubs to return 0) and compares it with `0.003`; with a DSP that always answers 0 the brake / stop-angle branch never settles. The speed words themselves follow the state block of s2 | VERIFIED (listing, call log), consequence ASSUMED |
| Where are slow, fast and the ramp? | In the rotor state block of the organ part (doubles, s2): **horn 0.80 Hz slow, 7.0 Hz fast; rotor 0.73 Hz slow, 6.4 Hz fast**; per update the current speed moves by **+0.06** (horn) / **+0.01** (rotor) up and **-0.06** / **-0.0218** down toward the target. The words follow `word = int(hz * 8388607 / 11025) << 8` | VERIFIED (state block, words `0x26000`, `0x22B00`, ramp words `0x28E00`, `0x23300` reproduced) |
| Does the +48 dB gap go away? | **No.** The activated words equal the boot words (same run), and the cabinet gain, drive and mic words are the ones the model was verified with; the hand-over stays open ([fx-leslie](fx-leslie.md) s8) | VERIFIED (negative) |
| Reverb instance words for types 2..11 | **Not obtained.** Enabling the reverb after boot sends only the 64 type words per type (`fx-type-words.csv`); the instance pointers of `0xB0CAE` are not in the boot log (that context is zero in the image) and no enable path of the harness sends them. The consumer that does is not the part list: it is the part of `0xC010E55C` ff. that installs a chain module through the edit-record array; not run | open |

## 2. The rotor state block (doubles at `0x9115D2B0` in the settled harness)

| Offset | Value | Meaning |
|---|---|---|
| `+0x08`, `+0x10` | 0.8, 0.8 | horn current and target speed (Hz) |
| `+0x18`, `+0x20`, `+0x28`, ... | 0.06, 1.0, -0.06, 1.0, -0.06, 1.0 | horn ramp: step up, limit, step down |
| `+0x48`, `+0x50` | 0.8, 7.0 | horn slow and fast speed (Hz) |
| `+0x58` | 0.2876 | coefficient (not identified) |
| `+0x70`, `+0x78` | 0.73, 0.73 | rotor current and target speed |
| `+0x80`, `+0x90`, `+0xA0` | 0.01, -0.0218, -0.0218 | rotor ramp steps |
| `+0xB0`, `+0xB8` | 0.73, 6.4 | rotor slow and fast speed |

`tools/arm_leslie_speeds.py` reads the block and computes the words; with the speeds above the **fast words** are `0xB2F26 = 0x14CE00` (7.0 Hz), `0xB2F25 = 0x130500` (6.4 Hz) and `0xB2FA1 = int(0.5748 * 7.0 * 8388607 / 11025) << 8` (EMPIRICAL factor: it fits `0x15D00` at 0.80 Hz and `0x17800` at 0.86 Hz, interval [0.57456, 0.57502]). The ramp was observed once per ~136 harness ticks (the periodic update): setting the targets to 7.0 and 6.4 moved the words to `0x28E00` and `0x23300` (0.86, 0.74 Hz) at tick 136. The time per update in seconds depends on the real timer period, which the harness does not model, so the Rust wiring keeps the engine's ramp time constants and takes only the speed endpoints from the firmware.

Hz at the DSP frame rate: the words mean `(word >> 8) * 256 / 2^32 * (fs / 2)` with `fs = 43,402.34 Hz`, so `0x26000` is 0.786 Hz and `0x22B00` 0.718 Hz (nominal 0.80 and 0.73: the ARM assumes 44.1 kHz).

**B3 clip with the ARM speed endpoints** (`organ_b3`, `--tone-source b3wheels --firmware-routing --b3-tail on --organ-level firmware --leslie firmware`, slow speed, so the words equal the earlier run): sub +6.5, low -3.7, mid +4.1, presence -7.8, brilliance +0.4 dB re RMS, score 4.51; with `--leslie-gain-db 48` the RMS gap is -0.2 dB (it was -8.2 dB at +40 dB), so a gain of about 48 dB matches the hardware level; presence is still not within 3 dB.

## 3. What was tried

| Experiment | Result |
|---|---|
| Memory hooks on `part + 0xC8..0xD3` during 40 idle ticks and after `kRvbEnable` / `kRvbType` | no read, no write |
| Write hook on the heap during the dispatch of `kRvbEnable` | the `-1` record store (`0xC010E564`), the edit-record insert (`0xC0220344`), nothing else |
| Call log of `0xC020D68C` over 250 ticks | two calls (horn 0.8, rotor 0.73), flag in `r1`; the third word `0xB2FA1` is sent with them |
| Targets 7.0 / 6.4 written into the block, 400 ticks | one update at tick 136: speeds 0.86 / 0.74, words `0x28E00`, `0x17800`, `0x23300` |
| `kRvbEnable = 1`, `kRvbType = 3`, 420 ticks | type words only (`0x9500A..`, `0xB0F0D..`, `0xB1C26..`), no pointer or length words |

## 4. Is the -48 dB a scale artifact? (2026-11-14)

Hypothesis (coordinator): the needed +48.2 dB is `2^8`, like the `<< 8` of the speed words, so a 24-bit-in-32 word format mismatch. Checked:

| Check | Result |
|---|---|
| ARM conversion of the cabinet taps (`0xC0204C34..58`, `0xC0204D4C..6C`, sender `0xC0204E64`) | `clz`, normalise, mantissa `x >> 8`, exponent `0x87 - clz`: `float = int * 2^-23`. The taps `0x967FE..` (0.005 to 0.023) are Q23 fractions as intended; a Q31 source would make them 256 times **smaller** in the table, not the taps in the DSP too small. No factor 256 on the data path |
| Constants of the processor `0x1256AD` | no `256.0`, `2^-8`, `2^8` immediate; the cabinet routine scales by `2^-7` in (`0x3C000000`) and `128` out (`0x43000000`), net 1 |
| Where the level goes (trace, noise, 4,000 calls) | rotor stage about -8 dB, cabinet gain `c[-30]` 0.2016 (-14 dB) / 0.01375 (-37 dB), FIR taps of 0.005..0.023 over 370 + 32 taps (noise gain about -14 dB): the sum is the measured -50 dB. Every factor is a stored coefficient, none is a format shift |
| Whole pass, real position | `sharc-pass` with the Pipe chord (`pno-pass-install-pipe1`, `pno-pass-keys-c4e4g4`, level word `0x95B42`) gives a lane rms of 0.0173 (peak 0.019). `0x1256AD` was inserted as the chain record at `0xC2E4A` (routine `0xC2E2A`, state `0xC2E2C = 0xB2F2A`, gain word `+0.5`): it is called (`--calls`), but the chain pair `0xC2E06` stays zero for the organ, so the lane is unchanged (rms 0.017343 with and without). The chain is not the organ path of this pass; the processor's real feed (the organ lane copy) was not found, so the ratio Leslie output to dry organ at the same point could not be measured |
| Level of the B3 clip with a +48 dB gain | RMS gap -0.2 dB, crest 17.8 dB (hardware 18.6), but sub +6.5, mid +4.1, presence -7.8, brilliance +0.4 dB: the level is matched, **the criteria (sub, mid, presence within 3 dB) are not met**; a scale error could not repair them since they are re RMS |

Conclusion: no format mismatch was found; the +48 dB gap stays an unexplained gain that the hardware makes up somewhere after the processor. Not done: the DSP read-back (`0xC0066E64` answered from the oracle's `DM(0xB2F23)`).

## Implications

- The "consumer of the part command list" does not exist in the form assumed: activation is state driven (the default program's words are sent in `settle 2`); the module words seen at boot are the real ones for that program.
- A DSP in the loop (answering the read-backs from the oracle's DM) is the missing piece for the rotor brake logic and probably for any activation that waits for a DSP state; the stub `0xC0066E64` returns 0.

## Follow-ups

1. Answer `0xC0066E64` reads from a DSP image state (the phase words) and re-run the rotor update through a stop and a start.
2. Find the module install call for the reverb (the edit-record array walker after `0xC0220344`) and run it with `kRvbEnable = 1` and each type to get the instance pointer words for types 2..11.

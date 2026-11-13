# FX clients and context words: where the effect handlers live and what they write

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin`, `bank0_SRAM.bin` (OS v1.66, ARM base `0xC0000000`), the `unicorn` harness (`tools/arm_steady.py`, `tools/arm_edit_capture.py`, `tools/arm_boot.py`), `capstone` listings. Derived data committed (addresses, indices and message words only): [data/fx-units.csv](data/fx-units.csv), [data/fx-type-modules.csv](data/fx-type-modules.csv), [data/fx-type-words.csv](data/fx-type-words.csv), [data/fx-param-fits.csv](data/fx-param-fits.csv), [data/fx-delay-time.csv](data/fx-delay-time.csv). Builds on [fx-type-map](fx-type-map.md), [fx-routine-models](fx-routine-models.md), [fx-chain-install](fx-chain-install.md), [arm-organ-messages](arm-organ-messages.md) s10.
- **Question:** where are the effect clients (Fx1, Fx2, Reverb, Delay, Compressor, Amp, Leslie) built, which module and routine does each type value select, and which context words of the modelled routines do the edit ids write, in which units?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = shape of code or data, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Are the effect clients missing in the steady state? | **No.** The boot builds all of them: 6 FX *parts* (1 organ, 2 piano, 3 synth) with **12 FX unit objects** (`L = 0..11`, two per part). They are not registered in the dispatcher's client list (10 entries), which is why [arm-organ-messages](arm-organ-messages.md) s10 found no value cell for Fx2, Dly, Rvb, Cmp and Leslie. Nothing has to be constructed by hand; `tools/fx_clients.py` finds the units and drives `0xC00CC4F0(unit, 0, id, &value)` through them. | VERIFIED (hooks on `0xC01177A8` / `0xC01155EC` during `boot_to_main_loop`) |
| Does the type value select a routine? | **Yes, a module, in pairs.** Fx1 type `2k, 2k+1` and Fx2 type `2k, 2k+1` (types 4, 5 of Fx2 are single) share one module (descriptor list of [fx-type-map](fx-type-map.md) s2) and differ by data; the delay always owns the delay module and the feedback-effect type adds a second module. Table in section 4. This corrects [fx-type-map](fx-type-map.md) s5 ("no type to routine table"): the table exists as the **set of context rows the type handler writes**, not as a pointer array. | VERIFIED (58 type values on the 3 unit kinds, `fx-type-modules.csv`) |
| Context words | A DM word `w` of the state row `b` is the model word `c[w - b - 0x20]` (section 3). 746 non-constant series of (id, value) to context word are in `fx-param-fits.csv`. | VERIFIED (field positions agree with the delay model on 12 words) |
| Value scale | Continuous ids (range 12 or more) are read as **Q16.16**: the cell must hold `v << 16`. With the raw value every amount word is `65536` times too small. | VERIFIED (section 2) |
| Closed forms found | Delay mix and feedback, compressor threshold and makeup, all-pass rate and cross-fade, Amp drive (`tools/fx_forms.py`, tested against the committed series). | VERIFIED |
| Names | **The "table LFO" `0x1228CF` is the Amp drive stage** (its phase word is `kAmpDrive / 127`), and the "resonant filter" `0x1243F4 / 0x124497` is the Amp **tone stack** (treble, mid, bass words). The LFO role of [fx-routine-models](fx-routine-models.md) s4 is not supported. | EMPIRICAL (the id to word join) |
| Not closed | Enable ids send nothing in isolation; `kCmpResponse`, `kDlyMode`, `kDlyFbkPingPong` do not change a word; the delay time lands in words the modelled routine does not read; reverb type tables are outside the state rows. Section 7. | open |

## 2. Where the FX clients are built (VERIFIED run)

```bash
# hooks on 0xC00FEDD0 / 0xC00FEE1C / 0xC00FEE6C (call sites), 0xC01177A8 (part), 0xC01155EC (unit)
# during arm_steady.boot_to_main_loop, 433 M instructions, no extra stub
```

The application constructor (the large function around `0xC00FC9A0..0xC00FEF00`) builds, in order: 2 organ layers (`0xC0019BAC`), 2 piano layers (`0xC00194B8`), 3 synth layers (`0xC0015B48`), 3 ext layers (`0xC00166EC`) and then the FX parts with `0xC01177A8 (part, index, client, mode)`:

| Call site | Parts | Mode | Client id (`r2`) | Units built by the part (`0xC01155EC`) |
|---|---|---|---|---|
| `0xC00FEDD0` | 1 | 0 organ | 9 | L3, L9 |
| `0xC00FEE1C` | 2 | 1 piano | 0xB, 0xC | L4, L10 and L5, L11 |
| `0xC00FEE6C` | 3 | 2 synth | 0xD, 0xE, 0xF | L0, L6 and L1, L7 and L2, L8 |

The part constructor is `0xC01177AC` (entry `0xC01177A8` loads a literal first); it stores the part vtable `0xC02BB930`, subscribes the Edt ids with `0xC00D57F0 (part, id)` and builds two unit objects. Each unit (vtable `0xC02BAD68`, index `L` at `+0x10`, part at `+0x14`) is itself a **store** (its vtable slot 8 is the store setter `0xC00D051C`) with a value table: 36 to 48 of the `Edt_Effects_*`, `Edt_Com_kEffectsRvbGlobal`, section-mode and (organ unit) Leslie ids. The 19 module objects of [fx-type-map](fx-type-map.md) s3 sit at `unit + 0x208 .. 0x250`. `fx-units.csv` lists unit, kind and the state-table row of every unit.

The state-table row of a unit (`module + 4`) is allocated per `L`: 12-row tables use rows `0, 1, 2, 3, 4` for L0, L1, L2, L4, L5 (core 1), rows `5..9` for L6, L7, L8, L10, L11 (core 2) and rows `10, 11` for the organ units L3, L9 (target 2, DM `0xB28xx`); 24-row tables use `0..4`, `10..14`, `20, 21`.

**Q16 scale** (VERIFIED, `Edt_Effects_kFx1Amt`, Fx1 type 4):

| Cell written | Words `0xE0A49`, `0xE0A4A` | Reading |
|---|---|---|
| `64` | `7.69e-6`, `0.99999` | `64 / 127 / 65536` |
| `64 << 16` | `0.503937`, `0.496063` | `64 / 127` and `1 - 64 / 127` |
| `127 << 16` | `1.0`, `0.0` | full wet |

The registry default cell reads `0x40` raw in the settled snapshot (`kFx1Amt` default 64), so the value cells seen by `value_cell` are not yet in handler units when the firmware has not run its setter; `tools/fx_clients.py::cell_word` writes `v << 16` for ids whose range is 12 or more and `v` for enums and flags.

Why the 82 silent ids of [arm-organ-messages](arm-organ-messages.md) s10.3 were silent: the sweep only visited the 10 dispatcher clients. Through the units: `python tools/fx_clients.py CODE.bin SRAM.bin --cache settled.snap --units 3 --slots capture.csv` (20 to 30 min per unit with three runs in parallel). 7 slots, up to 12 types each and up to 9 ids per slot; each value is applied in ascending order from the state after the type switch.

## 3. From a DM word to a context word (VERIFIED)

State tables are the rows `{target, base, size, tag}` of [fx-type-map](fx-type-map.md) s2 (`tools/fx_contexts.py::blocks`: 42 state tables). `target` 0 and 2 mean core 1 (`base = 0x28000000 + 4 w`), `target` 1 means core 2 (`0x28800000 + 4 w`). A row has `(size >> 4)` words (stride of the table; delay 70, LFO 54, compressor 25). **The context word is `c[o]` with `o = w - w0 - 0x20`**: the routines address `c[-0x20 .. n]` and `I1` points at `w0 + 0x20`.

Check against the delay model ([fx-routine-models](fx-routine-models.md) s3), 12 words of the delay row written by `kDlyMix`, `kDlyFbkAmt` and the delay clock:

| ARM wrote (ctx) | Model field | Reading |
|---|---|---|
| `c[4]` | dry gain target `g_d` (`c[4]`) | matches |
| `c[-0x1B]` | input gain target `g_in` (`c[-0x1B]`) | matches |
| `c[-0x1A], c[-0x19]` | matrix targets `m_ll`, `m_rr` | matches |
| `c[-0x18], c[-0x17]` | `m_xl`, `m_xr` | matches |
| `c[-0x16], c[-0x15]` | `m_rl`, `m_lr` | matches |
| `c[-0xB], c[-0xA]` | feedback all-pass targets `a1`, `a2` (`0.99855`, `0.30972`) | matches |
| `c[0x24], c[0x25]` | ratio clamp `hi`, `lo` (`1.5`, `0.25`) | matches |

Every one of these positions is a field of the model, so the bias `0x20` is confirmed by an independent source. The three words the clock writes, `c[0x15]`, `c[0x16]` and `c[0x19]`, are **not** read by `0x1206B0` (section 5.1).

```bash
python tools/fx_contexts.py CODE.bin SRAM.bin capture.csv ... --out joined.csv --types types.csv --fits fits.csv
```

## 4. Type value to module (VERIFIED; `data/fx-type-modules.csv`)

Routines are the core 2 pcs of [fx-chain-install](fx-chain-install.md) s2 (core 1 copies sit at fixed offsets). "words" is the number of context words the type switch writes to the module.

| Slot | Type | Module (list#entry) | Core 2 routines | Note |
|---|---|---|---|---|
| Fx1 | 0, 1 | `0xC02BAF54#1` (run 1) | `0x122D93` | |
| Fx1 | 2, 3 | `0xC02BB030#1` (run 32) | `0x122C39` (all-pass, modelled) | |
| Fx1 | 4, 5 | `0xC02BB10C#1` (run 23) | `0x1239D0` | cross-fade `v / 127` |
| Fx1 | 6, 7 | `0xC0279290#0` (run 11) | `0x123E9B` | |
| Fx1 | 8, 9 | only `0xC04414C4#0` (waveshaper) | `0x123DD2` | |
| Fx1 | 10, 11 | `0xC02BB5E4#2` (runs 42, 22) | `0x124374`, `0x122D2C` | |
| Fx1 | all | `0xC04414C4#0` | `0x123DD2` | 2 words in every type (output stage, ASSUMED) |
| Fx2 | 0 | `0xC02BB1E8#1` (run 20, phaser) | `0x12379B` | |
| Fx2 | 1 | `0xC02BB2C4#1` (run 19) | `0x1238FC` | |
| Fx2 | 2, 3 | `0xC02BB3A0#2` (runs 17, 18) | `0x12348D`, `0x123539` | |
| Fx2 | 4 | `0xC0443740#0` (runs 2, 5) | `0x122E24`, `0x122F55` | |
| Fx2 | 5 | `0xC0443694#0` (runs 3, 4) | `0x122F77`, `0x1230AC` | |
| Fx2 | 6, 7 | `0xC0279128#2` (runs 35, 36) | `0x123C84`, `0x123DB4` | |
| Fx2 | 8, 9 | `0xC0441558#4` (runs 13, 14) | `0x123A7B`, `0x123BD0` | not the compressor row (`#0`) |
| Fx2 | 10, 11 | `0xC0278FAC#2` (runs 24, 25) | `0x123557`, `0x12377D` | heavy biquad bank |
| Dly | all | `0xC0279098#0` (runs 7, 8) + `0xC0443740#0` | `0x1206B0` family | `0xC0443740#0` holds the feedback fx state |
| Dly (feedback fx) | 2, 3 | + `0xC0441558#4` | `0x123A7B`, `0x123BD0` | |
| Dly (feedback fx) | 4, 5 | + `0xC0441B70#1` (run 34) | `0x123BEE` | |
| Dly (feedback fx) | 7 | + `0xC0443694#0` | `0x122F77` | |
| Dly (feedback fx) | 9 | + `0xC0441A18#2` (runs 27, 28) | `0x1230CA` | |
| Dly (feedback fx) | 10, 11 | + `0xC0441AC4#2` (runs 9, 10) | `0x12314A`, `0x12329B`, `0x123466` | multi-tap packed delay |
| Rvb | 0, 1 | `0xC02BB54C#0` (run 41), 39 words | `0x12417E`, `0x12435C` | |
| Rvb | 2..5, 7..11 (6 is the default and sends nothing) | **no state row**: 54 words at DM `0x9500A .. 0xB1C2C` (a coefficient table) plus the routing set of `0xC02BB44C#0` | | section 7 |
| Amp | 1..6 | `0xC0442A94#0..2` (FIR + IIR), `0xC04438D8#0` (drive), pan `0xC02BB10C`, `0xC02BB2C4`; piano and synth add `0xC02BB54C`, `0xC04437B0`, `0xC04438D8#2` | `0x122A1D`, `0x122B1D` | 130 FIR words ([fx-type-map](fx-type-map.md) s4) |
| Amp | 7, 8, 9 | `0xC02BB10C#1`, `0xC02BB2C4#1`; piano and synth add `0xC04437B0#0`, `0xC04438D8#2`, `0xC044398C#0` | | link words and a few coefficients |

The even / odd pairing (two types per module) is the same pattern as the Amp unit: one module, **data decides the variant**. A Rust `fx/` that keeps one stage per module and a parameter set per type matches the firmware.

## 5. Context words of the modelled routines

Closed forms are in `tools/fx_forms.py` and checked against `fx-param-fits.csv` by `tools/tests/test_fx_forms.py` (float32 words, error below `1e-5` of the series range). `v` is the UI value of the id.

### 5.1 Delay `0x1206B0` (module `0xC0279098#0`)

| Id | Context word | Closed form | Tag |
|---|---|---|---|
| `kDlyMix` (0..127) | `c[-0x1B]` (send level `g_in`) | `min(1, (v / 64)^2)` | VERIFIED words, EMPIRICAL role |
| `kDlyMix` | `c[4]` (dry `g_d`) | `min(1, (2 - v / 64)^2)` | same |
| `kDlyFbkAmt` (0..127) | `c[-0x1A]`, `c[-0x19]` (`m_ll`, `m_rr`) | `v / 127` | same |
| `kDlyFbkAmt`, feedback fx type 7 | `c[-0xB]` (`a1`) | `0.99567 - 1.0709e-4 v` (organ, piano), `0.99783 - 5.384e-5 v` (synth) | VERIFIED words |
| `kDlyRateClkDiv` (0..16383) | `c[0x15]` | float `61438` for `v <= 128`, then `59179 (256)`, `54252 (512)`, `46498 (1024)`, `32551 (2048)`, `23460 (4096)`, `17836 (8192)`, `6510 (12288)`, `868 (16383)`; table in `fx-delay-time.csv` | VERIFIED words |
| (constant on clock write) | `c[0x16] = 10850`, `c[0x19] = 30.199293` | | VERIFIED words |
| (default) | `c[0x24] = 1.5`, `c[0x25] = 0.25` (ratio clamp), `a1 = 0.998553`, `a2 = 0.309717` | | VERIFIED words |

**Decoded in [fx-delay-time](fx-delay-time.md):** `c[0x15]` is the delay tap in samples at 48 kHz, `c[0x15] = min(61438, floor(43.402339 k))` with `k` the fractional index of `v` in a 1501-entry threshold list (VERIFIED, 264 values), `c[0x16]` the previous tap, `c[0x19]` the cross-fade step (`-32768` then `+30.199`), and the echo of an input comes `trunc(c[0x15]) + 2` samples later (oracle, ring variant `0x120AB5`). `0x1206B0` and `0x1208AF` read `c[0x10..0x13]`, not these words; `0x120D15` is the ring clear helper. The delay time is wired behind `FirmwareFx::delay_time`; the full ring variant is not ported to `FwDelay`.

### 5.2 Drive stage `0x1228CF` (module `0xC04438D8#0`, Amp unit; "table LFO" of fx-routine-models s4)

| Id | Word | Closed form |
|---|---|---|
| `kAmpDrive` (0..127) | `c[-0x20]` (phase target, smoothed with 0.9908 per call) | `v / 127` (VERIFIED, all 10 Amp types, also `0xC0443A20#0 c[-0x20]` the second drive module) |

So the "phase" of s4 is the **drive amount** and the table pointers `c[-0x15]`, `c[-0x14]` are the drive curves; the 257-entry `-cos` table A of that section is a gain curve, not an oscillator. `c[0]` bound, the waveshaper bound `c[1]` and the two filter sections are the stage's voicing and are installed by the Amp type switch (the 25 words of `Amp, 1..6`, `fx-type-words`). [fx-routine-models](fx-routine-models.md) s4 keeps its equations (VERIFIED against the oracle); only the role name changes.

### 5.3 Tone stack `0x1243F4` / `0x124497` (module `0xC04438D8#2`; piano and synth units)

| Id | Words | Behaviour |
|---|---|---|
| `kAmpTreble` (-60..60) | `c[-0x1B]` (target `F`), `c[-0x1C]` (target `E`) | `F`: `-0.822 (-60)`, `-0.350 (-15)`, `0 (0)`, `0.530 (15)`, `1.340 (30)`, `2.579 (45)`, `4.474 (60)`; `E` flat above 0 (`-0.541`), `-0.899 (-60)` below: a high shelf whose boost is `F` |
| `kAmpBass` (-60..60) | `c[-0x20]` (target `A`) = the same curve as treble `F`; `c[-0x14]` (smoother `a`): `0.0144` above 0 up to `0.0782` at -60 | low shelf |
| `kAmpMid` (-60..60) | `c[-0x17]`, `c[-0x18]`, `c[-0x1F]` (`C`, `D`, `B`), `0xC044398C#0 c[-0x1C], c[-0x1D]` | mid band gain |
| `kAmpMidFreq` (0..127) | `c[-0x17]`, `c[-0x1F]` (`0.6999 ... 0.6995`, `0.0287 ... 0.0301` at 0..2), `0xC044398C#0 c[-0x20]` (`6.2e-5 ... 1.0`, exponential in `v`) | mid centre frequency |

Mapping to the model names of [fx-routine-models](fx-routine-models.md) s6 is EMPIRICAL (position only). Series samples: `fx-param-fits.csv` (slot Amp, type 7..9).

### 5.4 All-pass / phaser `0x122C39` (module `0xC02BB030#1`, Fx1 types 2 and 3)

| Id | Word | Closed form |
|---|---|---|
| `kFx1RateClkDiv` (0..127) | `c[-0x20]` (integer phase increment `RATE`) | `12800 + 16384 v` (integer word, VERIFIED, both types) |
| `kFx1Amt` type 2 | `c[-0x1A]` / `c[-0x19]` | `v / 254` and `1 - v / 254` (the word peaks at 0.5) |
| `kFx1Amt` type 3 | `c[-0x16]` / `c[-0x19]` | `v / 254` and `1 - v / 254` |

Reading (ASSUMED): the 32-bit counter advances by `RATE` each call of 2 frames x `c[4]` blocks, so the LFO frequency is `RATE * fs / (2 * c[4] * 2^32)`; with one block per call the range is `0.0715 Hz .. 11.7 Hz` at 48 kHz. The polynomial `c[-0x13..-0x11]`, the counter scale and the smoother weight `c[3]` are written by the type switch (`fx-type-words`).

### 5.5 Compressor gain computer `0x120630` (module `0xC0441558#0`)

| Id | Word | Closed form |
|---|---|---|
| `kCmpAmt` (0..127) | `c[-0x1B]` (right makeup target = threshold word, log2 level) | `-36 v / 127` |
| `kCmpAmt` | `c[-0x1C]` (makeup target `T`) | `1 + 5.72 (v / 127)^2` (1.0 .. 6.72) |
| `kCmpResponse` (0, 1) | attack pair `c[-0x20], c[-0x1F]` = `0.102071, 1.0`; release pair `c[-0x1E], c[-0x1D]` = `4.607e-4, 0.04` | **identical for both values** (the id does not change these words in any state tried) |

The pair names follow the model of [fx-type-map](fx-type-map.md) s7 (attack `a` at `-0x20`, release `r` at `-0x1E`) and are EMPIRICAL (position). So `kCmpAmt` is solved (threshold and makeup); `kCmpResponse` is not (section 7).

## 6. Leslie

Only `Edt_OrgCom_kEffectsLeslieDrv` reaches a state row: list `0xC02BB76C#0` words `3` and `7` of its row = `v / 127` (VERIFIED). The speed, stop and angle ids produce nothing in isolation (they act through the rotor state machine on the tick). The Leslie module has no code record in the ARM tables (one state row, no routine), so the rotary effect is not in the 100 FX code records.

## 7. Not decoded (open)

| Item | Evidence | Next step |
|---|---|---|
| Enable ids (`kFx1Enable`, `kFx2Enable`, `kAmpEnable`, `kCmpEnable`, `kDlyEnable`, `kRvbEnable`, `kLeslieEnable`) | no DSP message in 2 M instructions; the handler (`0xC010E774..`) queues a command in the part's list (`[part + 0xC8]`, node `{-1, counter}` of 8 bytes) | run the part's command consumer (the main loop does not reach it in the window) |
| `kCmpResponse`, `kDlyMode`, `kDlyFbkPingPong` | words identical for every value | read the handler's branch; probably gated by the `Edt_Com_k*Global` ids, which no store owns (`value_cell` is None in all 22 stores) |
| ~~Delay time words `c[0x15]`, `c[0x16]`, `c[0x19]`~~ | decoded: [fx-delay-time](fx-delay-time.md) | remaining: enable path (`c[0x14]`, ring words), tempo sync, loop period |
| Reverb | types 2..5 and 7..11 write 54 words at DM `0x9500A..` (outside the state rows), types 0 and 1 write module `0xC02BB54C#0` | join the `0x95xxx` table to the multi-tap module `0xC0441AC4`; `kRvbColor` value 2 rewrites the 54 table words |
| Organ units L3 / L9 differ from piano and synth in word counts of some types (for example Fx1 type 2: 3 against 5 words) | `fx-type-modules.csv` | compare the base state of the units |
| Other 1000 table-shaped series of `fx-param-fits.csv` | most are Amp type re-sends | fit with a wider family (piecewise exponential) |

## 8. Wiring plan for `crates/nord-core/src/fx/` (no Rust changed here)

The exported stages take the firmware's context words (`from_words`, `set_value(offset, f32)`), so the wiring is a table of `(slot, type) -> stage + word fills`:

1. **New `fx/fw_map.rs`** with `enum FwStage { Delay(FwDelay), Allpass(FwAllpass), Drive(FwLfo), Tone2(FwResonant2), Tone4(FwResonant4), Compressor(FwCompressor) }` and a function `fn set_param(&mut self, id: EdtId, v: f32)` that applies the closed forms of `tools/fx_forms.py` (port them as `const fn`s next to the fields; every constant tagged per [provenance-in-code](../../.agents/rules/provenance-in-code.md): words `VERIFIED` with the series file as source, roles `EMPIRICAL`).
2. **Compressor** (`MasterBus::set_fw_compressor`): fill `c[-0x1B] = -36 v / 127`, `c[-0x1C] = 1 + 5.72 (v/127)^2` from `kCmpAmt`; keep the attack pair `(0.102071, 1.0)` and release pair `(4.607e-4, 0.04)` as `VERIFIED` defaults. Closes the `kCmpAmt` item of [fx-type-map](fx-type-map.md) s7; `kCmpResponse` stays open.
3. **Amp**: `FwLfo` becomes the drive stage (`c[-0x20] = v / 127`) behind the cabinet FIR (`amp_eq.rs`), `FwResonant2/4` the tone stack (words from `fx-param-fits.csv`: a 13-point table per word, interpolate linearly in dB). Rename `FwLfo` to a drive-stage name in the same change.
4. **Fx1 types 2, 3 / Fx2 phaser** (`FwAllpass`): `RATE = 12800 + 16384 v`, wet `v / 254`, dry `1 - v / 254`; counter source is a per-call increment of the 2-frame block.
5. **Delay**: `FwDelay` takes `g_in`, `g_d`, `m_ll`, `m_rr`, `a1`, `a2` directly from `kDlyMix` / `kDlyFbkAmt`; the time law is wired as `FirmwareFx::delay_time` (`fw_delay_time.rs`, [fx-delay-time](fx-delay-time.md)); the ring variant `0x120AB5` is not ported to `FwDelay` yet.
6. **Type tables**: encode `fx-type-modules.csv` as `const TYPE_MODULE: [(slot, type, module_id); N]`; two types per module, one stage object per module and a parameter set per type (as the Amp already does).
7. Tests: golden tests per `(id, v)` point against the committed series (`fx-param-fits.csv` samples are float32 words), `render_hash` unchanged while the stages stay opt-in.

## Implications

- The ARM side of the FX chain is closed far enough to wire compressor amount, delay mix and feedback, Amp drive and the phaser rate with measured scalings.
- Names of two routines change (drive stage, tone stack); the models stay valid.
- The four enum ids are 12-valued with **two types per module**, so the Rust enums can share stages.

## Follow-ups

1. Enable gating and the command consumer (section 7), then re-run `fx_clients.py --slots` with the effects enabled to see the active-state words (delay time, reverb).
2. ~~Model `0x1208AF / 0x120AB5 / 0x120D15` and wire the delay time.~~ Time law decoded and wired ([fx-delay-time](fx-delay-time.md)); a bit-exact model of `0x120AB5` is still open.
3. Reverb table at DM `0x9500A..` against the multi-tap module.

## Reproduce

```bash
uv run --with unicorn python tools/arm_steady.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --save target/steady.snap
uv run --with unicorn python tools/fx_clients.py CODE.bin SRAM.bin --cache target/settled.snap --units 3 --slots u3.csv   # also 4 and 0
uv run --with unicorn python tools/fx_contexts.py CODE.bin SRAM.bin u3.csv u4.csv u0.csv --types types.csv --fits fits.csv
uv run pytest tools/tests/test_fx_forms.py tools/tests/test_fx_contexts.py tools/tests/test_fx_clients.py tools/tests/test_fx_fit.py
```

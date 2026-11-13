# PNO module chain: who installs the DSP routines, module map, piano parity list

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66; ARM mode, base `0xC0000000`), the five DSP images (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`; core 1 and core 2 SRAM images of FX, PNO, ORG), ADI `elfdump` listings of pc `0x120300..0x12F000` of six cores (`tools/adi_dis.py 0x240600 0x24F000`), `capstone` ARM listing of `0..0x279000`, `unicorn` ARM harness (`tools/arm_emu.py`, `tools/arm_capture.py`), `sharc-run` / `sharc-fx` (oracle). Derived data committed: [data/pno-module-chain.csv](data/pno-module-chain.csv) (addresses and instruction counts only). Builds on [pno-dsp-map](pno-dsp-map.md), [org-stage-map](org-stage-map.md), [organ-tonegen](organ-tonegen.md), [organ-tonewheel-bank](organ-tonewheel-bank.md), [arm-param-messages](arm-param-messages.md), [arm-fw-emulation](arm-fw-emulation.md), [fx-dsp-map](fx-dsp-map.md).
- **Question:** Which ARM code installs the routines of the PNO Core1 call table `DM(0xC01D6..)` and chain, and the Core2 stage table `DM(0xC10B8)` / strip word 0; which module id maps to which DSP routine, per image; what does each routine compute and what is the matching component in `crates/nord-core/src/piano.rs`?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = shape of code or data, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Why did a literal search for `0x1206BE`, `0x122620`, ... in the ARM find nothing? | The ARM never holds those as plain words. Its channel-record tables (`arm-chan-tables.csv`) store the entry of every installable routine as a **system address** `base = 0x28000000 + 4 pc` (core 1) or `0x28800000 + 4 pc` (core 2), in records with `size` 1..3 and `tag` 0. The sender converts back with `(base - 0x28000000) >> 2` (visible at `0xC022F2E0..0xC022F2FC`). 116 of 116 such rows are instruction starts of the image and core their `target` names. | VERIFIED (`tools/pno_module_chain.py directory --check`) |
| Target to image map | Targets `0,1` = FX core 1/2, `2` = PNO core 1, `3` = PNO core 2, `4,5` = ORG core 1/2. Each row set fits only its own image: 28/28 (target 0 on FX c1, at most 14 on any other core), 28/28 (1 on FX c2, at most 12), 45/45 (2 on PNO c1, at most 23). This turns the chip names of [arm-param-messages](arm-param-messages.md) s3 from EMPIRICAL into VERIFIED for FX and PNO core 1. | VERIFIED (section 2) |
| Who writes the PNO Core1 call table | One ARM state machine, step function `0xC022F060`, calls `0xC022C524(slot, value)` (13 call sites), which sends `(0x28300758 + 0x10 slot, routine)` = `DM(0xC01D6 + 4 slot)`. Fed by emulation, the routine installed depends on `state` (`this+0xB0`), `kind` (`this+0x8C`), `event` (`this+0x90`): section 3. | VERIFIED (emulation) |
| Routines in PNO core 1 | 45 ARM-named routines: **27 identical to routines of the FX effect library** (equal length, masked instruction streams similarity >= 0.999 against FX core 1 or 2) and **18 PNO core 1 only**: the **9 piano engine routines** (oscillator bank `0x122620`, mix stage `0x12268F`, post stage `0x12272D`, `0x121BBE`, `0x121E2B`, `0x122296`, `0x122A71`, `0x122B85`, `0x1229B3`), the parameter mapper `0x1256AD` (1,440 instr) with tail `0x126367`, the idle routine `0x121B33`, the delay clear `0x120DA2` (a near copy of FX `0x120F7E`) and five 2..10 instruction helpers `0x12031B, 0x120336, 0x12033D, 0x120349, 0x12032D` that pair with the Core2 routines `0x12538C, 0x1253A7, 0x1253AE, 0x1253BA, 0x12539E`. | VERIFIED (`tools/pno_module_chain.py shared`: 27 of 45 rows have an identical routine elsewhere), roles ASSUMED |
| Core2 stage modules | `bank0_CODE@0x2FA050` (74 ids) is indexed by `[this+0xC8]` (0..0x49) in the strip word-0 sender `0xC01CC85C`; `@0x2F72F0` (7 ids) by `[this+0x7C]` in `0xC01CCF84` (strip word 1). 56 distinct routines + 7. The code of pc `0x120300..0x125380` is **byte-identical in PNO core 2 and ORG core 2** (0 differing bytes), so both images run the same 63 stage routines; the program difference starts at pc `0x125D76`. | VERIFIED |
| Piano versus organ program | Not separable from the ARM objects driven here: the same strip sender and the same slot state machine serve both. Piano-specific is the Core1 set (PNO image); the Core2 stage set is shared. Which `state/kind/event` a piano program selects is not decoded (section 6). | open |
| Oscillator bank role | The installer places `0x122620` in the slot only for `state 1`, `kind 3, 4, 5`, `event 3 or 4`; `0x12272D` (post stage, LFO + delay taps) for `kind 4, 5` `event 1`; `0x122A71` (LFO to delay modulation) for `kind 4`, `event 2`. `state 2` events install the mix/process side: `0x12268F` (events 3, 4), `0x121E2B`, `0x1229B3`, `0x122B85`, `0x122296`. Still not decisive between B3 tonewheels and string resonance (section 6). | VERIFIED (installer), ASSUMED (role) |

## 2. The ARM names routines through channel rows (VERIFIED)

`uv run python tools/pno_module_chain.py directory docs/research/arm-chan-tables.csv --check --listing 0=FX_c1.txt --listing 1=FX_c2.txt --listing 2=PNO_c1.txt --listing 3=PNO_c2.txt --listing 4=ORG_c1.txt --listing 5=ORG_c2.txt` prints:

```text
target 0 FX c1: aligned 28      target 3 PNO c2: aligned 5
target 1 FX c2: aligned 28      target 4 ORG c1: aligned 5
target 2 PNO c1: aligned 45     target 5 ORG c2: aligned 5
```

Cross-check against the wrong cores (every row set tested on every listing, same script): target 0 rows fall on instruction starts of FX c1 28/28 but of FX c2 14/28, PNO c1 9, ORG 11; target 2 rows on PNO c1 45/45, FX c1 18, FX c2 23, PNO/ORG c2 about 12. Rows of targets 3, 4, 5 (the same shared program of PNO c2, ORG c1 and ORG c2) cannot be told apart, which is expected: they name the same Core2/ORG routines (`0x12538C`, `0x1253A7`, ... in table `0xC0544F68`).

Module directory = table `0xC0545148` (rows `>= 6`), channel tables `0xC0544DD8`, `0xC0544EC8`, `0xC0544F68`, `0xC05450C8`, `0xC0545528`, `0xC0545778`, `0xC0549E28`, `0xC054AC48`. A library module appears as a **triple** (FX c1, FX c2, PNO c1) with the same instruction stream, for example rows 18, 19, 20 = `0x12410F`, `0x12379B`, `0x124476`; the FX core 1 copy sits `0x974` above the core 2 copy ([fx-dsp-map](fx-dsp-map.md) s1).

Size field of a record: `size = ((n - 1) << 4) | flag`; the slot rows of table `0xC05475E8` have `size 0x33` = 4 words (routine, state record, buffer, 0), the chain heads of table `0xC0546F28` rows 20..23 `size 0x21` = 3 words. Code rows have `size 1..3` and tag 0 (ASSUMED: flag = word type, 3 = one 48-bit instruction). VERIFIED: `n` and layout against `0xC01D6` dump in [pno-dsp-map](pno-dsp-map.md) s6; the flag meaning is not tested.

## 3. PNO Core1 installers (VERIFIED by emulation)

| ARM function | Role | Evidence |
|---|---|---|
| `0xC022C524 (slot r0, value r1)` | writes `value` to `DM(0xC01D6 + 4 slot)` via row `slot` of table `0xC05475E8` (`ldr ip,[T, r0, lsl #4]`, 4 words of `0x10` bytes); `bx lr` for `slot > 7` | listing `0xC022C524..0xC022C660`; table rows `0x28300758 + 0x10 k`, `k = 0..7`, `size 0x33` |
| `0xC022F060 (this)` | step function: `this+0xB0` state (0, 1, 2), `this+0x8C` kind (0..5), `this+0x90` event (0..5), `this+0x3C` slot. 13 calls to `0xC022C524`, all inside it | listing; run by `tools/pno_module_chain.py slots` |
| `0xC022AB50 (this, kind)` | slot buffer pointer (`DM(0xC01D6 + 4 slot + 2)`): picks a base from tables `0xC0546228`, `0xC0547168`, `0xC0549BC8`, `0xC05488B8` by `this+0x40` (0..3) and writes `(base >> 2) + 0x20` | listing `0xC022AB50..0xC022AD38` |
| `0xC022DB38..0xC022DCFC` (inside the enable routine of [organ-tonewheel-bank](organ-tonewheel-bank.md) s3) | the 184 oscillator increments | already documented |
| `0xC0227D60` | constructor of the chain-element object, tables `0xC0544618`, `0xC0546F28` (chain heads `DM(0xC2F48, 0xC2F82, 0xC2FBC, 0xC2FD6)`, `size 0x21`) and the stub triples of table `0xC0544DD8` | listing, not run |

Slot installer result, `slot 0`, 108 combinations (`uv run --with unicorn --with capstone python tools/pno_module_chain.py slots bank0_CODE.bin bank0_SRAM.bin`; the table is the value sent for `DM(0xC01D6)`; `+n` = other messages in the same call):

| state | kind | event 0 | event 1 | event 2 | event 3 | event 4 | event 5 |
|---|---|---|---|---|---|---|---|
| 0 | any | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` |
| 1 | 0 | `0x121BBE` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` |
| 1 | 1 | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` |
| 1 | 2 | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` |
| 1 | 3 | `0x121B33` | `0x121B33` | `0x121B33` | `0x121B33` | **`0x122620`** | `0x121B33` |
| 1 | 4 | none | **`0x12272D`** | **`0x122A71`** | **`0x122620`** | **`0x122620`** | none |
| 1 | 5 | none | **`0x12272D`** | `0x121B33` | `0x121B33` | **`0x122620`** | `0x121B33` |
| 2 | any | `0x121E2B` | `0x1229B3` (+2, kinds 0, 1: +72..78 init words) | `0x122B85` | `0x12268F` | `0x12268F` | `0x122296` |

(`none` = the call sent no slot word; emulation guard path. State 2 is identical for all six kinds.) `0x121B33` is `r0 = 0; rts`: the **idle routine** (not the pass-through `0x12637B` the image ships). State 0 resets the slot to it. State 2 events 1 with kind 0 or 1 also write 72..78 words into `DM(0x95E46..0x95EA5)` (stride 6 words, values `0xC37FFDFE` (-255.99), `0x370A0000` (8.2e-6), `0x3C805600`, `0x36400000`) and, for every kind, `DM(0x95FC6)` = `1.0` (`0x3F800000`) and `DM(0x95FC7)` = 0: the parameter block of `0x1229B3` (its `pm(i9)` data). Hence a piano sound is a **two-stage slot program**: state 1 installs a generator (`0x122620` bank, `0x12272D` post stage, `0x122A71` modulator) and state 2 a processor (FIR/IIR or mix stage).

What the fields mean (`kind` = 0..5, `event` = 0..5) is not decoded: the fields are set by the owner object (`this+0x8C/0x90` are not stored by any `str ..., #0x8C` in `0xC0226000..0xC0232000`). ASSUMED: `kind` is the layer or model class and `event` a processing step.

## 4. Module map, PNO core 1 (45 ARM-named routines)

Shape counts come from `tools/sharc_routines.py` (instructions, multiplies, biquad pair counts, loops); constants are float immediates. All routines **ran** to their `rts` in the oracle when installed in slot 0 of the real pass (section 7) or are part of a bit-identical triple.

### 4.1 PNO-only (9)

| Routine | Installed by | Shape | Algorithm (reading of the listing) | Rust component | Status |
|---|---|---|---|---|---|
| `0x122620` | state 1: kind 3 ev 4; kind 4 ev 3, 4; kind 5 ev 4 | 44 instr, 3 FIR | wavetable oscillator bank: `phase += fracmul(inc, tune); idx = phase >> 20; x = T[2 idx], y = T[2 idx + 1]`, 184 elements, 2 frames | none (`organ.rs` tonewheels differ) | VERIFIED algorithm ([organ-tonewheel-bank](organ-tonewheel-bank.md)); role ASSUMED |
| `0x12268F` | state 2 ev 3, 4 | 63 instr, 17 mul, 6 FIR, 3 loops | per-element blend of x and y with two PM weight vectors, accumulation over 91 pairs, short recursive section, 2 output words per frame | none | EMPIRICAL |
| `0x12272D` | state 1 kind 4, 5 ev 1 | 256 instr, 57 mul, consts `0.5 2.0 1.0 0.08 0.91 0.2` | triangle LFO (`abs`, `*4`, clip), `fix`, 65-point table interpolation, 19-tap delay loops | none | EMPIRICAL |
| `0x122A71` | state 1 kind 4 ev 2 | 115 instr, 22 mul, 8 clip, consts `+-1` | LFO: `acc += DM(0xC27D6)`, `float`, `abs`, `2 abs - 1` (triangle), odd polynomial `x c1 + x^3 c2 + x^5 c3` with `DM(0xC27DA/DC/DE)`, then six iterations of a read-modify-write (`r2 = r0 * r2`) on a delay-line pointer set: LFO-modulated fractional delay | none (`unison` detune is a fixed ratio) | EMPIRICAL |
| `0x121BBE` | state 1 kind 0 ev 0 | 239 instr, 26 mul, 6 loops, 9 circular-buffer setups, 15 `fext/fdep` | circular delay buffer with bit-field addressing and 2 FIR taps, `1.0` constant | none | EMPIRICAL |
| `0x121E2B` | state 2 ev 0 | 455 instr, 121 mul, 18 FIR, consts `1.0 0.99 0.7 8.0 0.01` | record read (`I2, I3, I10, I11` from `-0x20..-0x1D`), 2 x (dot product of 2 x ~21 PM coefficients against a circular DM history at `0xB3300`), then a chain of 3 stereo all-pass/lattice stages (`f13 = f5*f14; f12 = f2*f7; f2 = f9 + f12`) with feedback `0.99` | `ModalResonator`? (string resonance bank) | EMPIRICAL |
| `0x122296` | state 2 ev 5 | 369 instr, 94 mul, 12 FIR, consts `1.0 0.99 8.0 0.01` | sibling of `0x121E2B` (same constants, shorter) | as above | EMPIRICAL |
| `0x122B85` | state 2 ev 2 | 359 instr, 142 mul, 16 FIR, const `0.9977` | longer FIR/IIR with loop gain `0.9977` | as above | EMPIRICAL |
| `0x1229B3` | state 2 ev 1 | 82 instr, 18 mul, 5 FIR, consts `0.001 0.09008` | 2 passes of an 8 x 4-tap dot product (`DM(0xE2110)` history x PM coefficients), three first-order sections with state in `DM(I5 + 0..6)`, two gains smoothed toward targets with `c = 0.001` (`0x3A83126F`), output `= 0.09008 * g * (...)` | `timbre_lp` / `timbre_low_lp` (timbre EQ), not equal | EMPIRICAL |

### 4.2 Library routines shared with FX (27 rows)

Identical in FX c1 / FX c2 / PNO c1 (masked instruction streams equal). Roles are those of [fx-dsp-map](fx-dsp-map.md) s5 (shape classes, ASSUMED names):

| Directory rows | PNO c1 pc (FX c2 pc) | Shape | Role (ASSUMED) |
|---|---|---|---|
| 15-17 | `0x1245D7` (`0x1238FC`) | 95 instr, 28 mul, clip | clipping mixer |
| 18-20 | `0x124476` (`0x12379B`) | 160 instr, 3 RECIPS, consts `0.85 2.0 0.99999 0.1` | table-LFO first-order all-pass chain (phaser), Newton divisions for the coefficients |
| 21-23 | `0x1235AA` (`0x1228CF`) | 147 instr, `0.9908 20 2` | LFO oscillator (cos table), smoothing `0.9908` |
| 24-26 | `0x123A07` (`0x122D2C`) | 50 instr | LFO-modulated delay tap |
| 27-29 | `0x1246AB` (`0x1239D0`) | 77 instr | modulator / pan |
| 30-32 | `0x124232` (`0x123557`) | 252 instr, `0.9908 1.0 0.5 0.293` | smoothed multi-tap delay |
| 33-35, 45-47 (tables 0xC0544DD8, 0xC0545528) | `0x124458`, `0x123E05`, `0x124168`, `0x124214`, `0x12356A` | 14 to 81 instr, loops and circular buffers | buffer clear and move |
| 36-41 | `0x12546A`, `0x1254A9` (`0x12478F`, `0x1247CE`) | 34 and 216 instr (125 mul, 25 biquad pairs) | biquad bank, Butterworth fs/4 coefficients of [pno-dsp-map](pno-dsp-map.md) s4 |
| 42-44 | `0x123DA5` (`0x1230CA`) | 44 instr | 3-section biquad |
| table `0xC054AC48` | `0x124E59`, `0x125037`, `0x12504F` | 196 / 13 / 63 instr | second fs/4 bank + head, delay clear |
| table `0xC0549E28` | `0x120DFF`, `0x120FE5`, `0x120DA2` | 201 / 17 / 50 instr | first fs/4 bank (same code as the numerically verified FX `0x120FDB`) |
| table `0xC0545528` | `0x1237F8`, `0x1236F8` | 111 / 104 instr, FIR + IIR | oversampling filters (`0x122B1D`, `0x122A1D` of FX) |
| table `0xC0545778` | `0x123914`, `0x125371`, `0x1248C9`, `0x12495F` | 99, 112, 63, 129 instr | LFO all-pass (phaser/vibe), nonlinear resonant filter, waveshaper, limiter |
| table `0xC05450C8` | `0x1256AD` (1440 instr, 437 mul), `0x126367` (11 instr) | 65-point table interpolators of [pno-dsp-map](pno-dsp-map.md) s5; short tail | parameter mapper |

The biquad bank `0x120DFF` and its two siblings are the **numerically verified** routine of [fx-dsp-map](fx-dsp-map.md) s6 (Butterworth fs/4 low pass, 16/16 output words and 24/24 state words bit-identical); because the code is identical, that verification holds for the PNO copy (VERIFIED by identity, not re-run).

## 5. Module map, Core2 (shared by PNO and ORG): ids to routines

Strip word 0 = `((DM(...) & 0x7F) << 24) | id` with `id = bank0_CODE@0x2FA050[type]`, `type` from `[this+0xC8]` (0..0x49), 3 sends per strip (PNO c2, ORG c1, ORG c2), strip `k` from `[this]` (`k <= 5`; strips 6..11 are the paired set, [org-stage-map](org-stage-map.md) s3). Strip word 1 = `bank0_CODE@0x2F72F0[[this+0x7C]]`. Full list with shapes: [data/pno-module-chain.csv](data/pno-module-chain.csv) (`arm_id74`, `arm_id7`); `uv run python tools/pno_module_chain.py ids bank0_CODE.bin 0x2FA050 74 --listing 5=ORG_c2.txt` reproduces it. All 81 ids are instruction starts (VERIFIED, matches organ-tonegen).

| Type index | Routine(s) | Shape | Reading |
|---|---|---|---|
| 0 | `0x120CD0` (default sent by the strip burst) | 97 instr, 24 mul, 8 biquad pairs, consts `0.6912 0.2304 2.1701 -2.0` | filter stage with coefficient design |
| 1-13 | `0x121183`, `0x1215FB`, `0x121B99` (x3), `0x125275` (idx 6, 14, 15, 40: 6 instr, no multiply = pass-through), `0x1212D3`, `0x12169D`, `0x121AB5`, `0x12179A`, `0x121867`, `0x1219C3`, `0x1232A5` | 65..184 instr, 8..12 biquad pairs | filter variants |
| 16-23 | `0x121C54` (x6), `0x121D6D`, `0x121E9D` | 125..138 instr, 1 RECIPS, consts `0.3456 0.1152 4.3402` | filter with division (Q or cutoff to coefficients) |
| 24-35 | `0x124985` (x3, 197 instr, FIR + loops), `0x12255D`, `0x1226DD`, `0x122A32`, `0x122C5C` (x3, 311 instr), `0x1239BE`, `0x123C7C` (358 instr, 20 biquad pairs), `0x123FE9` | 172..358 instr, 1..4 RECIPS, 10..20 clip | multi-section filters |
| 36-48 | `0x122F1F`, `0x1230DF` (8 RECIPS), `0x123427`, `0x1235C9`, `0x12377D`, `0x120DAB`, `0x120F55`, `0x12145E`, `0x122417`, `0x121FD8`, `0x122030`, `0x122151` | 34..273 instr | filters, small stages (idx 46..48: 34..121 instr, constants `0.2`, `0.0005 64.0`) |
| 49-57 | `0x124269` (x2), `0x12453F` (x3), `0x1247B6` (x2), `0x124B6A` (1 instr, same code as the FX `RTS` slot `0x120D12`), `0x124B6D` | 189..313 instr, 14..18 biquad pairs, const `0.999` | heavy filters |
| 58-73 | `0x125281 .. 0x125335`, stride `0x0C` | 6 instr each, no multiply | 16 stub variants (indexed pass-through) |
| word 1 table (7) | `0x1253BD`, `0x12566E`, `0x12549D` (182 instr, 77 mul, 10 `fext/fdep`), `0x1257B2`, `0x125913`, `0x125A50`, `0x125B79` (null) | 96..182 instr, const `3.0` | B-stage modules (delay-line module `0x12549D` of [org-stage-map](org-stage-map.md) s3) |

Observations (EMPIRICAL): 29 of the 56 distinct routines (all but the stubs and the small stages) carry the constant family `0.3456, 0.1152, 4.3402` (or twice that, `0.6912, 0.2304, 2.1701`; `0.1152 * 4.3402 * 2 = 1`), so the 74 types are one parametrised filter family (type = filter model) and not 74 independent algorithms. The names of the EQ/filter models (`Edt_*`) are not joined to the type index; `0x1206BE`, `0x123900`, `0x12082F`, `0x120C68`, `0x124C3A` of [org-stage-map](org-stage-map.md) are **not** in the ARM tables: they are routines of the ORG-only image or helpers (the ARM never names them).

## 6. Piano program versus organ program (what is and is not established)

| Fact | Status |
|---|---|
| PNO core 1 owns the 9 PNO-only routines; the ARM can install them only through the slot state machine of s3. | VERIFIED |
| The 27 library routines are in the same image but no installer was found for them in the PNO core 1 chain (rows exist; their `chain head` writers `0xC0227D60` and relatives were not run). | open |
| `0x122620` is installed by the same machine as the other piano routines (`0x12272D`, `0x122A71`) and by none of the Core2 ids. If the oscillator bank were a B3 tonewheel bank it would be installed by an organ-layer object, not by the slot machine of the PNO image; no evidence of an organ-only caller was found. This tips the balance toward the string-resonance reading (about 65 %, was 60 %). | ASSUMED |
| `kind` / `event` semantics and the program-type selector (piano category, `Edt_Piano_*`) | open: the writers of `this+0x8C/0x90` were not found in `0xC0226000..0xC0232000`; the owner object must be traced from the application constructor |

## 7. Oracle runs (VERIFIED, `sharc-run` strict)

```bash
python tools/ns4_dsp_ldr.py DIR/dsp_image_1_PNO.bin --sram SRAM
O="--dev 0x28ac0380=1 --dev 0x3108d008=5 --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 --flag 1=1 --irq P4I@2200000 --dev 0x3108945c=1"
target/release/sharc-run SRAM/core1_sram.bin 0x121007 3000000 $O --poke 2000000:0xc01d6=0x121e2b
```

The poke puts the routine into slot 0 after boot (as the ARM does), the `P4I` interrupt releases one pass. Steps added over the stub `0x12637B` (baseline 2,593,638): `0x121B33` +11, `0x121BBE` +1,433, `0x122620` +1,514, `0x12272D` +1,832, `0x121E2B` +1,142, `0x122296` +869, `0x122B85` +933, `0x12268F` +549, `0x1229B3` +253, `0x124458` +110, `0x12546A` +32, `0x1254A9` +222, `0x123E05` +111. Every one of them executes to its `rts` with `skipped ops 0`, then the pass falls into zeroed memory exactly like the baseline (the unpopulated chain, [pno-dsp-map](pno-dsp-map.md) s3). With the image's default (zero) state records, `0x121E2B` and `0x122B85` write `DM(0xFFFFFFFF)` (null record pointers), which says the real state records are built by the ARM, as expected.

Two oracle limits found on the way, both for the core lane: (1) `0x1245D7`, `0x124476`, `0x1235AA`, `0x123A07`, `0x1246AB`, `0x124232`, `0x123DA5` stop at `DO (PC, n) UNTIL LCE, LCNTR = Rn` (unimplemented form `0x12A` of the loop instruction with a register count, at `0x124624`, `0x124562`, `0x12360D`, `0x123A58`, `0x1246DE`, `0x1242F5`, `0x123DB3`), (2) `0x122A71` loops until the step limit at `0x122AF4` with a zero state record. The algorithm of `0x122620` is the only one fully reproduced ([organ-tonewheel-bank](organ-tonewheel-bank.md)); the others are described by shape (s4), not yet by a reference model, because their record layouts come from the ARM.

## 8. Comparison with `crates/nord-core` and parity to-do

| Rust component | Firmware counterpart | Match | To do |
|---|---|---|---|
| SMP sample voice (`piano/q31_kernel.rs`, `smp_unpack.rs`) | SMP image | VERIFIED (kernel bit-exact, [smp-kernel-fit](smp-kernel-fit.md)) | none here |
| `ModalResonator` x12 (`piano.rs:757`, 2-pole, T60 from a constant) | `0x121E2B`/`0x122296`/`0x122B85` (FIR + all-pass chains, loop gains `0.99`, `0.9977`) and `0x122620` + `0x12268F` (detuned bank, 91 pairs mixed with weight vectors) | structure differs: hardware has up to 184 fixed oscillators with ARM weights (if string resonance) and long FIR history, Rust has 12 modes | decide the role of `0x122620` (needs the 9 weight blocks `DM(0x9566E..0x95949)` and one piano program), then replace the 12 modes by the 92-pitch bank with detuned copies |
| `timbre_lp`, `timbre_bp_state`, `timbre_low_lp` (`piano.rs:1248`) | `0x1229B3` (FIR + 3 first-order sections + smoothed gains `c = 0.001`), biquad banks | no numeric tie | model `0x1229B3` once its coefficients (`pm(i9)` block at `DM(0x95E46..)`) are captured from the ARM |
| `dyn_comp_env` compressor (`piano.rs:1394`) | library compressor classes of FX ([fx-dsp-map](fx-dsp-map.md) s5) | none found in the 9 PNO-only routines | locate the compressor in the 36 library routines (log2 estimate class), tie to `Dyn Comp` 1..3 |
| `unison_detune_ratio` (`piano.rs:1283`) | oscillator copies spread 1.89 cent rms ([organ-tonewheel-bank](organ-tonewheel-bank.md) s4) | different unit (fixed ratio per mode vs per-element spread) | revisit after the role is settled |
| pedal noise (`pedal_noise_env`) | none | no PNO routine is noise-like (no LCG) except the three random sample-and-hold generators of [pno-dsp-map](pno-dsp-map.md) s3 step 8 | check whether the generators drive the hammer/pedal noise |
| output EQ and gains | fs/4 biquad banks (3) | cut-off identical in all copies | the banks are 2x oversampling filters; add the oversampling pair to the render path only when a per-block comparison exists |

Provenance for new constants in code: use the tags of this note (shape counts are EMPIRICAL; the filter constants `0.3456 / 0.1152 / 4.3402` are VERIFIED as immediates of the listed routines but their meaning is ASSUMED).

## Implications

- A 1:1 piano needs the ARM objects that own `this+0x8C/0x90`: the slot machine is now isolated and runnable (`tools/pno_module_chain.py slots`), so the next step is to trace which application path creates it and with which `kind`/`event` for a given piano program, then replay the resulting messages into `sharc-run` as in [arm-fw-emulation](arm-fw-emulation.md) s7.
- The ARM stores routine addresses as system addresses in channel rows; any further "where is routine X referenced" question should be answered with `arm-chan-tables.csv`, not a literal search.
- The Core2 stage set is one parametrised filter family shared by PNO and ORG: a single faithful implementation serves both images.

## Follow-ups

1. Find the owner object of `0xC022F060` (which function calls it, the writers of `this+0x8C/0x90/0xB0`) and map `kind`/`event` to piano categories and pedals.
2. Capture the DSP messages of one full piano program load (slot words, chain heads `DM(0xC2F48..)`, the 9 weight blocks of the oscillator bank) with the harness; replay into `sharc-run` and compare the mix stage `0x12268F`.
3. Run the library routines with the real state records once the core lane implements `DO ... UNTIL LCE, LCNTR = Rn` (instruction form `0x12A`).
4. Join the 74-type index to the `Edt_*` filter-model parameters (strip word 0 sender `0xC01CC85C` callers).
5. Add a reference model for `0x1229B3` and `0x121E2B` (FIR + all-pass), as was done for `0x122620`.

## Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
for i in 0_FX 1_PNO 2_ORG; do python tools/ns4_dsp_ldr.py DIR/dsp_image_$i.bin --sram DIR/sram_$i; done
python tools/adi_dis.py DIR/sram_1_PNO/core1_sram.bin 0x240600 0x24F000 > PNO_c1.txt   # likewise FX/ORG, cores 1 and 2
python tools/pno_module_chain.py directory docs/research/arm-chan-tables.csv --check --listing 0=FX_c1.txt --listing 1=FX_c2.txt --listing 2=PNO_c1.txt
python tools/pno_module_chain.py directory docs/research/arm-chan-tables.csv --listing 0=FX_c1.txt --listing 1=FX_c2.txt --listing 2=PNO_c1.txt
python tools/pno_module_chain.py shared docs/research/arm-chan-tables.csv --target 2 --listing 0=FX_c1.txt --listing 1=FX_c2.txt --listing 2=PNO_c1.txt
python tools/pno_module_chain.py ids target/fw_unpacked/bank0_CODE.bin 0x2FA050 74 --listing 5=ORG_c2.txt
uv run --with unicorn --with capstone python tools/pno_module_chain.py slots target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin
```

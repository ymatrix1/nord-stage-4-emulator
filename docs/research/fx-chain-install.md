# FX chain install: ARM routine directory, stage rows, Amp type switch, Rust parity list

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66, ARM base `0xC0000000`), the FX DSP image (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`: `core1_sram.bin`, `core2_sram.bin`), ADI `elfdump` listings of both cores (`tools/adi_dis.py 0x240600 0x24F000`), `capstone` ARM listing of `0..0x279000`, the `unicorn` harness (`tools/arm_emu.py`, `tools/arm_boot.py`). Derived data committed: [data/fx-chain-install.csv](data/fx-chain-install.csv) (record addresses, pcs, sizes, tags and census counts only). Builds on [pno-module-chain](pno-module-chain.md), [fx-dsp-map](fx-dsp-map.md), [arm-param-messages](arm-param-messages.md), [arm-edit-params.csv](arm-edit-params.csv), [effects-and-routing spec](../specs/effects-and-routing.md).
- **Question:** Which ARM data and code installs the routines of the 35-stage FX chain (7 layers x 5 effects; every stage holds the null stub `0x1249D2` core 2 / `0x125346` core 1 in the image), which effect routine belongs to which module, what are the stage records, and what does `crates/nord-core/src/fx/` lack?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = shape of code or data, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Where does the ARM name the FX routines? | In 16-byte channel records `{target, base, size, tag}` with `base = 0x28000000 + 4 pc` (core 1, target 0) or `0x28800000 + 4 pc` (core 2, target 1). `arm-chan-tables.csv` holds only 28 such rows per FX core because `tools/arm_params.py` sizes tables by the next known table start. A scan of the whole emulated ARM RAM (`0xC0540000..0xC055C000`) after the static initialiser finds **50 rows per core, 100 in total, in 43 module slots** (section 2); every row is an instruction start of its core's listing for the 28 rows per core already checked by [pno-module-chain](pno-module-chain.md) s2. | VERIFIED (`scan`) |
| Which routines were missing from the PNO note? | The compressor gain computer `0x120630`, the three delay routines `0x1206B0 / 0x1208AF / 0x120AB5`, the multi-tap pair `0x12314A / 0x12329B` (core 1 copies `0x123ABE / 0x123C0F`), the resonant filter pair `0x1243F4 / 0x124497` (core 1 `0x124D68 / 0x124E0B`) and about 15 smaller routines. Every routine class of [fx-dsp-map](fx-dsp-map.md) s5 except the inline master IIR is therefore **named by the ARM**. | VERIFIED |
| Stage rows | Channel table `0xC0547668` rows 0..34 (target 0) are the **35 stage records**: 9-word records (`size 0x83`) at DM `0xB63EE + 8 k` (step 10 after every fifth row); word 0 is the routine pointer. All 35 core 1 rows hold the null stub `0x125346` in the image and the 13 listed core 2 rows `0x1249D2` (the CSV extent is capped at 48 rows). | VERIFIED (`stages`) |
| Who writes the routine pointer? | ARM writer wrappers over table `0xC0547668` (pattern `ldr ip,[T, r3, lsl #4]`; functions `0xC021D928`, `0xC021DA80`, `0xC021DC14`: `(this, value r1, ?, row r3)`, each sends `(row base, value)`). Values are converted from system addresses with the sequence `cmn r1,#0xd8000001 ... cmp r1,#0x28800000 ... lsr r1,#2`, which occurs at more than 100 sites in `0xC0118950..0xC02429xx`. | VERIFIED (listing) |
| Amp type switch | `0xC021D064 (this, case)` has a 10-way jump table (`cmp r1,#9; ldrls pc,[pc,r1,lsl #2]`); this matches `Edt_Effects_kAmpTypeNew` (0..9) and the 10 `AmpCabinet` modes of `fx/types.rs`. Run in the harness after the layer constructor `0xC0220E50`, each case sends **two cascaded biquad coefficient sets** (5 words each at DM `0xB060B..F` and `0xB0614..18`) plus routine pointers (section 5). | VERIFIED (`amp`); order of the cases against UI names ASSUMED |
| Edit id to routine map | Not closed. `Edt_Effects_kFx1Type / kFx2Type / kRvbType / kDlyFbFxType` (0..11) go through module objects (`this+0x50..0x64` of the layer object) whose descriptors are filled by constructors the harness does not run (section 4, 6). | open |

## 2. FX code records of the ARM (VERIFIED)

```bash
uv run --with unicorn --with capstone python tools/fx_chain_install.py scan target/fw_unpacked/bank0_CODE.bin \
    target/fw_unpacked/bank0_SRAM.bin --out docs/research/data/fx-chain-install.csv \
    --listing 0=FX_c1.txt --listing 1=FX_c2.txt
# 100 FX code records
```

`scan` looks for every word whose value is a system code address of the FX image, reads `target` from the previous word and `size`, `tag` from the next two. Target 0 records must name a core 1 address and target 1 records a core 2 address (all 100 do). A *run* is a series of records 16 bytes apart: one module of the directory (core 1 rows, then core 2 rows). `size` 1, 2 = first and second routine of a two-part module, 3 = single routine (ASSUMED from the pairs); `tag 0x100 / 0x200` appears only on the delay module (three parts).

Module slots, core 1 versus core 2 (the core 1 copy of a library routine sits at core 2 + `0x974` after the pass routine; `0x120630..0x120FDB` are identical in both cores):

| Run | First record | Core 1 pcs | Core 2 pcs | size | tag | Shape (core 1; i instr, m mul, bq biquad pairs, rcp RECIPS) |
|---|---|---|---|---|---|---|
| 0 | 0xc05445b8 | `0x1231cf` | `0x12285b` | 0x3 | 0x0 | 25 i, 0 m |
| 1 | 0xc05445e8 | `0x123707` | `0x122d93` | 0x3 | 0x0 | 65 i, 16 m, 1 clip |
| 2 | 0xc0544a08 | `0x123798` | `0x122e24` | 0x3 | 0x0 | 139 i, 28 m |
| 3 | 0xc0544a38 | `0x1238eb` | `0x122f77` | 0x3 | 0x0 | 143 i, 31 m, 2 bq |
| 4 | 0xc0544a68 | `0x123a20` | `0x1230ac` | 0x3 | 0x0 | 14 i, 0 m |
| 5 | 0xc0544a98 | `0x1238c9` | `0x122f55` | 0x3 | 0x0 | 16 i, 0 m |
| 6 | 0xc0544ac8 | `0x120630` | `0x120630` | 0x3 | 0x0 | 54 i, 15 m, 2 bq, 7 clip |
| 7 | 0xc0544af8 | `0x1206b0` `0x1208af` `0x120ab5` | `0x1206b0` `0x1208af` `0x120ab5` | 0x1,0x2,0x1 | 0x100,0x100,0x200 | 237 i, 56 m, 1 rcp, 9 clip |
| 8 | 0xc0544b88 | `0x120d15` | `0x120d15` | 0x3 | 0x0 | 57 i, 0 m |
| 9 | 0xc0544bb8 | `0x123abe` `0x123c0f` | `0x12314a` `0x12329b` | 0x1,0x2 | 0x0,0x0 | 155 i, 20 m, 1 bq |
| 10 | 0xc0544c18 | `0x123dda` | `0x123466` | 0x3 | 0x0 | 18 i, 0 m |
| 11 | 0xc0544c48 | `0x12480f` | `0x123e9b` | 0x3 | 0x0 | 84 i, 26 m, 4 clip |
| 12 | 0xc0544c78 | `0x124d68` `0x124e0b` | `0x1243f4` `0x124497` | 0x1,0x2 | 0x0,0x0 | 61 i, 23 m, 2 clip |
| 13 | 0xc0544cd8 | `0x1243ef` | `0x123a7b` | 0x3 | 0x0 | 148 i, 27 m |
| 14 | 0xc0544d08 | `0x124544` | `0x123bd0` | 0x3 | 0x0 | 14 i, 0 m |
| 15 | 0xc0544da8 | `0x124efa` | `0x124586` | 0x3 | 0x0 | 51 i, 16 m, 1 rcp, 1 clip |
| 16 | 0xc0544dd8 | `0x125346` | `0x1249d2` | 0x3 | 0x0 | 2 i, 0 m (null stub) |
| 17 | 0xc0544e08 | `0x123e01` | `0x12348d` | 0x3 | 0x0 | 81 i, 9 m |
| 18 | 0xc0544e38 | `0x123ead` | `0x123539` | 0x3 | 0x0 | 14 i, 0 m |
| 19 | 0xc0545238 | `0x124270` | `0x1238fc` | 0x3 | 0x0 | 95 i, 28 m, 3 clip |
| 20 | 0xc0545268 | `0x12410f` | `0x12379b` | 0x3 | 0x0 | 160 i, 46 m, 3 rcp, 2 clip |
| 21 | 0xc0545298 | `0x123243` | `0x1228cf` | 0x3 | 0x0 | 147 i, 53 m, 9 bq, 3 clip |
| 22 | 0xc05452c8 | `0x1236a0` | `0x122d2c` | 0x3 | 0x0 | 50 i, 9 m, 1 bq, 2 clip |
| 23 | 0xc05452f8 | `0x124344` | `0x1239d0` | 0x3 | 0x0 | 77 i, 19 m, 1 clip |
| 24 | 0xc0545328 | `0x123ecb` | `0x123557` | 0x3 | 0x0 | 252 i, 70 m, 8 bq, 2 clip |
| 25 | 0xc0545358 | `0x1240f1` | `0x12377d` | 0x3 | 0x0 | 14 i, 0 m |
| 26 | 0xc0545388 | `0x125103` `0x125142` | `0x12478f` `0x1247ce` | 0x1,0x2 | 0x0,0x0 | 34 i, 5 m, 1 bq, 1 clip |
| 27 | 0xc05453e8 | `0x123a3e` | `0x1230ca` | 0x3 | 0x0 | 44 i, 6 m, 3 bq |
| 28 | 0xc0545418 | `0x123a9e` | `0x12312a` | 0x3 | 0x0 | 15 i, 0 m |
| 29 | 0xc05456e8 | `0x123491` | `0x122b1d` | 0x3 | 0x0 | 111 i, 28 m, 1 bq, 15 fir |
| 30 | 0xc0545718 | `0x123203` | `0x12288f` | 0x3 | 0x0 | 32 i, 0 m |
| 31 | 0xc0545748 | `0x123391` | `0x122a1d` | 0x3 | 0x0 | 104 i, 28 m, 1 bq, 15 fir |
| 32 | 0xc05459b8 | `0x1235ad` | `0x122c39` | 0x3 | 0x0 | 99 i, 38 m, 1 bq, 1 clip |
| 33 | 0xc05459e8 | `0x12500a` | `0x124696` | 0x3 | 0x0 | 112 i, 22 m, 2 bq, 5 clip |
| 34 | 0xc0545a18 | `0x124562` | `0x123bee` | 0x3 | 0x0 | 63 i, 17 m, 1 clip |
| 35 | 0xc0545a48 | `0x1245f8` | `0x123c84` | 0x3 | 0x0 | 129 i, 35 m, 1 bq, 1 clip |
| 36 | 0xc0545a78 | `0x124728` | `0x123db4` | 0x3 | 0x0 | 14 i, 0 m |
| 37 | 0xc0545aa8 | `0x124746` | `0x123dd2` | 0x3 | 0x0 | 86 i, 28 m, 1 clip |
| 38 | 0xc0549e58 | `0x1231cb` | `0x122857` | 0x3 | 0x0 | 2 i, 0 m |
| 39 | 0xc0549e98 | `0x120fdb` `0x1211c1` | `0x120fdb` `0x1211c1` | 0x2,0x1 | 0x0,0x0 | 201 i, 128 m, 28 bq, 18 fir |
| 40 | 0xc0549ef8 | `0x120f7e` | `0x120f7e` | 0x3 | 0x0 | 50 i, 0 m |
| 41 | 0xc054ad68 | `0x124af2` `0x124cd0` | `0x12417e` `0x12435c` | 0x2,0x1 | 0x0,0x0 | 196 i, 124 m, 26 bq, 18 fir |
| 42 | 0xc054adc8 | `0x124ce8` | `0x124374` | 0x3 | 0x0 | 63 i, 0 m |

Roles (shapes EMPIRICAL, names ASSUMED, from [fx-dsp-map](fx-dsp-map.md) s5): run 6 compressor gain computer; run 7 delay (tap reader and two variants, `RECIPS` Newton division, packed 16-bit floats); run 9 multi-tap packed delay memory (reverb lines); run 12 nonlinear resonant filter (`CLIP` inside the feedback path); runs 19-37 the modulation library of [pno-module-chain](pno-module-chain.md) s4.2 (table LFO `0x1228CF`, phaser `0x12379B`, modulated delay taps, FIR+IIR oversampling `0x122A1D / 0x122B1D`, waveshapers); run 39 the fs/4 biquad bank `0x120FDB` (numerically verified in [fx-dsp-map](fx-dsp-map.md) s6); runs 16 and 38 the 2-instruction null stub and a second 2-instruction routine (`0x1231CB` core 1, `0x122857` core 2).

## 3. Stage records (VERIFIED)

```bash
python tools/fx_chain_install.py stages docs/research/arm-chan-tables.csv core1_sram.bin   # 35 of 35
python tools/fx_chain_install.py stages docs/research/arm-chan-tables.csv core2_sram.bin   # 13 of 13 (listed rows)
```

| Item | Result |
|---|---|
| Table | `0xC0547668`: 35 target-0 rows, then target-1 rows; `size 0x83` = 9 words, `tag 0` |
| Base | row 0 = DM `0xB63EE` (system address `0x282D8FB8`), step 8 words, step 10 after rows 4, 9, ... (a two-word group header, as in [fx-dsp-map](fx-dsp-map.md) s4) |
| Word 0 | routine pointer: all checked rows hold the null stub in the image, so the ARM writes the real routine at run time |
| Words 1, 2 | input pointer and state pointer (per-stage layout of [fx-dsp-map](fx-dsp-map.md) s4: `-0x20` fn, `-0x1F` input, `-0x1E` state, `-0x1D` next link, read from the callee's state block) |
| State blocks | per-module state-block tables (`0xC0546268` rows `size 0x353` = 54 words, `0xC0546AA8` `0x463` = 72 words, `0xC0546B68` `0x183` = 25 words) carry five rows per FX core; the `tag >> 24` field runs 1..5 for core 1 and 13..17 for core 2: the **5 effects of a layer are the slots 1..5** (EMPIRICAL from the tag field) |

## 4. How the routines are installed

| Step | Evidence | Tag |
|---|---|---|
| The layer object (constructor `0xC0220E50`, vtable `0xC04437D8`) allocates module objects at `this+0x50, 0x54, 0x58, 0x5C, 0x60, 0x64`; `this+0x68` is a flag, `this+0x6C` a value, `this+0x70` a mode (0..2) | constructor run in the harness | VERIFIED (run) |
| Virtual method `0xC021D064 (this, case)` installs the Amp type: for each module object it reads the descriptor `[[obj+4]+4]` (a system address), converts it to a pc and calls the writers `0xC021CAFC (this+4, pc+0x20)`, `0xC021CF08`, `0xC021C9A0 (pc+0x20)`, `0xC021CDAC`, `0xC021CC50` | listing `0xC021D064..0xC021D928` | VERIFIED (listing) |
| `pc + 0x20` is the state pointer of the stage; the plain pc is the routine word | same | EMPIRICAL |
| The descriptors `[[obj+4]+4]` are zero in the harness (every `pc + 0x20` message is `0x20`) because the module-directory constructors (`0xC0227D60`, the effect-object constructors around `0xC01FEE08..0xC0206B44`) are not run | harness output of section 5 | VERIFIED (negative) |

## 5. Amp type switch `0xC021D064` (VERIFIED by emulation)

```bash
uv run --with unicorn --with capstone python tools/fx_chain_install.py amp target/fw_unpacked/bank0_CODE.bin \
    target/fw_unpacked/bank0_SRAM.bin
```

The harness builds the edit registry, constructs the layer object and calls the switch for `case = 0..9`. Messages (core 1 rows): coefficient sets `b0 b1 b2 a1 a2` for the TDF II form `y = b0 x + s1; s1 = b1 x + s2 - a1 y; s2 = b2 x - a2 y` ([fx-dsp-map](fx-dsp-map.md) s6) at DM `0xB060B..0xB060F` (set 1) and `0xB0614..0xB0618` (set 2); three constants at DM `0xB05F7..9` (`128.0`, `16.0` or `-3.0`, `10.0`); zeros at `0xB05FE..0xB0610`; `DM(0xC197A) = 1.1111` and `DM(0xC197B) = 0.70711` in several cases; state pointers and routine pointers at `DM(0xC1A9E..0xC1AA3)`.

| Case | Set 1 `b0 b1 b2 a1 a2` | Set 2 `b0 b1 b2 a1 a2` | Reading (ASSUMED) |
|---|---|---|---|
| 0 | 2.8102 -4.7603 2.0653 -1.4660 0.5811 | 2.7632 -5.3429 2.6158 -1.3357 0.3447 | two boost sections |
| 1 | 1.0000 -1.9693 0.9698 -1.9693 0.9698 | 3.0030 -5.8949 2.9173 -1.6843 0.6915 | near-unity section (b close to a), then boost |
| 2 | 11.1813 -19.6849 8.7030 -1.1254 0.3470 | 0.7055 -1.3993 0.6938 -1.9990 0.9991 | high-gain section, then a narrow low-frequency section (pole radius 0.9995) |
| 3 | 0.9932 -1.9722 0.9796 -1.9722 0.9728 | 3.2727 -6.4303 3.1744 -1.8372 0.8420 | as case 1 |
| 4 | 1 0 0 0 0 | 1 0 0 0 0 | identity (both sections bypassed) |
| 5 | 1.6311 -3.2623 1.6311 -1.9174 0.9206 | 0.0586 0.1172 0.0586 -1.2035 0.4379 | high pass (zeros at DC) then low pass (`b = 1 2 1` shape): band-limited cabinet, fits `Small Speaker` |
| 6-9 | none | none | no coefficient sets; cases 6 and 9 send routine pointer `0x1231CB` (2-instruction routine, run 38) to `DM(0xC1AA3)`, cases 7 and 8 send only the `0x20` state pointers |

The numbers are the float words of the firmware read back by the harness (VERIFIED). The UI name of each case is not decoded: the Rust order `Off, Twin, Twin II, JC, JC II, Small, Small II, To Rotary, LP24, HP24` is EMPIRICAL (third-party decoder), and only case 5 fits it by shape. Cases 6-9 carry the modes without biquad voicing (rotary and the two resonant 24 dB filters, which use run 12 with `CLIP` in the feedback path; ASSUMED).

## 6. What is not decoded (open)

| Item | Why | Next step |
|---|---|---|
| `Edt_Effects_kFx1Type / kFx2Type / kRvbType / kDlyFbFxType` (0..11) to routines | module objects need their directory constructors; the Amp switch is one of several type switches | run the constructors in `0xC01FEE08..0xC0206B44` and `0xC0227D60`; sibling virtual methods are in the vtable at `0xC04437D8` |
| Strip words each routine reads | routines read their own state block, not the strip; the strip offsets of the pass are in [fx-dsp-map](fx-dsp-map.md) s3 (`0x68 0x6C 0x70 0x78 0x7A 0x00 0x28 0x50 0x08 0x30 ...`, copied into `DM(0xC03D8..0xC03E8)`); joining them to `Edt_Effects_*` needs the ARM strip sender | follow-up 4 |
| Core 2 stage rows 13..34 | `tools/arm_params.py` caps table extents at 48 rows | raise the cap for table `0xC0547668` |
| Meaning of `size` | inferred from the pairs | check the writer that reads it |

## 7. Comparison with `crates/nord-core/src/fx/` and parity to-do

| Rust component | Firmware counterpart | Match | To do |
|---|---|---|---|
| `LayerFxChain` (`mod.rs`, 5 slots per layer, 7 layers) | 35 stage records, 5 effects per layer = tag slots 1..5 | structure matches | keep |
| `MasterBus` (`mod.rs`, peak envelope compressor, 0.985 one-pole) | master 4-section TDF II IIR (inline pass code, PM `0x945F2..0x94605`, 4 sets of 5 coefficients) and compressor gain computer `0x120630` | different structure | add the master 4-biquad IIR; implement the gain computer (4-lane `ABS/MAX`, `LOGB/MANT` log2 estimate, threshold, fixed-point interpolation); `kCmpAmt` (0..127, default 95) and `kCmpResponse` select its curve |
| Amp (`amp_eq.rs`, `AmpCabinet`) | 10 cases of `0xC021D064`, two cascaded biquad sets each (section 5) | no numeric tie | use the firmware sets for cases 0-5 (identity for case 4) once the case to UI order is settled |
| Amp `LP24 / HP24`, filter resonance | run 12 family (4 frames, `CLIP` in the feedback path) | not modelled | add a resonant state-variable/ladder stage with clipped feedback, 4-frame blocks |
| `Biquad` (`biquad.rs`) | TDF II, 40-bit registers, truncating store | form VERIFIED | optional: truncating store for bit-exact tests |
| Delay (`delay.rs`) | run 7 (RECIPS Newton, packed 16-bit floats `FPACK/FUNPACK`, smoothing 0.9908), `0x120D8A` modulated read, fs/4 bank `0x120FDB` | not modelled | store the line as 16-bit floats, smooth the time with the 0.9908 one-pole, add the fs/4 pair around a modulated delay |
| Reverb (`reverb.rs`) | run 9 multi-tap packed lines (8 and 16 taps) | structure plausible | derive line lengths once the type tables are found |
| Modulation (`modulation.rs`: Chorus, Flanger, Phaser, Vibe) | LFO `0x1228CF` (table, smoothing 0.9908), phaser `0x12379B` (3 RECIPS, first-order all-pass chain), triangle/odd-polynomial LFO `0x122C39` | not modelled | use the table LFO and the all-pass phaser structure |
| `Mod1Type` Tremolo / AutoPan / RingMod / Wah | modulator `0x1239D0`-class (pan/mod) and run 12 | none matched by name | identify after the type tables |

Provenance for new constants derived from this note: structure is `EMPIRICAL`; the coefficient sets of section 5 are `VERIFIED` as firmware words (reproduced by `fx_chain_install.py amp`); the case to UI-name mapping stays `ASSUMED`.

## Implications

- The FX routine library is fully named by the ARM (100 rows, 43 modules); only the inline pass code (master IIR) is not installed.
- Amp voicing is data, not code: two biquad sets per type sent by one switch. That is directly usable in `amp_eq.rs`.
- A faithful FX chain needs the per-type module descriptors (section 6); stage rows, state-block sizes and the writer functions are now isolated.

## Follow-ups

1. Run the directory constructors and the sibling virtual methods of vtable `0xC04437D8` (type switches for Fx1, Fx2, Rvb, DlyFbFx) and tabulate `type -> routine pc`.
2. Raise the 48-row cap for table `0xC0547668` and list all 35 core 2 rows.
3. Install a routine in a stage record in the oracle (`sharc-org --poke`) and compare `0x120630` and `0x12379B` with reference models.
4. Join strip offsets to `Edt_Effects_*` through the strip sender.

## Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_ldr.py DIR/dsp_image_0_FX.bin --sram DIR/sram_0_FX
python tools/adi_dis.py DIR/sram_0_FX/core1_sram.bin 0x240600 0x24F000 > FX_c1.txt   # likewise core 2
uv run --with unicorn --with capstone python tools/fx_chain_install.py scan target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --out docs/research/data/fx-chain-install.csv --listing 0=FX_c1.txt --listing 1=FX_c2.txt
python tools/fx_chain_install.py stages docs/research/arm-chan-tables.csv DIR/sram_0_FX/core1_sram.bin
uv run --with unicorn --with capstone python tools/fx_chain_install.py amp target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin
```

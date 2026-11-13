# FX type map: module objects, routine ownership and what a type switch sends

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66, ARM base `0xC0000000`), `capstone` ARM listing of the whole image, the `unicorn` harness (`tools/arm_emu.py`, `tools/arm_boot.py`, `tools/arm_boot_full.py`), FX DSP image listings (`tools/ns4_dsp_images.py`). Derived data committed: [data/fx-type-map.csv](data/fx-type-map.csv) (descriptor addresses, tables, pcs only). Builds on [fx-chain-install](fx-chain-install.md), [fx-dsp-map](fx-dsp-map.md), [pno-module-chain](pno-module-chain.md), [arm-edit-params.csv](arm-edit-params.csv).
- **Question:** Which FX routine pcs does each ARM effect module own, and does `Edt_Effects_kFx1Type / kFx2Type / kRvbType / kDlyFbFxType` (0..11) select a routine, a data set or a mode word?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = shape of code or data, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is there a table `type index -> routine pc`? | **No such table was found in the ARM image** (static scan, section 5). Routines are bound to module objects (and to the 12 unit instances), not to type values. A type value reaches the DSP as **data** (coefficient sets, FIR taps) or as a **mode word** (section 5). | VERIFIED for the Amp unit (`probe`, `modules`); open for the four Edt type ids |
| Module to routine map | 100 of 100 FX code records of [fx-chain-install](fx-chain-install.md) s2 belong to one of **28 descriptor lists** (33 lists in all, 5 hold only state tables). Section 2. | VERIFIED (`inventory`) |
| What is a module object | An enum property: value `+0x40`, previous value `+8`, parent `+0x38`, descriptor slice `+0x34`; vtable slot 7 `SetValue` (`0xC02237D0`), slot 6 range getter, slot 24 change handler (called by the commit `0xC0223708` when the value changed). Section 3. | VERIFIED (disassembly, `modules`) |
| Type switches run | 3 handlers of the Amp unit: the 10-way layer switch (`0xC021D064`, [fx-chain-install](fx-chain-install.md) s5), a 12-way handler (`0xC020506C`, module `+0x54`) that sends biquad sets and a FIR, and a mode-word handler (`0xC01F4844`, module `+0x5C`). Section 4 and 5. | VERIFIED (`probe`) |
| Edt type ids to handler | **Not closed.** The FX unit objects (`0xC01155EC`) are built by a part constructor (`0xC01177AC`) from a booted application state. The harness cannot build them; the blocker is named in section 6. | open |

## 2. Module to routine map (VERIFIED)

```bash
uv run --with unicorn --with capstone python tools/fx_type_map.py inventory \
    target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
    --out docs/research/data/fx-type-map.csv --listing 0=FX_c1.txt --listing 1=FX_c2.txt
# 33 descriptor lists, 100 code records
```

Each list is a run of 16-byte descriptors `{table, 0, 1, rows}` in the ARM data (`0xC0278FAC..0xC02BB5E4` and `0xC04414C4..0xC0443D3C`). `table` is a channel table of [fx-chain-install](fx-chain-install.md): a table of FX code records (`base` = system address of a routine) or of DSP state blocks (12 or 14 rows, one per unit instance). The 28 lists that hold code cover **all 100 code records** and so all 43 module slots; no routine of the library is unowned. Core 1 pc / core 2 pc, instruction count of core 1 (EMPIRICAL shape, role ASSUMED from [fx-dsp-map](fx-dsp-map.md) s5):

| List | Code descriptors (run of fx-chain-install s2) | Routines core 1 / core 2 | Role (ASSUMED) |
|---|---|---|---|
| `0xC0278FAC` | runs 24, 25 | `0x123ECB / 0x123557` (252 i, 8 bq), `0x1240F1 / 0x12377D` (14 i) | heavy biquad bank + small helper |
| `0xC0279098` | runs 7, 8 | `0x1206B0 / 0x1208AF / 0x120AB5` (identical in both cores), `0x120D15` | **delay** (tap reader, two variants, helper) |
| `0xC0279128` | runs 35, 36 | `0x1245F8 / 0x123C84` (129 i), `0x124728 / 0x123DB4` | |
| `0xC0279290` | run 11 | `0x12480F / 0x123E9B` (84 i) | |
| `0xC02BAF54` | run 1 | `0x123707 / 0x122D93` | |
| `0xC02BB030` | run 32 | `0x1235AD / 0x122C39` | phaser all-pass |
| `0xC02BB10C` | run 23 | `0x124344 / 0x1239D0` | pan / modulator |
| `0xC02BB1E8` | run 20 | `0x12410F / 0x12379B` (3 `RECIPS`) | phaser |
| `0xC02BB2C4` | run 19 | `0x124270 / 0x1238FC` | |
| `0xC02BB3A0` | runs 17, 18 | `0x123E01 / 0x12348D`, `0x123EAD / 0x123539` | |
| `0xC02BB44C` | run 39 | `0x120FDB / 0x1211C1` (identical in both cores) | fs/4 biquad bank |
| `0xC02BB520` | run 40 | `0x120F7E` | |
| `0xC02BB54C` | run 41 | `0x124AF2 / 0x12417E`, `0x124CD0 / 0x12435C` | |
| `0xC02BB5E4` | runs 42, 22 | `0x124CE8 / 0x124374`, `0x1236A0 / 0x122D2C` | |
| `0xC04414C4` | run 37 | `0x124746 / 0x123DD2` | waveshaper |
| `0xC0441558` | runs 6, 13, 14 | `0x120630` (both cores), `0x1243EF / 0x123A7B` (148 i), `0x124544 / 0x123BD0` | **compressor** (gain computer + two helpers) |
| `0xC0441A18` | runs 27, 28 | `0x123A3E / 0x1230CA`, `0x123A9E / 0x12312A` | |
| `0xC0441AC4` | runs 9, 10 | `0x123ABE + 0x123C0F / 0x12314A + 0x12329B`, `0x123DDA / 0x123466` | **multi-tap packed delay** (reverb) |
| `0xC0441B70` | run 34 | `0x124562 / 0x123BEE` | |
| `0xC0442A94` | runs 29, 31 + 4 state tables of 14 rows | `0x123491 / 0x122B1D`, `0x123391 / 0x122A1D` (FIR + IIR oversampling) | Amp cabinet FIR (section 4) |
| `0xC0443694` | runs 3, 4 | `0x1238EB / 0x122F77`, `0x123A20 / 0x1230AC` | |
| `0xC0443740` | runs 2, 5 | `0x123798 / 0x122E24`, `0x1238C9 / 0x122F55` | |
| `0xC04437B0` | run 0 | `0x1231CF / 0x12285B` (25 i) | Amp layer's own routine |
| `0xC04438D8` | runs 21, 12 | `0x123243 / 0x1228CF` (table LFO), `0x124D68 + 0x124E0B / 0x1243F4 + 0x124497` (resonant filter pair) | LFO + resonant filter |
| `0xC044398C` | run 15 | `0x124EFA / 0x124586` | |
| `0xC0443A20` | runs 33, 26 | `0x12500A / 0x124696`, `0x125103 + 0x125142 / 0x12478F + 0x1247CE` | |
| `0xC0443AD4` | runs 38, 30 | `0x1231CB / 0x122857` (2 i), `0x123203 / 0x12288F` | |
| `0xC0443D3C` | run 16 | null stub `0x125346 / 0x1249D2` | stage default |

## 3. Module object model (VERIFIED by disassembly and the harness)

| Item | Result |
|---|---|
| Layout | `+0` vtable, `+4` registered row (`0xFFFF`/`-1` when the directory refused it), `+8` previous value, `+0x34` pointer to the descriptor slice of the module (a list entry, section 2), `+0x38` parent container (`layer + 0x48`, vtable `0xC0443890`), `+0x40` value |
| `SetValue` (slot 7, `0xC02237D0`) | returns if the value is unchanged or if the range getter (slot 6) is not above it |
| Commit (slot 4, `0xC0223708`) | copies `+0x40` to `+8`, sets `+0x45`, calls slot 24 `OnChange(this)` |
| Registration | the module ctor calls `parent->vtbl[1](parent, this, slice, ...)` = `0xC02240F8`, which forwards to the directory object (`0xC0227D60`, one per unit instance L = 0..11, args from the table at `0xC02BAD20 + 0xC4/0xD0/0x100/0x10C`) and allocates a row of the slice's tables by tag. The directory ctor needs the SPI stubs of `arm_boot_full.install_ui_stubs` and `0xC0011B60` stubbed to 0; without a directory every module registers row 0 (the setting used by `probe`) |
| Mode setter (slot 8) of the 12 library classes | e.g. `0xC01F2C40 (this, mode 0/1/2, row)`: stores the mode at `+0x50` and writes a chain link word (`DM(row base + 5)` = pc of the other row `+ 1`); this is the stage topology, not a routine pointer |

Amp layer (`0xC0220E4C`, vtable `0xC04437D8`): the six module objects and their slices (`tools/fx_type_map.py modules`, VERIFIED):

| Offset | Object vtable | Range getter | Change handler | Slice (code descriptors) |
|---|---|---|---|---|
| `+0x50` | `0xC0278E84` | 1 | none | state `0xC05488F8`, LFO `0xC0545298` (`0x123243 / 0x1228CF`) |
| `+0x54` | `0xC0442A28` | **6** | `0xC020506C` (12-way) | 4 state tables x 14, FIR/IIR `0xC05456E8`, `0xC0545748` |
| `+0x58` | `0xC0443920` | 1 | none | state `0xC0546CE8`, resonant filter pair `0xC0544C78` |
| `+0x5C` | `0xC04439B4` | **2** | `0xC01F4844` | state `0xC05470A8`, run 15 `0xC0544DA8` |
| `+0x60` | `0xC0278E84` | 1 | none | state `0xC0549358`, run 33 `0xC05459E8` |
| `+0x64` | `0xC0443A68` | 1 | none | state `0xC0548E58`, run 26 `0xC0545388` |

## 4. The Amp unit's type switches (VERIFIED by emulation)

```bash
uv run --with unicorn --with capstone python tools/fx_type_map.py probe CODE.bin SRAM.bin \
    --module 0x54 --handler 0xC020506C --types 12 --variant 2
uv run --with unicorn --with capstone python tools/fx_type_map.py probe CODE.bin SRAM.bin \
    --module 0x50 --handler 0xC0206B58 --types 6 --variant 2
uv run --with unicorn --with capstone python tools/fx_type_map.py probe CODE.bin SRAM.bin \
    --module 0x5C --handler 0xC01F4844 --types 12
```

`--variant N` stubs the hardware variant query `0xC0228CB4` (returns the parent's `vtbl[7]`, 1 or 2 on hardware; it decides which branch each case takes).

| Handler | Types that send | Sends (core 1) | Reading |
|---|---|---|---|
| `0xC020506C` (module `+0x54`, range 0..6) | 0, 1, 2, 3, 7 (types 4, 5, 6, 8..11 send nothing in any variant: their branch is gated by the DSP-memory size checks `[row + 8] >= 0x720/0x790` of the 14-row tables, row 0 in the harness; the bodies `0xC0204FF8 (this, 0x63/0x82, ...)` were not exercised) | **130 data words at `DM(0x94030..0x940B1)`** (written from `0x940B1` downwards; a FIR coefficient run), **N biquad sets of 5 words at `DM(0xB0800 + 6 k)`** (stride 6, word 5 of each stride untouched), `DM(0xB084A..B)` and `DM(0xB0866)` (a length / delay pair). Type 0: one tap `DM(0x940B1) = 0.681877`, `DM(0xB084B) = 2`, `DM(0xB0866) = -4` (variant independent). Types 1, 2, 3 (variant 1 and 2): 12 sets (`k = 0..11`), 114 / 130 / 121 non-zero taps (`0x94040..`, `0x94030..`, `0x94039..`), `DM(0xB084B) = 6`, `DM(0xB0866) = -228 / -228 / -242`. Type 7: 6 sets (`k = 0..5`), all 130 taps, `DM(0xB084B) = 3`, `DM(0xB0866) = -260` | a cabinet-like FIR plus biquad bank for the **legacy amp types** (`Edt_Effects_kAmpType_obsolete2`, 0..6; ASSUMED from the range 6). The routine pair is fixed (`0x122A1D / 0x122B1D`), the type selects **data** |
| `0xC0206B58` (module `+0x50`, 6 cases) | all | the same two cascaded biquad sets as the Amp switch ([fx-chain-install](fx-chain-install.md) s5): case 0 identity, 1 = switch case 0 (2.8102 ...), 2 = case 1, 3 = case 2, 4 = case 3, 5 = case 5, plus `DM(0xB05F7..9) = 128, 16 (or -3), 10` | the Amp switch (`0xC021D064`) calls this per-module voicing; the case to UI-name map stays ASSUMED |
| `0xC01F4844` (module `+0x5C`, range 0..2) | all values 1..11 | **one word**: `DM(0xC1A24) = type` (core 1 only) | a pure **mode word**; the value is read by the DSP routine, no routine pointer moves |

So for the Amp unit **the type never selects a routine pointer**: the libraries of section 2 are bound once (registration), and the 10 + 7 + 3 type values are coefficient sets, FIR taps and a mode word. That is the structure the Rust `amp_eq.rs` should copy.

## 5. What this means for `Fx1Type / Fx2Type / RvbType / DlyFbFxType`

* The four ids are 12-valued (`arm-edit-params.csv`: min 0, max 11) like the 12 unit instances (`this+4 <= 11` checks all over the module code), which is a reason **not** to read them as "12 routines": a unit instance index and a type are different axes.
* Candidate carriers, by the pattern of the Amp unit: (a) a mode word like `DM(0xC1A24)`; (b) coefficient or tap data like the FIR of `0xC020506C`; (c) a chain-link mode (`0xC01F2C40` writes `0/1/2` and a link word). A static type-to-routine pointer array was **not found**: scanning `0xC0000000..0xC0540000` of the initialised image there is no run of 3 consecutive words that are code-record addresses, and the 45 words that name the first record of a run lie in the descriptor lists of section 2 (per module, not per type). A table built at run time by code cannot be excluded.
* The unit class that owns Fx1, Fx2, Rvb and Dly objects is created by `0xC01155EC` at `unit + 0x208..0x244` (ctors `0xC01F2C40`-family at `+0x208`, `+0x20C`, `0xC01FBDA8` `+0x214`, `0xC0013228` `+0x218`, `0xC0010880` x3, `0xC021B170`, `0xC021AD80`, `0xC0012958`, `0xC01FC808`, the Amp layer `0xC0220E4C` at `+0x238`, `0xC01FC17C`, `0xC0201B0C`, `0xC0011D10`). Which of these is the Fx1 / Fx2 / Rvb / Dly object is not decoded.

## 6. Blocker and next step (open)

**Update (2026-11-13): resolved in [fx-contexts](fx-contexts.md).** The part constructor chain does not need to be built by hand: the boot builds all 6 FX parts and 12 FX unit objects (hooks on `0xC01177A8` / `0xC01155EC`); they are only missing from the dispatcher's client list. `tools/fx_clients.py` drives the Edt ids through them; the type to module table is `data/fx-type-modules.csv` (two types per module). The text below is the state before that finding.

`0xC01155EC (unit, mode, L, &{0, x, 0x10}, flag, ..., part-linked pointers)` is called by the part constructor `0xC01177AC (part, layer, x, mode)` (twice, L from `part + 0x5AC4/0x5AC8`: 3 and 9, 4 and 10, 6 and 7 or 0 and ... by mode). Its first call `0xC0014068` builds a module block through the part object (`[part + 0x5000 + 0x5F8 + 4 k]`, vtable `[part + 0xD0]`), and with a part built from zeroed memory it enters a clear loop with a garbage element count (`0xC0014454`) after 200 M instructions. The part constructor needs the store, program-object and port objects of the booted application ([arm-organ-messages](arm-organ-messages.md) s8, s9). The Fx ids are subscribed in `0xC0117A40..0xC0118100` (`0xC00D57F0 (part, id)`), but the handler that forwards an id to a module is behind the same object graph.

Next steps, in order: (1) reach the steady state of `arm_boot_full.boot_full` (task-join loops, [arm-organ-messages](arm-organ-messages.md) s9), then re-run `tools/arm_sweep.py` with `Edt_Effects_k*Type` and compare the message streams for types 0..11; (2) or build the part object by hand: `0xC0014068` needs `part` fields `+0x5F8..`, `+0xD0` (vtable with `[vt+0xC]`), and valid counts at `[part+0xD0]`.

## 7. Reference model of the compressor gain computer `0x120630` (VERIFIED against the oracle)

```bash
cargo build -p nord-sharc --release
python tools/fx_routine_models.py check compressor core2_sram.bin --scenarios 500 --seed 21
# compressor 0x120630: 500/500 scenarios bit-identical (outputs and context words)
# golden scenario (6 blocks, state carried): identical
python tools/fx_routine_models.py golden compressor > crates/nord-core/src/fx/tests_compressor_golden.rs
```

`tools/fx_routine_models.py::compressor_0x120630` transliterates the routine over a flat memory dict. 2,100 random scenarios (seeds 5, 7, 8, 9, 11, 21; finite floats, signed and zero inputs, aliased frame offsets, both `GT` branches) plus the 6-block golden scenario give **0 differing words** in the 8 block words and the 26 context words (`I1 - 0x20 .. I1 - 0x07`). Tests: `tools/tests/test_fx_routine_models.py` (the 48 golden output words are recorded from the oracle).

Two facts the transliteration needed, both **VERIFIED** by the mismatches they removed:

* The pass sets `MODE1` `PEYEN` and `BDCST1` (`MODE1 |= 0xE00000`). A load through `I1` is **broadcast** (`PEy` gets the `PEx` word); a load through `I4` is a pair `(word, word + 1)`. The routine uses `I1` for parameters and `I4` (`= I1`) for per-channel state.
* All four frame loads precede the four stores, so aliased offsets (`M3 = M4 = 2`) read the old data.

Data flow (per block of 4 stereo frames; left = `PEx`, right = `PEy`; `P` = linked peak):

| Step | Equation | Words (offset from `I1`) |
|---|---|---|
| Peak | `p_c = max |x|` over the 4 frames, `P = max(p_L, p_R)` | frames at `I0 + 0, 2, M3, M4` |
| Level | `level_c = max(P, P r + (L - L r))`, stored back | `L` pair `-0x11`, release `r` pair `-0x1E` |
| Makeup | `B_c = T_c + (B_c - T_c) k`, stored back; both elements use the **left** word of `B` afterwards | target `T` pair `-0x1C`, `B` pair `-0x1A`, `k` `-0x09` |
| Log | `lg = ((e + 1) + MANT * 2^s) * K` with `s = -31` gives `e + frac` (`level = 2^e (1 + frac)`) | `s` `-0x0D`, `K` `-0x0B` |
| Excess | `o = max(lg, thr) - thr`; `c = a` if `o > E` else `r`; `E' = o c + (E - E c)`, stored back | threshold pair `-0x19` (**its left word is the right word of `B`**), attack `a` pair `-0x20`, `E` pair `-0x0F` |
| Reduction | `q = max(E'_L w_L, E'_R w_R) * g` (linked), `t = FIX(q, 2^f)` | slope `w` pair `-0x17`, `g` `-0x15`, `f` `-0x0C` (31 gives Q31) |
| Gain | `G = bits(T0 - T0 t + T1 t)` with the fractional `SSF` product (floor of `2ab` over `2^32`), read as a float | `T0` `-0x0A`, `T1` `-0x14` (1.0 and 0.5 give a piecewise `2^-t`) |
| Output | `y = x B_L G` for both channels | block in place |

Units: the level, threshold and excess are in log2 (octave) units of the block peak (`level` is a linear amplitude); `t` is the fraction of one octave of gain removed when `T0 = 1.0`, `T1 = 0.5`; the smoothing coefficients are one-pole weights per block (4 frames, 83 us at 48 kHz): `0.9908` is the same constant as the ORG and delay smoothers ([fx-dsp-map](fx-dsp-map.md) s5), so `k = 0.9908` is a likely `makeup` value (ASSUMED).

Not decoded: which ARM words fill the context (`Edt_Effects_kCmpAmt` 0..127 default 95, `kCmpResponse` 0/1), so the numeric defaults of a real instance are unknown. In the Rust port (`fx/fw_compressor.rs`) the algorithm is VERIFIED and every parameter value is ASSUMED.

Rust parity: `FwCompressor` reproduces the oracle golden vector with a worst relative output error of 2.1e-7 (f32 arithmetic against 40-bit registers and truncating stores); `MasterBus::set_fw_compressor(Some(..))` swaps it in for the peak-envelope compressor with a 4-frame latency. Default off: `render_hash` is `cb8d83b758cc3596ba52f1444b0e1f5d40726d27f36e53205464b47b414aacd0` before and after.

The delay `0x1206B0`, the table LFO `0x1228CF`, the all-pass `0x122C39` and the resonant filters `0x1243F4` / `0x124497` are modelled the same way (instruction-level transliterations checked against the oracle word for word and, through a new `sharc-fx --trace`, register for register); equations, tools and Rust ports are in [fx-routine-models](fx-routine-models.md).

## 8. Amp types decoded: `kAmpTypeNew` words, the drive stage with the real tables (2026-11-13)

```bash
uv run --with unicorn --with capstone python tools/fx_clients.py CODE.bin SRAM.bin --cache target/settled.snap \
    --slots amp.csv --slot Amp --units 3 --types-only            # about 1 min
python tools/fx_amp_types.py extract amp.csv --out docs/research/data/fx-amp-types.csv
python tools/fx_amp_types.py golden FX_core2_sram.bin PNO_core1_sram.bin > crates/nord-core/src/fx/tests_amp_golden.rs
cargo test -p nord-core --lib fx::fw_amp
```

**The type id.** `Edt_Effects_kAmpTypeNew` is 0..9 = the amp mode of the program (`amp_type_new & 15`: 1 Twin, 2 Twin II, 3 JC, 4 JC II, 5 Small, 6 Small II, 7 To Rotary, 8 LP24, 9 HP24). The Vox program's raw value 289 is `0x121`: mode 1 (Twin); bit 8 is the reverb type bit (`fx_preset.rs`). VERIFIED (the words below follow the value; 6 of 9 modes carry drive stage words).

**What a type sends (core 1, organ unit L3, `I1 = 0xC018A`).** `fx-amp-types.csv` holds every word that lands in the drive stage context (`0xC016A..0xC019F`, module `0xC04438D8#0`, routine `0x1228CF` on core 2 and `0x123243` on core 1):

| Context word | Value | Reading |
|---|---|---|
| `c[-0xD]`, `c[-0xB]` | `128.0`, `10.0` for every type | `G1` scale, `G2` offset |
| `c[-0xC]` | `16.0` (types 1, 2, 3, 5, 6), **`-3.0` (type 4, JC II)** | `G2` slope: `G2 = B(phase) (phase slope + offset)` |
| `c[7..=0xB]` | type 1 `2.8102 -4.7603 2.0653 -1.4660 0.5811`; type 2 `1 -1.9693 0.9698 -1.9693 0.9698`; type 3 `11.1813 -19.6849 8.7030 -1.1254 0.3470`; type 4 `0.9932 -1.9722 0.9796 -1.9722 0.9728`; type 5 identity; type 6 `1.6311 -3.2623 1.6311 -1.9174 0.9206` | section 1 `b0 b1 b2 a1 a2` |
| `c[0x10..=0x14]` | type 1 `2.7632 -5.3429 2.6158 -1.3357 0.3447`; type 2 `3.0030 -5.8949 2.9173 -1.6843 0.6915`; type 3 `0.7055 -1.3993 0.6938 -1.9990 0.9991`; type 4 `3.2727 -6.4303 3.1744 -1.8372 0.8420`; type 5 identity; type 6 `0.0586 0.1172 0.0586 -1.2035 0.4379` | section 2 |
| states `c[-6..-3]`, `c[3..6]`, `c[0xC..0xF]` | 0 | state reset |
| types 7, 8, 9 | no words in this context | To Rotary, LP24, HP24 use other modules |

These are the six "cases" of [fx-chain-install](fx-chain-install.md) s5 (case `k` = type `k + 1`). **Correction:** that note and `fw_coeffs::amp_fw_case_for` read case 5 (the `1.63 / 0.0586` high-pass and low-pass pair) as `Small`; the type value shows it is **Small II** (type 6), and `Small` (type 5) installs identity sections. `amp_fw_case_for` is opt-in and unchanged here (TD-10).

**Cabinet words of the same switch (not used).** Types 1 and 2 send a 105 tap FIR (`0x969D9..0x96A41`), types 3 and 4 130 taps, types 5 and 6 99 taps (module `0xC0442A94`, routines `0x122A1D / 0x122B1D`), a 12 set biquad bank at `0xE3702..0xE374D` (stride 6) and a length word. Twin and Twin II share the cabinet, as do JC / JC II and Small / Small II, so each pair differs only in the two drive sections. This cabinet is the speaker model; it has no Rust port.

**Tables and static words.** The drive stage reads three tables through the context pointers `ctx[-0x15] = 0xE0802` (`A`, 129 entries), `ctx[-0x14] = 0xE0883` (`B`, 129) and `ctx[2] = 0xE0200` (`C`, signed 10 bit index, 1025 entries). The PNO DSP image core 1 holds them (the same words are in the FX and ORG images) together with the context template of two stage instances (stride 54 words): high-pass `p = 0.99822`, `q = -0.99644`, `FIX` scale 31, mix bound `1.0`, shaper bound `0.99999994`, loop count 2 (two blocks of two frames: the routine runs once per 4 frames). `A` rises 0.0037 to 0.698, `B` falls 0.126 to 0.0033 irregularly, `C` is asymmetric (the positive half saturates at +0.31, the negative half reaches -1.0): measured data without a closed form (`fw_drive_tables.rs`). VERIFIED (dump through `sharc-fx`).

**Oracle check (VERIFIED).** `fx/tests_amp_golden.rs` is recorded from `sharc-fx` running the FX core 2 copy of `0x1228CF` (the copy `tools/fx_model_lfo.py` is verified on, 3,600 scenarios) with the real context (template + type words + `kAmpDrive`) and the real tables on a seeded two tone signal, 24 calls with the state carried. `FwAmpDrive` with `fw_drive_tables.rs` and `fw_amp_types.rs` reproduces it: Twin at `kAmpDrive 14` worst difference 1.4e-6 of the output peak, JC at `kAmpDrive 100` 1.7e-4 (f32 against 40 bit registers). The PNO core 1 copy of the routine was not executed (its pass context is not reachable with the harness); equality of both copies is ASSUMED (the same library code relocated). The drive law is `phase = kAmpDrive / 127` (smoothed per call), indexing `A` and `B` and the mix `clip(20 phase, 1)`.

**Opt-in wiring.** `FirmwareFx::amp` (also set by `FirmwareFx::ALL` and `render-patch --firmware-fx`): Twin, Twin II, JC, JC II, Small and Small II run `FwAmpStage` (4 frame latency) in place of the engine `tanh` drive and RBJ cabinet; the 3-band EQ and compressor that follow are the engine's; To Rotary, LP24 and HP24 keep the engine path. Default and `render_hash` unchanged (`25baa387...`).

**Vox before / after** (`tools/emu_vs_hw.py --spec docs/research/data/emu_vs_hw_amp_vox.json`, segment `organ_vox`, bands in dB re RMS, emulator minus hardware, 8 to 20 kHz not scored):

| Variant | sub | low | mid | pres | brill | centroid Hz | RMS | score |
|---|---|---|---|---|---|---|---|---|
| `vox` (engine amp) | -0.1 | +1.4 | +8.0 | -4.9 | -5.5 | -624 | +0.2 | 3.98 |
| `vox_noamp` | +1.4 | +0.5 | +8.1 | -7.4 | -11.7 | -979 | -1.3 | 5.82 |
| `vox_fwamp` (`--firmware-fx`) | +2.5 | -0.9 | -2.5 | -9.6 | -3.0 | +1728 | +11.6 | 3.69 |

Reading: the firmware drive stage alone is **not** a better amp. The mid excess drops (+8.0 to -2.5) and brilliance rises (-5.5 to -3.0), but presence falls (-4.9 to -9.6), the level rises 11.4 dB (the Twin sections have a gain of about +12 dB and there is no cabinet to take it back) and the centroid overshoots (+1728 Hz, the unscored air band +28 dB): the missing cabinet FIR (the speaker roll-off) and the organ unit's tone routines are the other half of the amp. The composite score improves slightly (3.98 to 3.69) for the wrong reasons. The switch therefore stays opt-in; the next step is the cabinet routine `0x122A1D / 0x122B1D` (FIR plus 12 biquads, state in PM) with the words above.

**Stop point.** Done: type to word decode (6 of 9 modes), table dump, oracle-verified drive stage with the real context, opt-in wiring, Vox measurement. Not done: the cabinet FIR and biquad bank model, the organ unit's tone routines (`0xC02BB10C#1`, `0xC02BB2C4#1`, routines `0x124344 / 0x1239D0`, `0x124270 / 0x1238FC`) and the amp output level staging.

## 9. Leslie drive: what the firmware does (investigation, stop point)

`Edt_OrgCom_kEffectsLeslieDrv` (0..127) writes `v / 127` into two DM words of the organ unit's row on the PNO DSP chip core 1 (`0xB2F2D`, `0xB2F31`, ARM writer `0xC0062D54`; captured with `fx_clients.py --slots leslie.csv --slot Leslie --units 3`: words `0x3C010204` at `v = 1` up to `0x3F800000` at `v = 127`, exactly `v / 127`). The rows at `0xB2F29..0xB2F31` carry the routine pointer `0x12637B`, which in the PNO core 1 image is a two instruction null stub (`NOP`, `RTS`). So **no code of the FX / PNO library reads the Leslie drive**: the rotor, horn, crossover and drive run elsewhere (the ARM tick state machine and the ORG image, [fx-contexts](fx-contexts.md) s6). VERIFIED: `d = v / 127` and the stub. Not found: the nonlinearity, its gain or any level staging. The engine's `tanh((1 + 3 drive) x)` (`organ/rotary.rs`, +18.4 dB small signal at drive 2.44; `rotary_drive` is the same fraction times 10, `organ_preset.rs`) has no firmware counterpart to port, so (B) stops here and the engine rotary is unchanged.

Fix proposal (not implemented; `organ/` is outside this change): make the rotary drive independent of the bus scale by dividing by its small signal gain, `tanh(g x) / g` with `g = 1 + 3 drive`, behind an opt-in switch next to `--tone-source`. The unity small signal gain removes the +18.4 dB that compensates the Legacy level and keeps the clipping shape. Evidence to collect first: the Leslie preamp in the ORG image (search the organ chip for the drive state words and a clip table) or a hardware recording with the rotary off and on.

## Implications

* The effect library is fully partitioned into 28 modules; each routine has an owner in the ARM (section 2). A Rust parity effort can name its blocks by module instead of by pc.
* For the Amp unit the firmware's `type -> behaviour` is data and a mode word. That is a weaker coupling than the "type selects routine" reading, and it means the FX types of the Rust enums need **parameter sets**, not different algorithms, wherever a module's slice is shared.
* The Edt type map to routines remains open; do not encode a type to routine table in Rust.

## Follow-ups

1. Close the part constructor chain of section 6 and sweep the four type ids.
2. Decode the unit's other change handlers (the classes of section 2 whose slot 24 is not a no-op: `0xC01F222C`, `0xC01F06C0`, `0xC01F3BC4`, `0xC0212A18`, `0xC021A6B8`, `0xC01F9668`, `0xC01FA51C`, `0xC01FCBC8`, `0xC0218A38`, `0xC01EAC40`) with bare module objects built like `probe`.
3. Raise the 12 x 14-row table gates of `0xC020506C` (types 4..6, 8..11) with a real directory.
4. Model the amp cabinet `0x122A1D / 0x122B1D` (s8) and the organ tone routines; find the Leslie drive nonlinearity (s9).

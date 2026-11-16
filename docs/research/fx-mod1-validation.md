# FX Mod 1 laws at 44 values, ENS amount and rate words, ENS tap registers

- **Checked:** 2026-11-15
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66), `target/settled.snap`, the FX image and `SRAM/core2_sram.bin` (`tools/ns4_dsp_ldr.py --sram`, `tools/adi_dis.py`), the `sharc-fx` oracle. Derived data (committed): [data/fx-mod1-dense.csv](data/fx-mod1-dense.csv) (piano unit, every `kFx1Amt` / `kFx1RateClkDiv` word at 44 values, 2,056 rows), [data/fx-mod2-dense.csv](data/fx-mod2-dense.csv) (types 8, 9 used here). Tools: `tools/fx_mod2_dense.py --slot Fx1`, `tools/fx_mod1_dense_gen.py`, `tools/fx_mod2_dense_gen.py`. Builds on [fx-mod2-validation](fx-mod2-validation.md) (D90), [fx-mod2-types](fx-mod2-types.md), [fx-mod-types](fx-mod-types.md).
- **Question:** (a) do the Mod 1 laws (A-PAN, TREM, RM, A-WAH, WAH, PUMP) stay inside the 11-sample interpolation, and do two ids write the same word; (b) does ENS get amount and rate words from the ARM; (c) what are `B4 / L4 / B7 / L7` when the SHARC dispatcher calls ENS?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Mod 1: 44 values per word

`python tools/fx_mod2_dense.py ... --slot Fx1 --units 3,4,0` re-runs the slot capture with the dense grid (0, 1, 2, 3, every 8th, 60..=80, 126, 127 and the 11 old samples). Worst error of linear interpolation between the old samples, as a share of each word's range (`--analyze`, piano unit):

| Types | Word | Error | Verdict |
|---|---|---|---|
| 0 to 3 A-PAN, TREM | all amount and rate words | 3e-8 | linear (VERIFIED) |
| 4, 5 RM | amount `c[-27]`, `c[-26]` | 3e-8 | linear |
| 4, 5 RM | rate `c[-32]` (phase increment) | **6.4 %** at v = 120 | exponential: 44 knots |
| 6, 7 A-WAH | amount words | 3e-8 | linear |
| 6, 7 A-WAH | rate `c[-20]` | **0.4 %** at v = 71 | 44 knots |
| 8, 9 WAH, 10, 11 PUMP | all | 0 to 3e-8 | linear |

No word is written by both ids with a varying value in any Mod 1 type (the shared addresses listed in the capture are type-switch constants), so there is no joint law. The four series with error get knots (`fx/fw_mod1_dense.rs`); the words at v = 0 and 1 are the 11-sample ones, because the first dense values hit the type-switch default (the ARM sends nothing for an unchanged value).

## 2. ENS has amount and rate words

D95 said the ARM writes no ENS amount or rate word: wrong, the generator did not list types 8, 9. The dense capture [fx-mod2-dense.csv](data/fx-mod2-dense.csv) has, for types 8 and 9: `kFx2Amt` writes `c[-3]` (0.33 to 0.65), `c[2]`, `c[3]` (0 to 0.5) and `c[4]` (0 to 0.4); `kFx2Rate` writes the phase increment `c[-6]` (0x3200 at 0 to 0x13000 at 127). No word is shared. The stage now uses the 44 knots (VERIFIED at the knots, EMPIRICAL between; test `the_ens_laws_hit_every_knot`).

## 3. ENS tap registers (`B4 / L4 / B7 / L7`)

> **Closed by D114 ([fx-ens-tap-registers](fx-ens-tap-registers.md)):** no code writes `L4` / `B4` in either core and none has to; the left voice is a second ring reached by a non-wrapping pre-modify, so the bullets below describe a model bug (a wrapped pre-modify), not a missing register state.

* The routine (`0x123A7B`) sets `L6 = 0x200`, `M6 = 0x202`, `B6 = c[-0x20]`, and reads its taps with `I4 = MODIFY(I6, M3)` followed by `DM(M6, I4)`: the wrap of the tap pointer comes from `I6`, the pre-modify by `0x202` is linear unless `L4` is non-zero.
* No instruction of the FX core 2 listing writes `L4` or `B4` (67,235 lines, `0x120000..0x150000`); `L6`, `L7`, `B6`, `B7` are written only by other routines and by the init (`0x120528`: zero in both register sets).
* The oracle at the dispatcher's first `CALL (M8, I8)` (`sharc-fx --trigger 0xC02C2 --stop-at 0x1217FB --trace`): `I4 = I5 = 0xC03E8`, `L4 = B4 = L7 = B7 = 0`. This is the oracle's reset state (an ASSUMED copy of the hardware's); nothing in the code changes it.
* Ring words: the image row has `c[-32..-27] = 0xB1C12` (six words) and `c[18..23] = 0xB2016` (`0x404` apart: a ring of `0x200` words plus mirror words); the ARM never writes them (boot log and dense capture show no write at those offsets).
* With these registers the left voice reads the unwritten half of the ring and stays 0 (test `the_ensemble_left_voice_needs_circular_tap_registers`); with `B4 = B7 = ring`, `L4 = L7 = 0x200` both voices run. A silent left voice is not credible for a shipped stereo effect, so `FwLawStage::new` keeps the circular taps: **ASSUMED**, opt-in. The hardware's `L4` after reset, or a dispatcher state outside the scanned region, would settle it.

## 3b. The ENS init routine and the loader kernels (second attempt, D109)

* **The init** (`0x123BD0`, the second routine of the module pair `0xC0441558`, [fx-mod2-types](fx-mod2-types.md) s1) reads `c[-0x20]` and `c[-0x1C]` only: `B7 = c[-0x20]`, `L7 = 0x404`, `I7 = c[-0x1C]`, then 50 loops of two `DM(I7, M2) = 0` (a partial clear of the ring from the pointer), stores `I7` back to `c[-0x1C]` and sets `L7 = 0` before `RTS`. It writes **`B7` / `L7` transiently and never `B4` / `L4`**; the ring of the module is `0x404` words (`0x200` ring plus mirror words), the spacing of the image row (`0xB1C12`, `0xB2016`).
* **The init is not what sets the tap registers.** No other routine, the pass body or the library init `0x120528` writes `L4` / `B4` (s3). The ADI loader kernels shipped with CCES (`SHARC/ldr/*/*.asm`) write `L0 = L4 = L8 = L12 = L15 = 0` ("so they can be used without wrap"), so `L4 = 0` is the state a booted image starts from, the oracle's too.
* **Experiments (both negative, not committed):** (1) the SIMD pair of an odd address as `A ^ 1` instead of `A + 1` (an `fw_pe` switch) leaves the `PEx` output exactly 0. (2) With `L4 = 0` the tap read `DM(M6, I4)` falls into the half of the ring that only two mirror words reach; the weights `c[-24..-9]` are non-zero pairs `(w, 1 - w)` after the first call, so the zero left output is not a weight problem either.
* **What would close it** (not available in the harness): a DSP-side run of a real ENS instance, that is the FX core 2 image replayed with the ARM boot script, the ARM's type switch to `kFx2Type` 8 (which installs the routine pointer the dispatcher calls through `CALL (M8, I8)`; every slot is a null stub otherwise, [fx-dsp-map](fx-dsp-map.md) s3) and a pass run, then reading `I4 / L4 / B4` at the routine entry and the left output. The module install is not in the slot capture (it lists parameter words only); finding it needs the ARM type-switch path of the module `0xC0441558`.

## Implications

* Defaults unchanged (D22): `mod1` and `mod2` stay opt-in; `render_hash` untouched.
* Mod 1 types 4 to 7 follow the ARM rate word between the old samples.
* ENS follows the amount and rate words.

## Follow-ups

1. ~~The hardware reset value of `L4` / `B4`~~ not needed (D114); an ENS hardware clip remains open.
2. Per-layer-kind words (D90).
3. The ENS delay words `c[6]`, `c[7]`, `c[10]` (-167.0 frames) are not rescaled to the engine rate.

## Reproduce

```bash
uv run --with unicorn python tools/fx_mod2_dense.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --slot Fx1 --units 3,4,0 --out target/fx1_dense.csv
uv run python tools/fx_mod2_dense.py --analyze target/fx1_dense.csv --export docs/research/data/fx-mod1-dense.csv
uv run python tools/fx_mod1_dense_gen.py && uv run python tools/fx_mod2_dense_gen.py
cargo test -p nord-core --lib fx::fw_mod1
cargo test -p nord-core --lib fx::fw_mod2
```

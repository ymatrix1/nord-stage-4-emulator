# FX ENS tap registers: nothing sets `L4` / `B4`, and nothing has to (the model wrapped a pre-modify)

- **Checked:** 2026-11-15
- **Sources (local, never committed):** the FX DSP image `target/DSP/dsp_image_0_FX.bin` flattened by `tools/ns4_dsp_ldr.py --sram` (`core1_sram.bin`, `core2_sram.bin`, the whole `0x120000..0x1C8000` short-word range, no slice), ADI `elfdump` listings of both cores (`tools/adi_dis.py`, chunks of `0x10000` bytes, 232,686 and 232,576 lines), `target/fw_unpacked/bank0_*.bin` (raw word search), the SHARC PRM (`.reference/adsp-2136x_2137x_214xx_pgr_rev2.4.pdf`, fig 6-5, fig 6-8, the MODE1 table), the oracle `sharc-fx` (`cargo build -p nord-sharc --release`). Builds on [fx-mod2-types](fx-mod2-types.md) s5 / s8, [fx-mod2-validation](fx-mod2-validation.md) s5, [fx-mod1-validation](fx-mod1-validation.md) s3 / s3b, D95, D100, D109.
- **Question:** what sets the ENS (`kFx2Type` 8, 9; routine `0x123A7B`, init `0x123BD0`, module `0xC0441558`) tap registers `B4 / L4 / B7 / L7` in the FX core 2 code, and does PHAS 1 need anything?

## 1. Static search of the full images (VERIFIED, reproducible)

| Search (both cores, `0x120000..0x1C8000`) | Core 1 | Core 2 | Result |
|---|---|---|---|
| raw parcel `0x0FB4` (`L4 = imm`) | 0 | 0 | no immediate write of `L4` |
| raw parcel `0x0FC4` (`B4 = imm`) | 2 (`0x1812B2`, `0x1816BA`) | 2 (`0x181172`, `0x18157A`) | inside the data tables (`0x181000+`: decoded as float-word noise, not code) |
| listing: any `l4` / `b4` token below `0x180000` | 0 | 0 | the first mentions are at `0x180A93` (core 2) / `0x180BD3` (core 1), table data |
| listing: `l7=` / `b7=` writes | many | many | pairs `b7=r1; l7=imm/reg ... l7=0` around ring zero-fills and delay loops; ENS: only the init `0x123BD4..0x123BE9` (`L7 = 0x404`, back to 0) |
| `CALL` / `JUMP` whose target is `0x123A7B` or `0x123BD0` (absolute or `(pc, off)`) | 0 | 0 | the routines are reached only through the dispatcher's `CALL (M8, I8)` |
| raw 32-bit words `0x123A7B`, `0x123BD0` (also `*2`, `0x28000000 \| *2`, `0x28800000 \| *2`) in both cores, `bank0_CODE.bin`, `bank0_SRAM.bin`, `bank0_ARM0.bin`, the FX image | 0 | 0 | the pc reaches the DSP through the ARM stage record (`data/fx-chain-install.csv`: record `0xC0544CE8`, core 2, pc `0x123A7B`) |
| references to `0xC129E`, `0xB1C12`, `0xB2016` as code immediates | 0 | 0 | only data (the instance row; `0xB2016 = 0xB1C12 + 0x404`) |

So no code in either core writes `L4` or `B4`, and the ARM only writes the routine pc and context words (data memory), never a DAG register. One mode switch matters: the library init (`0x120528`, called from the entry `0x1211E3`) runs `BIT SET MODE1 0x1C004F8` at `0x12055E`, which selects the **secondary** DAG banks (MODE1 bits 3 to 6, `SRD1H`, `SRD1L`, `SRD2H`, `SRD2L`) for good (no later `BIT CLR MODE1` of those bits). It initialises `M0..M2`, `M7..M10`, `M15`, `L6`, `L7`, `B6`, `B7` in both banks and never `L4` / `B4`. The register state at the ENS call is therefore the reset state of the secondary bank, which no code of the image changes.

## 2. Nothing has to set them: the left voice is a second ring reached by a pre-modify

Listing `0x123A7B..0x123BCF` (core 2):

* The routine sets `L6 = 0x200`, `M5 = 0x200`, `M6 = 0x202` and `B6 = c[-0x20]` (the ring base). `I6` is the write pointer (`c[-0x1E]`); `M2 = 2` is the stereo pair stride (SIMD: `PEx` at `A`, `PEy` at `A + 1`).
* Per call it copies the first pair of each ring to its guard words: `r0 = dm(i4, m5)` with `I4 = base`, `dm(i4, m2) = r0` (base `+0x200`, `+0x201`), then `dm(i4, m5)` from `base + 0x202` and `dm(i4, m0) = r0` (`base + 0x402`, `+0x403`).
* It writes the right pair at `dm(i6, m2)` (post-modify, wraps in `L6 = 0x200`) and the left pair at **`dm(m6, i6)`**, a **pre-modify**: address `I6 + 0x202`. It reads the taps with `i4 = modify(i6, m3)` (wraps in the ring of `I6`), then `r0 = dm(m6, i4)` (the left tap, `I4 + 0x202`) and `r1 = dm(i7, m1)` (the right tap).
* The buffer is therefore two rings of `0x200` words, each followed by two guard words, `0x202` apart: `0x404` words. That is the `L7 = 0x404` window of the init (`0x123BD0`) and the spacing of the image row (`0xB2016 - 0xB1C12 = 0x404`).
* A pre-modify outputs `I + M` and updates nothing; **it does not wrap** (PRM fig 6-5: the DAG outputs `I+M`, "pre-modify addressing does not change or update the I register"; the oracle `crates/nord-sharc/src/core/moves.rs`, test `pre_modify_with_a_register_does_not_wrap_or_update`, relies on it for the FX reverb core `0x123F61`). The left pair lands in the second ring, the left tap reads it back, and `L4`, `B4`, `B7` play no role. `L4 = 0` is also what the guard copy needs (the `m5 = 0x200` post-modifies through `I4` would wrap with `L4 = 0x200`).

**The bug was in the model.** `Pe::plain` (`crates/nord-core/src/fx/fw_pe_ops.rs`) and the Python `ops()` (`tools/fx_leslie_ops.py`) wrapped the register pre-modify like a post-modify (harness finding 1 of fx-mod2-types s8.1). With the wrap, `dm(m6, i6)` folded the left write onto `I6 + 2` of the first ring, the second ring stayed unwritten, the left tap read zeros and the left output was exactly 0: the symptom D95 explained by assuming circular `L4 = L7 = 0x200`. The assumed taps made the wrapped model run; they are not what the hardware does.

## 3. Evidence (re-run on 2026-11-15 from the current tree)

```bash
python tools/ns4_dsp_ldr.py target/DSP/dsp_image_0_FX.bin --sram SRAM
cargo build -p nord-sharc --release
python tools/fx_models.py check ens  SRAM/core2_sram.bin --scenarios 200   # wrapping model 0/100, plain pre-modify 200/200
python tools/fx_models.py check spin SRAM/core2_sram.bin --scenarios 200   # wrapping model 0/100, plain pre-modify 200/200
for t in vibe flang phas0 phas1 rm wah awah pump; do python tools/fx_models.py check $t SRAM/core2_sram.bin --scenarios 100; done   # 100/100 each, with and without the wrap
```

The oracle runs with `L4 = B4 = L7 = B7 = 0`. The earlier "500/500" of ENS and SPIN (fx-mod2-types s8.1) does not reproduce against the current oracle with the wrapping model. The other eight routines are identical with and without the wrap. `chor4` / `chor5` give 0/100 under `fx_models.py check` both ways (the chorus has its own checker, `tools/fx_model_chorus.py`; not touched here). Rust: `Pe::plain` is a plain add, the ENS and SPIN goldens are regenerated (`python tools/fx_golden_mod2_rest.py`), the test `the_ensemble_left_voice_runs_with_linear_tap_registers` (both peaks above 0.1 with no tap register) replaces the circular-taps test, `Mod2State::circular_taps` is removed.

## 4. PHAS 1

PHAS 1 (`0x1238FC`, module `0xC02BB2C4`, `I1 = 0xB0432`) never had a tap-register dependency: its words come from the boot row (`ROW_PHAS1`), the oracle check is 100/100 with all `L` / `B` at 0.

## 5. What stays open

* Hardware clips for ENS (and PHAS, FLANG, SPIN): the stages are word for word with the oracle, not compared with audio.
* The ENS words `c[6]`, `c[7]`, `c[10] = -167.0` and the ring pointers `c[0x12..0x17]` of the boot row are not written by the ARM in the boot log. The routine does not read `c[0x12..0x17]` (they belong to the second unit's ring); the stage maps both rings into one `0x410`-word array. No capture of registers is needed.
* The reset value of the secondary bank's `L4` / `B4` is not needed: the routine is correct for `L4 = 0` and would corrupt its own guard words otherwise. A hardware trace with `L4 != 0` at the call would contradict this note.

## Implications

* ENS runs on the `FirmwareFx::mod2` path with both voices and no ASSUMED register state; `render_hash` is unchanged (`mod2` is opt-in).
* SPIN's golden and output change too (its second ring is read through a register pre-modify with `L7 = 0x26C`).
* The ENS stage stays opt-in (D22): no hardware clip exists.

## Follow-ups

1. A hardware clip of an ENS program (3 factory programs, [program-coverage](program-coverage.md)) against `render-patch --firmware-fx`.
2. The ENS delay words (`c[6]`, `c[7]`, `c[10] = -167.0` frames) are not rescaled to the engine rate ([fx-mod1-validation](fx-mod1-validation.md) follow-up 3).

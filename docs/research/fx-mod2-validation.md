# FX Mod 2 types: per-type parameter validation (ARM writes, SHARC consumers, `FirmwareFx::mod2`)

- **Checked:** 2026-11-15
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66), `target/settled.snap` (booted ARM emulator), the FX DSP image and `SRAM/core2_sram.bin` (`tools/ns4_dsp_ldr.py --sram`), the `sharc-fx` oracle. Derived data (committed, numeric words the ARM sends, no firmware code): [data/fx-mod2-dense.csv](data/fx-mod2-dense.csv) (piano unit, every `kFx2Amt` / `kFx2Rate` word at 44 values), [data/fx-mod2-joint.csv](data/fx-mod2-joint.csv) (3 rates x 44 amounts for the words both ids write). Tools: `tools/fx_mod2_dense.py`, `tools/fx_mod2_joint.py`, `tools/fx_mod2_dense_gen.py`. Builds on [fx-mod2-types](fx-mod2-types.md) (routines), [fx-chorus-hw-clip](fx-chorus-hw-clip.md) (the CHOR clip), [fx-contexts](fx-contexts.md).
- **Question:** for every Mod 2 type (PHAS, FLANG, CHOR, VIBE, ENS, SPIN, each with its variation), does what the ARM writes for type, rate and amount reach the SHARC routine as `FirmwareFx::mod2` assumes, and does the Rust stage reproduce it?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

The SHARC routines of all six types are ported instruction by instruction and checked against the oracle ([fx-mod2-types](fx-mod2-types.md) s2, s7, s8). This pass checks the **ARM side**: the words behind `kFx2Rate` and `kFx2Amt`, measured on the booted firmware at 44 values (0, 1, 2, 3, every 8th, 60..=80, 126, 127) on the organ, piano and synth units (9,780 messages, `fx_mod2_dense.py`), then the two-id case with 924 more captures.

| Type | ARM words (rate, amount) | Status after this pass |
|---|---|---|
| 4 CHOR | rate `12800 + 4096 v`; mix `min(0.707, 0.0111339 v)`, `(v - 63.5) 0.0111339` clamped to `0.707`; dry `1 - 0.0023071 v`; delays `117 + 39 r`, `171 + 9 r` | closed forms VERIFIED at all 44 values incl. the knee `v = 64..78`; two constants corrected (slope `0.011129` to `0.0111339`, cap `1/sqrt 2` to `0.707`) |
| 5 CHOR var. | rate quadratic; **delay words `c[-0x1C]`, `c[-0x1B]` are written by both ids** | joint closed form (bilinear in rate and amount) VERIFIED to 1.4e-5 frames on 3 x 44; the amount-only fit held only at rate 64 |
| 0 PHAS 0 | `c[-28]` written by both ids | bilinear in the rate, VERIFIED to 1.3e-7 of the range; the old `A R / R(64)` product was wrong off the axes (up to 59 against 7.3) |
| 1 PHAS 1 | rate `c[-32]` | dense knots; no shared word |
| 2, 3 FLANG | `c[-16]` both ids, `c[-15] = -100 - c[-16]` | product `A(a) R(r) / R(64)` VERIFIED on 3 x 44 (9.5e-8 relative); knots dense |
| 6, 7 VIBE | amount `v / 128`, rate `0x3200 + 512 (v^2 >> 2)` | VERIFIED exactly at all 44 values (test) |
| 8, 9 ENS | amount `c[-3]`, `c[2]`, `c[3]`, `c[4]`, rate `c[-6]` (D100; D95 had missed them) | wired with ASSUMED circular tap registers (s5, [fx-mod1-validation](fx-mod1-validation.md) s3) |
| 10, 11 SPIN | rate `c[-16]`, amount words `c[-21]`, ... | dense knots; no shared word |

`FirmwareFx::mod2` stays opt-in ([D90](../DECISIONS.md)): only CHOR has a hardware clip, and it did not close the delay ([fx-chorus-hw-clip](fx-chorus-hw-clip.md)). The ARM plus SHARC check is VERIFIED for the parameter mapping and the algorithm of every wired type.

## 2. Dense capture and what the 11 samples missed

`fx_mod2_dense.py` replaces `fx_clients.value_grid` (11 points) by 44 and re-runs the slot capture. Interpolating linearly between the old 11 samples (the law stage until now) against the dense words, worst error as a share of each word's range (`--analyze`, piano unit):

| Type | Amt | Rate |
|---|---|---|
| 0 PHAS 0 | 1.80 % (knee at 64) | 2.07 % |
| 2, 3 FLANG | **3.86 %** (swing saturates near 88) | 0.40 % |
| 4 CHOR | 0.74 % (knee 63.5) | 0 |
| 6, 7 VIBE | 0.73 % | 0.40 % |
| 10, 11 SPIN | 0.92 % | 0 |

With a 21 point grid (every 8th value, 0..3, 126, 127) as the only knots and every other captured value held out (the original samples 15, 31, ..., 111 and 61..79 among them; `fx_mod2_dense.py --analyze --holdout`) the worst error is 0.69 % (CHOR 4 amount, which uses closed forms), 0.47 % (SPIN amount), 0.21 % (PHAS 0 rate), 0.13 % (FLANG amount) and below 0.1 % for the rest. The law stage interpolates all 44 knots (`fx/fw_mod2_dense.rs`, generated), so between knots it is within these bounds (EMPIRICAL).

**Unit kinds.** The organ and the piano unit write identical laws for PHAS, FLANG, VIBE, SPIN (same words, other base address `I1`); the synth unit equals the piano unit except in CHOR 5 (8 of 14 words), VIBE 6, 7 (8 words) and SPIN 11 (4 words). Organ CHOR has 5 of the 9 piano words. The stage uses the piano words for every layer (ASSUMED, unchanged, OQ21): the per-kind words need the layer kind inside `LayerFxChain` and are left as follow-up.

## 3. Words both ids write (the joint law)

The earlier captures varied one id at the other's default (64). Words written by both ids need the 2-D law. Grid: rate 0, 64, 127 against every dense amount, order independent (rate then amount equals amount then rate: PHAS 0 25 pairs, FLANG 25 pairs x 2 words, identical words):

| Word | Law | Max error |
|---|---|---|
| PHAS 0 `c[-28]` | `f0(a) + (f127(a) - f0(a)) r / 127` (table over amounts) | 1.3e-7 of the range at rate 64 |
| FLANG `c[-16]` | `A(a) R(r) / R(64)` (the old assumption, now VERIFIED) | 9.5e-8 |
| CHOR 5 `c[-0x1C]` | `113 - 0.8503937 a + 0.04724409 r + 0.003348007 a r` | 8e-6 frames |
| CHOR 5 `c[-0x1B]` | `24 + 1.7007874 a - 0.09448819 r - 0.006696014 a r` | 1.4e-5 frames |

Before: the PHAS 0 stage computed `A R / R(64)` (wrong by up to 8x at rate 0 and amount 127), and the CHOR 5 delays ignored the rate (at rate 127 and amount 127 the word is 65.0 frames, the stage had 35.2). Both change the sound of an opt-in path only.

## 4. CHOR constants (clip follow-up)

The 44 CHOR values show the amount law is exact through the knee: `c[-0x1E] = 117 + 39 (v - 63.5) / 63.5`, `c[-0x1D] = 171 + 9 r`, mix `c[-3]` ramps with slope `0.0111339` (not `0.011129`) to `0x3F34FDF4` = 0.7070000, `c[-2] = (v - 63.5) 0.0111339` (word at 64: 0.0055669). The firmware cap is 0.707, not `1/sqrt 2`. These are the words of the hardware clip's type 4; they do not change the delay finding of [fx-chorus-hw-clip](fx-chorus-hw-clip.md) (delay 1.7x, rate and wet gain agree).

## 5. ENS: the left voice (superseded by D114: no tap registers needed, see [fx-ens-tap-registers](fx-ens-tap-registers.md))

> **D114 (2026-11-15):** the circular taps below were a model bug. The harness wrapped the register pre-modify `dm(m6, i6)`; the PRM and the oracle do not wrap it, ENS keeps its left voice in a second ring at `I6 + 0x202`, and the routine is oracle-exact (200/200) at `L4 = L7 = 0`. `Mod2State::circular_taps` is removed. The text below is the history of D95.

ENS (types 8, 9, `0x123A7B`) is oracle-exact as a routine. Wiring needs the left voice. With the boot row its left output is exactly 0 (fx-mod2-types s8.3). Finding, reproduced by the test `the_ensemble_left_voice_needs_circular_tap_registers`: the routine sets `L6 = 0x200` only; the left and right taps are read through `I4` and `I7` (`dm(m6, i4)`, `m6 = 0x202`), whose `B` / `L` the routine never sets, and the writes wrap through `I6`. With `B4 = B7 = ring`, `L4 = L7 = 0x200` the left output runs (peak 1.6 against 0 with the boot row); with the registers at 0 (the oracle scenarios) it stays silent because the left mirror is never written. Whether the hardware dispatcher presets `B4 / L4 / B7 / L7` is not decoded (no `l4=` / `b4=` write in the core 2 listing, 12,616 lines), and no hardware clip has ENS (3 factory programs). `FwLawStage::new` now sets `Mod2State::circular_taps` for ENS (D95). **ASSUMED**; closing it needs the dispatcher's register state at the call (a SHARC trace with a real ENS instance) or a clip.

## 6. Hardware clips

Only CHOR has one (the scope clip, s4 of [fx-chorus-hw-clip](fx-chorus-hw-clip.md)); the Vox clip has a VIBE program with delay and reverb on, not isolated. PHAS, FLANG, ENS and SPIN have no hardware clip, so those four are VERIFIED word for word against the ARM and the oracle, not against audio.

## Implications

* Defaults unchanged (D22): the stages stay in `FirmwareFx::ALL`, not `DEFAULT`; `render_hash` is untouched.
* The parameter mapping is now VERIFIED at 44 values for every wired type (and exactly, via closed forms, for CHOR 4, 5 and VIBE).

## Follow-ups

1. Per-layer-kind words (CHOR 5, VIBE, SPIN 11 on the synth unit; CHOR on the organ unit).
2. ENS: the dispatcher's `B4 / L4 / B7 / L7` (SHARC trace) or a clip.
3. A clip with a dry reference for PHAS / FLANG / SPIN.

## Reproduce

```bash
uv run --with unicorn python tools/fx_mod2_dense.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --units 3,4,0 --out target/fx2_dense.csv
uv run python tools/fx_mod2_dense.py --analyze target/fx2_dense.csv --export docs/research/data/fx-mod2-dense.csv
uv run python tools/fx_mod2_dense.py --analyze target/fx2_dense.csv --holdout
uv run --with unicorn python tools/fx_mod2_joint.py CODE.bin SRAM.bin --cache target/settled.snap --type 0 --words 0xb032c --dense --out target/jd0.csv   # also type 2 (0xb023c,0xb023d), 3, 5 (0xc149c,0xc149d); merge into data/fx-mod2-joint.csv
uv run python tools/fx_mod2_dense_gen.py && uv run python tools/fx_mod2_dense_gen.py tests
cargo test -p nord-core --lib fx::fw_mod2
uv run pytest tools/tests/test_fx_mod2_dense.py
```

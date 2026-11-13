# FX reverb mix, reverb type words, `kCmpResponse` and the Leslie status

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the settled ARM snapshot and `unicorn` harness ([fx-delay-time](fx-delay-time.md) s2, `tools/fx_clients.py`), `capstone` listings of the `kCmpResponse` handler, `docs/research/data/fx-type-words.csv`. Derived data committed: [data/fx-reverb-amt.csv](data/fx-reverb-amt.csv) (128 captured word pairs), [data/fx-reverb-types.csv](data/fx-reverb-types.csv) (the per-type differing words).
- **Question:** what do `kRvbAmt` and the reverb type words write, why does `kCmpResponse` write no value-dependent word, and where is the Leslie?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Item | Result | Tag |
|---|---|---|
| `kRvbAmt` (0..127) | two words per reverb module (`0xC02BB44C` words `-0x1F`, `-0x1E`; the type 0, 1 module `0xC02BB54C` words 18, 19): send `wet = x^2 (2 - x)`, `x = v / 128`, and dry `sqrt(1 - wet^2)`; at `v = 127` exactly `(1, 0)`. Equal power, max abs error 1.2e-7 over all 128 values | words VERIFIED, role EMPIRICAL |
| `kRvbType` words | types 2..11 write 64 words each; only 10 to 21 of them differ from type 2 (s3) | VERIFIED (capture) |
| `kCmpResponse` | the handler (jump table entry reached from `0xC010C990`, gate `0xC010D0AC`, body `0xC0012270`) **ignores the value**: it re-sends the four constants attack `0.102071`, `1.0`, release `4.607e-4`, `0.04` for 0 and 1, in every state tried (fresh, `kCmpEnable` 1, `kCmpAmt` 64, `Edt_Com_kEffectsCmpGlobal` 0 and 1). The 253-instruction trace is identical for both values | EMPIRICAL (4 states x 2 values; the jump-table path never reads the value) |
| Types 0, 1 routines | `0x12417E` / `0x12435C` are a half-rate wrapper (fs/4 Butterworth banks, hold, dry/wet mix) around the core `0x123F61`; wrapper bit-exact (6000 of 6000 words), core not modelled (s5) | wrapper VERIFIED, core structure EMPIRICAL |
| Leslie | no code record in the 100 FX routines; the rotor runs from the ARM tick state machine and the ORG DSP image. Only `kEffectsLeslieDrv` (`v / 127`, words 3 and 7 of its row) is mapped | open (not attempted beyond [fx-contexts](fx-contexts.md) s6) |

## 2. Reverb amount

Reproduce (`tools/fx_clients.py` unit L3, one dispatch per value, about 5 s each):

```bash
uv run --with unicorn python tools/fx_clients.py CODE.bin SRAM.bin --cache target/settled.snap --units 3 --slots rvb.csv --slot Rvb
uv run pytest tools/tests/test_fx_reverb_amt.py        # closed form against fx-reverb-amt.csv
```

Reading of the series: `wet / x^2 = 2 - x` exactly at `v = 8, 16, 64, 96` (`x = v/128`: `31/16`, `15/8`, `3/2`, `5/4`), and `wet^2 + dry^2 = 1.00000` for every value. Rust: `fw_map::reverb_send` / `reverb_dry`, wired as `FirmwareFx::reverb_mix` (the dry and send of the engine reverb; the engine's own taps, decay and damping stay). Default off.

## 3. Reverb type words (types 2..11)

The block at DM `0x9500A..0x95033` (fixed point init data: `0x80000000`, `0x7FFFFFFE`, `-2.0 / 2.0` pairs) is the same for every type except the words below (types 0 and 1 write module `0xC02BB54C` instead); `0xB0F..` and `0xB1C26..` are the module state. Data: `fx-reverb-types.csv`.

| Type | `0x95013` / `0x95024` | `0xB0F0D` | `0xB0F1D` | `0xB0F20` | `0xB0F21` | `0xB0F59` / `0xB0F69` | `0xB0F5C` | `0xB1C26` |
|---|---|---|---|---|---|---|---|---|
| 2 | 262 | 0.21 | 0.1758 | 0.1258 | 0.7 | 98957 | 34.72 | 2.385 |
| 3 | 1324 | 0.21 | 0.1758 | 0.1258 | 0.7 | 98957 | 34.72 | 2.385 |
| 4 | 270 | 0.21 | 0.2629 | 0.1538 | 0.7 | 98957 | 34.72 | 1.95 |
| 5 | 1766 | 0.21 | 0.2629 | 0.1538 | 0.7 | 98957 | 34.72 | 1.95 |
| 7 | 278 | 0.28 | 0.5557 | 0.2982 | 0.6 | 197914 | 138.9 | 0.8049 |
| 8 | 920 | 0.35 | 0.7091 | 0.4211 | 0.5 | 98957 | 34.72 | 0.475 |
| 9 | 920 | 0.35 | 0.7091 | 0.4211 | 0.5 | 197914 | 138.9 | 0.475 |
| 10 | 1788 | 0.35 | 0.8688 | 0.4661 | 0.5 | 98957 | 34.72 | 0.3755 |
| 11 | 1788 | 0.35 | 0.8688 | 0.4661 | 0.5 | 197914 | 138.9 | 0.3755 |

Structure (EMPIRICAL, positions only): the types come in pairs of two (2 and 3, 4 and 5, 8 to 11 share the `0xB0F1D`, `0xB0F20`, `0xB1C26` voicing) and differ in a single length word (`0x95013`, `0x95024`: 262 against 1324, 270 against 1766, 920, 1788) and the LFO-like pair `0xB0F59` (98957 or 197914) / `0xB0F5C` (34.72 or 138.9, a 4:1 ratio). The 0.21, 0.28, 0.35 words rise with the size of the room, `0xB0F21` falls 0.7, 0.6, 0.5. Type 6 sends nothing (the default). The roles (decay, damping, lengths) are **not verified**: that needs an impulse response of the multi-tap routines `0x12314A`, `0x12329B`, `0x123ABE`, `0x12417E`, `0x12435C` in the oracle.

## 4. `kCmpResponse`

Dispatch trace of `Edt_Effects_kCmpResponse` (id `0x1AB`): `0xC00CC4F0` dispatch, `0xC010C990` jump table (`id - 0x18A` entries), `0xC010D0AC` (`cmp r3, #0; beq` to `0xC010D78C`), `0xC0012270` (the compressor setter: it loads the effect slot, then sends four words through the message queue in the block `0xC0012588`) with `r3 = 0` for both values. The second path (`0xC01F5CC0`) that would use a non-zero gate was never entered. So `kCmpResponse` has no DSP effect in OS v1.66 in the settled state: the engine's `comp_fast` keeps its legacy meaning, and the compressor words stay `CMP_ATTACK` / `CMP_RELEASE`.

## 5. Reverb routines of types 0 and 1 (Spring): wrapper modelled, core not

Checked 2026-11-13 with `tools/adi_dis.py` on the FX Core 2 image (`ns4_dsp_ldr.py --sram`) and the `sharc-fx` oracle. The earlier reading "five multi-tap routines" does not hold for `0x12417E` / `0x12435C` (types 0, 1 are SPRING in the UI, [effects-and-routing](../specs/effects-and-routing.md) s3): they are **a sample-rate wrapper around one core routine, `0x123F61`**, called through `I8` (`call (m8,i8)`, `i8 = 0x123F61` in both). `0x12417E` is 0xDE parcels to the call and returns at `+0x1DD`; `0x12435C` (`+0x1DE`..`+0x1F5`) is the 2-frame variant; the rest up to `+0x275` is the clear routine of the same module (zeroes the three buffers and the state words, reads `emuclk`).

| Piece | What it does | Status |
|---|---|---|
| `0x12417E` bank 1 | the three fs/4 Butterworth stages of `0x120FDB` (PM `0x9437A`, `b0 b1 b2 a1 a2` = `0x3E95F619 0x3F15F619 0x3E95F612 0 0x3E2FB0D5`) over the 4 input frames (`I0`), states `s1L s1R s2L s2R` per stage at `I1 + 0x49..0x54`, result in `I1 + 0x41..0x48` | VERIFIED word for word |
| core call | frames 0 and 2 of the low-passed block (`R0`, `R4`) go to the core, which returns two frames in `R12`, `R14`: the core runs at half rate (2 frames per 4) | VERIFIED (trace) |
| hold + bank 2 | `w0 w0 w2 w2` stored at `I1 + 0x41`, the same cascade (PM `0x9437F`, identical words), states at `I1 + 0x55..0x60` | VERIFIED word for word |
| dry / wet mix | `out = in * c[0x15] + z`, product a register value, sum truncated on store | VERIFIED word for word; `c[0x15]` is **rewritten by the core** (the oracle read 0.1430 after a call that started with 0.1344), so the model takes the value loaded after the call |
| `0x12435C` | the core on frames 0, 1 directly (`R0`, `R4`), then `out = in * c[0x15] + (R12, R14)` | VERIFIED (post-call segment, 8 runs x 3 calls, 96 of 96 words) |

Tests: `tools/fx_reverb_model.py check4` (`0x12417E` end to end with the core's outputs read from the oracle trace: 5 seeds x 30 carried calls x 40 words = 6000 of 6000 words identical; an underflowing value flushes to zero, the sign of that zero is not modelled) and `check2`; `tools/tests/test_fx_reverb_model.py` (no image); Rust `fx/fw_rvb_wrap.rs` (`FwRvbWrap`, `fw_rvb_mix2`) against generated golden words (`tests_rvb_wrap_golden.rs`, ext40 reference exact, f32 path below 2e-6).

### The core `0x123F61` (231 parcels, structure only)

Read from the listing and from oracle runs (EMPIRICAL; not a model):

* `c[0x12]` is smoothed into `c[0x14]` with the pole `c[0x11]` (`c[0x14] = c[0x14] c[0x11] + c[0x12] (1 - c[0x11])`; `c[0x12]` is the `kRvbAmt` send word, s2). `c[0x16]` and `c[-6]` scale the output, which is the **previous** call's result (`c[-0x20]`, `c[-0x1E]`), a one call latency.
* Two input frames are averaged `(f0 + f1) / 2`, written into a circular line A (`b2 l2 i2 m4` from `c[0x27] c[0x28] c[0x29] c[0x2B]`) and read back at the tap offset `c[0x2B] = -2658` words (type words, both types); a second line B (`c[0x2C..0x30]`, tap `c[0x30] = -4068`).
* A loop `lcntr = c[-0x12]` (**21 passes, type word `0x15`**) of 15 instructions runs an 8-multiply two-section filter per pass on PM coefficients (`I10 = c[-0xC]`).
* Four Schroeder all-pass sections share one circular buffer (`b6 = c[0x32]`, length `l6 = c[0x20]`): each reads two taps at the signed word offsets `c[0x18 + 2k]` (left) and `c[0x19 + 2k]` (right), computes `v = g x_tap + ...` with gains `c[0x26]`, `c[0x25]` and writes back. The type words (identical for types 0 and 1):

| Section | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| tap offset L / R (words) | -228 / -244 | -784 / -820 | -1224 / -1248 | -1568 / -1568 |
| at the core rate fs / 2 (2 words per stereo sample) | 114 / 122 | 392 / 410 | 612 / 624 | 784 / 784 samples |
| in ms at 48 kHz (core rate 24 kHz) | 4.75 / 5.08 | 16.3 / 17.1 | 25.5 / 26.0 | 32.7 / 32.7 |

  Types 0 and 1 differ in the coefficient words `c[1..16]` and `c[-1]`, `c[-11]`, `c[-15]`, `c[-17]`, and in the gains `c[0x25]` (`0xBF274E9D` = -0.6535 against `0xBF2F5EBD` = -0.6850) and `c[0x26]` (`0x3F274E9D` = +0.6535 against `0x3F2F5EBD` = +0.6850).

A chain of dispersive all-passes at half rate is what a spring reverb needs; this is the first firmware evidence for the SPRING mapping of types 0, 1 (ASSUMED until a recording is compared).

An impulse through the oracle with **synthetic** buffer pointers (the lengths and base words are set by the ARM at module creation and are unknown) shows the structure but not the real decay: with 4096 / 8192 / 4096 word buffers and the type 0 words the first wet output comes at output sample 5441 (113 ms), then a band-limited oscillation of period 8..11 samples; the right channel stays silent because the pointer set is incomplete. This is not a measurement of the hardware reverb.

## 6. Not done (stop point)

| Item | State |
|---|---|
| Core `0x123F61` bit-exact model | not done: 231 parcels with five circular-buffer pointer files (`B2/L2/I2/M4`, `B6/L6/I6`), a PM coefficient list read through `I10`, a register-count loop and dual-PE address arithmetic (`m4 = s1` takes a PEy value); `fx_pe.py` has no `B/L` wrap yet. Needs the real pointer and length words (ARM module constructor `0xC02BB54C`), which are not in `fx-type-words.csv` |
| Types 2..11 (module with no state row) and the Dly feedback pair `0x123ABE` / `0x12314A` / `0x12329B` | not analysed; their code is not these wrappers |
| End-to-end reverb type, `emu_vs_hw.py` reverb segment, spectral / decay gap | not possible without the core; no segment added |
| Leslie / rotary | not started |

## Implications

- The reverb dry / send law is exact; it is the only part of the reverb the Rust engine can follow the firmware on now.
- `kCmpResponse` is closed: no word depends on it.

## Follow-ups

1. Capture the pointer and length words of the type 0 instance (ARM harness, `0xC02BB54C` after the module constructor) and add `B/L` circular addressing to `fx_pe.py`; then transliterate the core `0x123F61` (the wrapper is done).
2. Check the SPRING reading: compare the all-pass taps of s5 against a spring recording (`emu_vs_hw.py`).
3. Types 2..11 (`0x9500A..` words, s3): find their routines; they are not `0x12417E` / `0x12435C`.
4. Leslie: capture the rotor words over many ARM ticks per `kEffectsLeslieSpeed` value.

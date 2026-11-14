# FX reverb mix, reverb type words, `kCmpResponse` and the Leslie status

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the boot log (`tools/arm_bulk_tables.py capture`), the FX images (`ns4_dsp_images.py`, `ns4_dsp_ldr.py --sram`), `sharc-fx --trace`, `sharc-visa` listings, the settled ARM snapshot and `unicorn` harness ([fx-delay-time](fx-delay-time.md) s2, `tools/fx_clients.py`), `capstone` listings of the `kCmpResponse` handler, `docs/research/data/fx-type-words.csv`. Derived data committed: [data/fx-reverb-amt.csv](data/fx-reverb-amt.csv) (128 captured word pairs), [data/fx-reverb-types.csv](data/fx-reverb-types.csv) (the per-type differing words).
- **Question:** what do `kRvbAmt` and the reverb type words write, why does `kCmpResponse` write no value-dependent word, and where is the Leslie?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Item | Result | Tag |
|---|---|---|
| `kRvbAmt` (0..127) | two words per reverb module (`0xC02BB44C` words `-0x1F`, `-0x1E`; the type 0, 1 module `0xC02BB54C` words 18, 19): send `wet = x^2 (2 - x)`, `x = v / 128`, and dry `sqrt(1 - wet^2)`; at `v = 127` exactly `(1, 0)`. Equal power, max abs error 1.2e-7 over all 128 values | words VERIFIED, role EMPIRICAL |
| `kRvbType` words | types 2..11 write 64 words each; only 10 to 21 of them differ from type 2 (s3) | VERIFIED (capture) |
| `kCmpResponse` | the handler (jump table entry reached from `0xC010C990`, gate `0xC010D0AC`, body `0xC0012270`) **ignores the value**: it re-sends the four constants attack `0.102071`, `1.0`, release `4.607e-4`, `0.04` for 0 and 1, in every state tried (fresh, `kCmpEnable` 1, `kCmpAmt` 64, `Edt_Com_kEffectsCmpGlobal` 0 and 1). The 253-instruction trace is identical for both values | EMPIRICAL (4 states x 2 values; the jump-table path never reads the value) |
| Types 0, 1 routines | `0x12417E` / `0x12435C` are a half-rate wrapper (fs/4 Butterworth banks, hold, dry/wet mix) around the core `0x123F61`; wrapper bit-exact (6000 of 6000 words), core modelled instruction by instruction against the oracle (s6); its pointer words and PM coefficient list are not found (s6, s7) | wrapper and core code VERIFIED, data open |
| Leslie | no code record in the 100 FX routines; the rotor runs from the ARM tick state machine and the ORG DSP image. Only `kEffectsLeslieDrv` (`v / 127`, words 3 and 7 of its row) is mapped | open; superseded by [fx-leslie](fx-leslie.md): the boot installs `0x1237F8` in the Leslie row and the ARM sends rotor speed words |

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

## 5. Reverb routines of types 0 and 1 (Spring): wrapper and core code modelled

Checked 2026-11-13 with `tools/adi_dis.py` on the FX Core 2 image (`ns4_dsp_ldr.py --sram`) and the `sharc-fx` oracle. The earlier reading "five multi-tap routines" does not hold for `0x12417E` / `0x12435C` (types 0, 1 are SPRING in the UI, [effects-and-routing](../specs/effects-and-routing.md) s3): they are **a sample-rate wrapper around one core routine, `0x123F61`**, called through `I8` (`call (m8,i8)`, `i8 = 0x123F61` in both). `0x12417E` is 0xDE parcels to the call and returns at `+0x1DD`; `0x12435C` (`+0x1DE`..`+0x1F5`) is the 2-frame variant; the rest up to `+0x275` is the clear routine of the same module (zeroes the three buffers and the state words, reads `emuclk`).

| Piece | What it does | Status |
|---|---|---|
| `0x12417E` bank 1 | the three fs/4 Butterworth stages of `0x120FDB` (PM `0x9437A`, `b0 b1 b2 a1 a2` = `0x3E95F619 0x3F15F619 0x3E95F612 0 0x3E2FB0D5`) over the 4 input frames (`I0`), states `s1L s1R s2L s2R` per stage at `I1 + 0x49..0x54`, result in `I1 + 0x41..0x48` | VERIFIED word for word |
| core call | frames 0 and 2 of the low-passed block (`R0`, `R4`) go to the core, which returns two frames in `R12`, `R14`: the core runs at half rate (2 frames per 4) | VERIFIED (trace) |
| hold + bank 2 | `w0 w0 w2 w2` stored at `I1 + 0x41`, the same cascade (PM `0x9437F`, identical words), states at `I1 + 0x55..0x60` | VERIFIED word for word |
| dry / wet mix | `out = in * c[0x15] + z`, product a register value, sum truncated on store | VERIFIED word for word; `c[0x15]` is **rewritten by the core** (the oracle read 0.1430 after a call that started with 0.1344), so the model takes the value loaded after the call |
| `0x12435C` | the core on frames 0, 1 directly (`R0`, `R4`), then `out = in * c[0x15] + (R12, R14)` | VERIFIED (post-call segment, 8 runs x 3 calls, 96 of 96 words) |

Tests: `tools/fx_reverb_model.py check4` (`0x12417E` end to end with the core's outputs read from the oracle trace: 5 seeds x 30 carried calls x 40 words = 6000 of 6000 words identical; an underflowing value flushes to zero, the sign of that zero is not modelled) and `check2`; `tools/tests/test_fx_reverb_model.py` (no image); Rust `fx/fw_rvb_wrap.rs` (`FwRvbWrap`, `fw_rvb_mix2`) against generated golden words (`tests_rvb_wrap_golden.rs`, ext40 reference exact, f32 path below 2e-6).

### The core `0x123F61` (structure; the instruction-level model is s6)

Read from the listing and from oracle runs (EMPIRICAL):

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

## 6. Core `0x123F61`: instruction-level model (2026-11-13)

`tools/fx_model_rvbcore.py` is a transliteration of all 230 instructions of the core (one `Machine.step` per executed instruction, 17 of them in the 21-pass loop body `_body`), produced from the `sharc-visa` listing of the FX Core 2 image. `tools/fx_rvbcore_check.py check` runs the 2-frame wrapper `0x12435C` (which calls the core) in `sharc-fx --trace` and compares **every register and DAG register before every instruction** plus every written memory word with the model:

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_0_FX.bin --sram SRAM
python tools/fx_rvbcore_check.py check SRAM/core2_sram.bin --calls 40 --seed 3           # real line lengths
python tools/fx_rvbcore_check.py check SRAM/core2_sram.bin --calls 170 --seed 5 --short  # data reaches every stage
uv run pytest tools/tests/test_fx_rvbcore.py      # pinned golden words, no image needed
```

| Run | Result |
|---|---|
| 40 calls, seed 3, type 0 words, lines 2658 / 4068 words | 9360 of 9360 written words identical, every instruction's registers equal |
| 60 calls, seed 2, `--short` (lines 16 / 24 words, data reaches the loop) | 14040 of 14040 identical |
| 170 calls, seed 5, `--short` (data reaches the all-pass taps, first tap 114 calls) | 39780 of 39780 identical, every instruction's registers equal |

What the oracle taught beyond the listing text (VERIFIED by the trace comparison):

* `[pre-mod]` in the listing is the **pre-modify without write-back**: `R9 = DM(I5, M6) [pre-mod]` reads `I5 + M6` and leaves `I5` unchanged. A plain `DM(Ii, Mm)` is post-modify.
* A write to `Bn` also loads `In`; post-modify, pre-modify and `MODIFY` honour `Bn` / `Ln` (`fx_pe.dag_next`, the rule of `crates/nord-sharc/src/core/dag.rs`: a non-negative step reaching `B + L` subtracts `L`, a negative step below `B` adds `L`, one wrap only).
* A float result below the smallest normal number flushes to zero (the loop reaches it); the sign of a zero differs between oracle and `Fraction` arithmetic and is not modelled (the trace compare treats `+0` and `-0` as equal).
* `M4 = S1` takes the PEy element, `R3 <-> S2` swaps `PEx.R3` with `PEy.S2`, `S1 = R0` swaps the pair (as in [fx-routine-models](fx-routine-models.md)); `M10 = 2` (the PM list is read as pairs).

Dataflow (EMPIRICAL, read from the model): the two input frames are averaged per channel and the pair is written to line A (`DM(I2, M2)`, circular, one pair = 2 words per call) after adding `c[-11] * c[-28]` (a state word); line A is read at `c[43]` words behind the write pointer; a loop of `c[-18] = 21` passes runs the 8-multiply two-section filter on pairs read from the PM list at `I10 = c[-12]` (4 words per pass), with the state words `c[-26 .. -20]`; line B (`c[44 .. 48]`) is written and read the same way; four all-pass sections share the buffer `B6 = c[50]`, `L6 = c[0x20]`: each reads the pair at the left tap (`DM(I4, M0)`) and the pair `L6` words further at the right tap (`DM(I5, M6)`, so the buffer holds **two copies of length `L6`**) and writes both; `R12`, `R14` of one call are the results of the previous call (one call latency) and the smoothed gain `c[0x15]` is rewritten.

### Pointer and length words (first pass: not found; found in the image, s8)

| Word | Meaning in the core | Source |
|---|---|---|
| `c[0x20] = 1570` | all-pass buffer length `L6` (largest tap 1568 + 2) | VERIFIED: boot state of the Rvb instances at `0xB2439`, `0xB2573`, `0xB26AD`, `0xB27E7`, `0xB2921` (`tools/arm_bulk_tables.py capture`, `fx_core1.script`) |
| `c[18]`, `c[19]` | `kRvbAmt` send and dry | VERIFIED (s2) |
| `c[0x27 .. 0x29]`, `c[0x2B]`; `c[0x2C .. 0x2E]`, `c[0x30]` | line A base, length, write pointer, tap; line B | **not in the boot log** (the ARM sends 39 words per instance, none of these) and **zero in the DSP image** |
| `c[0x32]`, `c[0x34]` | all-pass base, write pointer | same |
| `c[-13]`, `c[-12]`, `c[0x11]`, `c[0x16]`, `c[0x17]` | loop state array, PM coefficient list, smoothing pole, output scales | same; the list at `I10` is not in the image's PM data (`0x94378..` holds only the Butterworth banks) |

The tap words fix the **shape** of the lines: with `L = |tap|` (ASSUMED) each is a pure delay of 2658 and 4068 words (1329 and 2034 calls, 111 and 169 ms at 24 kHz if one call is one stereo pair). The model needs a pointer layout, not the firmware's addresses (`layout()` in the check tool uses regions at `0xB5000 .. 0xB9000` and a random coefficient list: the check proves the routine, not the data). **The 84-word coefficient list is the open item**: without it the decay and spectrum of the type 0, 1 reverb cannot be reproduced, so no impulse response or decay time is quoted.

## 7. Not done (stop point of the first pass; superseded by s8, s9)

| Item | State |
|---|---|
| Real pointer words and the PM coefficient list `c[-12]` | not found: not in the 96,111-message boot log, not in the FX image DM / PM. They come from an ARM path the boot capture does not run (reverb enable / module activation, [fx-contexts](fx-contexts.md) s7) or from a table `0xC02BB54C`'s constructor copies; next: hook the DSP-message writer while the harness enables the reverb |
| Impulse and noise responses with the real context | blocked by the list above |
| Rust port (`fx/`, opt-in `FirmwareFx::reverb`) | not started: the model (230 instructions) is the specification, but a port without the real list would only reproduce the routine on synthetic data |
| `emu_vs_hw.py` reverb segment, decay time, spectral gap | not possible without the list |
| Types 2..11 (`0x123ABE`, `0x12314A`, `0x12329B`, `0x12417E` variants) | not started; the listing-to-steps generator handled every instruction form of the core and applies to them |
| Leslie / rotary | not started |

## 8. The real instance, its data and the responses of types 0 and 1 (second pass)

**Where the data was.** s6 looked at the organ context `0xB0CAE` (all zero) and at the boot log. The pointer, length and state words are **static initial DM of the FX DSP image itself**, for the one instance `0xB27E8` in both cores ([data/fx-reverb-instance.csv](data/fx-reverb-instance.csv), 46 words per core; the cores differ only in the SDRAM bases). The ARM adds only the 39 type words and the `kRvbAmt` pair; the boot log never needed to carry the rest. No ARM activation path has to be driven.

| Word | Value | Meaning |
|---|---|---|
| `c[0x11]` | `0x3F7DC91D` = 0.99132 | smoothing pole of `c[0x12..0x15]` |
| `c[0x16]`, `c[0x17]` | 1.0, 1.0 | output scales |
| `c[0x20]`, `c[0x33]` | 1570, 3140 | all-pass length `L6`, two copies |
| `c[0x27..0x29]`, `c[0x2B]` | `0x08013000` (core 1) / `0x0802F000` (core 2), 5000, = base, tap | line A: base, length 5000 words, write pointer, tap `-2500` in the image, `-2658` after the type 0 words |
| `c[0x2C..0x2E]`, `c[0x30]` | `0x08014388` / `0x08030388`, 5000, = base, tap | line B (tap `-3000` in the image, `-4068` after the type words) |
| `c[0x32]`, `c[0x34]` | `0x000E24E7`, = base | all-pass buffer (3140 words) |
| `c[-13]` | `0x000B2849` | loop state array (21 passes) |
| `c[-12]` | `0x0009454B` | PM list read by **dead loads** (below) |

The delay of a line is the tap, not the length: `|tap|` words behind the write pointer in a 5000 word ring, one pair (2 words) per call, one call per 4 frames at 48 kHz: line A 1329 pairs = 111 ms, line B 2034 pairs = 170 ms (type 0). The first wet output of the impulse response appears at output frame 5408 (call 1352), as expected.

**The PM list needs no data.** The loop's two `PM(I10, M10)` loads write `R6`, `R7`, which the next pass overwrites before reading; the last pass's values are overwritten by `F6 = PASS F2`. The model gives identical outputs for a zero and for a random list (`tools/fx_rvbcore_check.py`, 150 calls). The list at `0x9454B` is zero in the image and is never sent; that is harmless.

**An oracle fix found on the way.** With the real data the oracle's reverb was unstable (+85 dB/s growth). The cause was the register **pre-modify with `L`**: the all-pass stores `DM(I6, M0) [pre-mod]` and `DM(I6, M6) [pre-mod] = R10` with `M6 = L6`. The oracle folded the pre-modified address back by `L6`, so both stores landed at `I6` (the second overwrote the first) and the second copy (`I5 + L6`, read with the same form) stayed empty. The firmware clearly keeps two copies `L6` words apart (`c[0x33] = 2 c[0x20]`, the clear routine zeroes 3140 words), and the immediate pre-modify form `DM(imm, Ii)` of the oracle never wrapped. `crates/nord-sharc/src/core/moves.rs` now treats `DM(Ii, Mm) [pre-mod]` as `I + M` without wrap and without write-back (test `pre_modify_with_a_register_does_not_wrap_or_update`; the 283 oracle tests still pass). The model `tools/fx_model_rvbcore.py` follows (`plain`). EMPIRICAL: evidence is the design (two copies, 3140 words) and the stable, decaying response; no hardware measurement of the wrap itself.

**Model against oracle on the real layout** (`tools/fx_rvbreal.py check`, the 4-frame wrapper `0x12417E` end to end, 8 output words per call, noise input; the core outputs reach the output after call 1352): type 0, 2000 calls, 16000 of 16000 words identical; type 1, 2000 calls, 16000 of 16000. The instruction-level check of s6 with the fixed oracle: 9440 of 9440 words (40 calls, `--short`).

**Responses** (`tools/fx_rvbreal.py responses`, oracle, `kRvbAmt` 127 = wet only, 3 s of impulse and of white noise at 48 kHz, left channel; T60 from the -5 to -35 dB Schroeder slope, extrapolated beyond the 3 s window):

| Type | T60 | EDT | Echo density at 0.1 / 0.2 / 0.4 / 0.8 s (1 = gaussian) | Wet-to-input per band, 60-250, 250-500, 0.5-1k, 1-2k, 2-4k, 4-8k, 8-16k Hz (dB) |
|---|---|---|---|---|
| 0 | 4.3 s | 3.2 s | 0.73 / 0.54 / 1.00 / 0.99 | -7.1, -4.0, -3.6, -5.3, -11.7, -17.5, -27.7 |
| 1 | 5.4 s | 5.1 s | 0.76 / 0.94 / 0.98 / 0.93 | -6.3, -2.6, -2.8, -4.9, -4.3, -6.0, -15.3 |

Reading (EMPIRICAL): both types are dense after 0.4 s; type 0 is darker (-18 dB at 4-8 kHz against -6 dB) and decays faster, type 1 is brighter and longer. These are numbers of the firmware routine at `kRvbAmt` 127 with no `kRvbColor` or type-specific tail words beyond the 39 captured ones; a SPRING reverb of 4 to 5 s is long, so the name mapping stays ASSUMED until a recording is compared.

## 9. Not done (stop point of the second pass; superseded by s10)

| Item | State then | State now |
|---|---|---|
| Rust port behind `FirmwareFx::reverb` | not started | done, s10 |
| `emu_vs_hw.py` reverb segment, decay and spectral gap against a hardware clip | not started | done, s10 (decay not measurable from the clips) |
| Types 2..11 (`0x123ABE`, `0x12314A`, `0x12329B`) | not started | not started; `tools/fx_rvbcore_gen.py` generates the steps from any `tools/fx_model_*.py` of the same shape once the model of those routines exists |
| ARM activation path | not needed for types 0 and 1 (s8) | unchanged |

## 10. The Rust port of types 0 and 1 (third pass, 2026-11-13)

**What exists.** `FirmwareFx::reverb` (opt-in, `ns4 render-patch --firmware-reverb`) runs the routine `0x12417E` for `ReverbModel::Spring` (variation flag: type 1) and leaves every other reverb on the engine FDN.

| File | Role |
|---|---|
| `tools/fx_rvbcore_gen.py` | reads the syntax tree of `tools/fx_model_rvbcore.py` (no listing, no firmware bytes) and writes `fx/fw_rvb_steps.rs` (the 215 straight-line instructions plus the 16 of the loop pass, one line each, effects committed after all right-hand sides are read) and `fx/fw_rvb_words.rs` (the derived words of [data/fx-reverb-instance.csv](data/fx-reverb-instance.csv) core 2 and the 39 type words of types 0, 1 from [data/fx-type-words.csv](data/fx-type-words.csv); the five firmware pointers become region tags). `--check` and `tools/tests/test_fx_rvbcore_gen.py` fail on a stale file |
| `fx/fw_rvb_core.rs` | register-pair machine (words or floats, `B`/`L` circular buffers, `BDCST1` loads) over one pre-allocated word array: context, state array, PM list, 3140 word all-pass buffer, two 5008 word lines (14.9 k words); no allocation after `new` |
| `fx/fw_ext40.rs` | `Fp` trait: `E40` (32 significant bits, ties to even, exact integer mantissa arithmetic, store truncates to 24 bits, flush below 2^-126) and plain `f32` |
| `fx/fw_rvb_wrap.rs` | the half-rate wrapper made generic over `Fp` (`FwRvbWrap` stays the `f32` type) |
| `fx/fw_reverb.rs` | `Reverb<F>` / `FwReverb` (E40) / `FwReverbF32`: block of 4 frames or frame by frame with **4 frames of latency**; `set_amount` maps `kRvbAmt` through `fw_map::reverb_send` / `reverb_dry` into `c[0x12]`, `c[0x13]` (the core smooths them into `c[0x14]`, `c[0x15]`) |

**Golden vectors** (`tools/fx_rvbcore_golden.py` runs `0x12417E` in the `sharc-fx` oracle on the real instance, 3000 calls, 24000 words per case; `tests_fw_rvb_golden.rs`):

| Case | Oracle first output | Words | E40 port |
|---|---|---|---|
| type 0, `kRvbAmt` 127, impulse | call 1335 | 24000 | every word equal (FNV-1a hash, non-zero count, first call, 6 sample calls) |
| type 0, `kRvbAmt` 64, noise | call 0 | 24000 | every word equal |
| type 1, 127, impulse | call 1333 | 24000 | every word equal |
| type 1, 64, noise | call 0 | 24000 | every word equal |

So the match is **exact (0 differing words, sign of zero aside)** on the 40-bit path. The `f32` path (plain single precision, no store truncation) keeps the data flow and differs by 95 dB (impulse) to 120 dB (noise) below the signal (SNR of the 3000-call runs, worst absolute error 9e-7); both paths are kept, `FwReverb` (E40) is what the chain uses (at least 7 times faster than real time in a release build: the golden tests run 12 cases of 0.25 s of audio, 8 of them on the 40-bit path, in 0.4 s). Sign of a flushed underflow is not modelled (compared as zero, as in `fx_reverb_model.py`). The wrapper alone on the 40-bit path also reproduces the earlier wrapper golden words (`tests_rvb_wrap_golden.rs` words, 64 + 24 state words).

**Against the video** (`tools/emu_vs_hw.py`, segments `fx_reverb` = A:74 B3 Old Forest, organ, Spring type 0, `kRvbAmt` 0.62 (the highest Spring amount of the Part A programs), `organ_b3` = A:14 (type 0, 0.22) and `fx_amp` = A:64 (type 0, 0.20); new variants `fw-reverb` = `--firmware-reverb`). Score = mean |band delta| in dB over the five bands below 8 kHz, emulator minus hardware clip; lossy YouTube audio, a live performance: shape evidence only.

| Segment | Variant | 400-1500 Hz | 1.5-3 kHz | 3-8 kHz | Stereo width hw / emu | Score |
|---|---|---|---|---|---|---|
| fx_reverb | engine FDN | -4.1 | +8.7 | +9.6 | 0.25 / 0.21 | 5.84 |
| fx_reverb | firmware | -2.4 | +8.3 | +8.0 | 0.25 / 0.13 | 5.09 |
| organ_b3 | engine FDN | +2.2 | -1.9 | +1.2 | 0.48 / 0.35 | 4.36 |
| organ_b3 | firmware | +2.3 | -1.8 | +1.5 | 0.48 / 0.34 | 4.43 |
| fx_amp | engine FDN | -1.7 | +3.2 | +10.7 | 0.09 / 0.07 | 4.75 |
| fx_amp | firmware | -1.7 | +3.2 | +10.7 | 0.09 / 0.06 | 4.70 |

* **Spectrum.** The firmware reverb improves the clip with the high amount (score 5.84 to 5.09: low and mid bands 0.9 to 1.7 dB closer, 3 to 8 kHz 1.6 dB closer) and is neutral at the low amounts (4.36 to 4.43, 4.75 to 4.70). The dominant gaps (+8 to +10 dB above 1.5 kHz, +14 dB RMS) are not the reverb: the same in both variants.
* **Stereo width.** The firmware reverb is narrower than the hardware in every clip (0.13 against 0.25 at amount 0.62; the engine FDN 0.21): the routine's two frames are held and the wet path is nearly mono at the output (`R12`, `R14` of one stereo call); a gap of 0.12 against 0.04.
* **Decay.** Not measurable: the performer keeps playing, so no clip contains a release followed by a reverb tail (`emu_vs_hw.py` `tail_t60_s` returns none for the hardware in all three segments). Emulator side, from the last release: engine FDN T60 0.6 to 0.7 s, firmware 3.4 to 4.1 s (the oracle's own impulse response says 4.3 s at amount 127). The steepest 0.5 s fall of the hardware clip is -22 dB/s in `fx_reverb` (engine -91, firmware -38) and -123 dB/s in `organ_b3` (engine -129, firmware -78): the first favours the long tail, the second the short one, so the clips do not decide the T60.

**Decision (D22).** The port matches the oracle exactly, but the end-to-end conditions are not met: the decay gap cannot be measured with the available clips, the stereo gap grows, the spectral gain is 0.75 dB at one clip, and the name mapping of types 0, 1 to the UI's SPRING is still ASSUMED (s8). `FirmwareFx::reverb` therefore stays opt-in and `render_hash` is unchanged. What would move it: a hardware clip with a clean release into a Spring reverb (or a long sustained note released), or the type 0 name check on the unit.

## Implications

- The reverb dry / send law is exact, and types 0 and 1 are reproduced word for word in Rust (s10, opt-in `FirmwareFx::reverb`); types 2..11 are not.
- `kCmpResponse` is closed: no word depends on it.

## Follow-ups

1. A hardware clip with a clean release into a Spring reverb (decay), and the SPRING name check of types 0, 1; then re-run the s10 comparison and revisit the default (D22).
3. Types 2..11 (`0x9500A..` words, s3): their routines (`0x123ABE`, `0x12314A`, `0x12329B`) are not these wrappers; the listing-to-steps generator applies.
4. Leslie: capture the rotor words over many ARM ticks per `kEffectsLeslieSpeed` value.

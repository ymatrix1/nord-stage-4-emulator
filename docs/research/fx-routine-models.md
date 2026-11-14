# FX routine models: delay, table LFO, all-pass, resonant filters

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the FX Core 2 flat SRAM image (`tools/ns4_dsp_ldr.py --sram` of the OS v1.66 DSP image), ADI `elfdump` listings of the routines (`python tools/adi_dis.py core2_sram.bin 0x240600 0x24F000`, CCES 2.12.1), the `sharc-fx` oracle (`crates/nord-sharc/src/bin/sharc_fx.rs`). Derived data committed: the models, closed forms and golden vectors below; no firmware bytes. Builds on [fx-dsp-map](fx-dsp-map.md), [fx-chain-install](fx-chain-install.md), [fx-type-map](fx-type-map.md) (compressor model, s7).
- **Question:** what exactly do the delay `0x1206B0`, the table LFO `0x1228CF`, the all-pass `0x122C39` and the resonant filters `0x1243F4` / `0x124497` compute, can a reference model reproduce them bit for bit, and can the Rust `fx/` reach parity with them?

## 1. Short answer

All five routines are modelled as instruction-by-instruction transliterations (`tools/fx_model_*.py`) that match the oracle **word for word** on thousands of random scenarios and **register for register before every executed instruction** of the oracle trace. Equations were extracted from the models (symbolic run, `tools/fx_symbolic.py`), written as float64 closed forms (`tools/fx_closed_forms.py`, `tools/fx_closed_delay.py`) that agree with the models to 1e-4 or better, and ported to Rust as opt-in stages with golden tests.

| Routine | pc | Instructions | Model | Random scenarios identical | Rust port (opt-in) |
|---|---|---|---|---|---|
| Delay | `0x1206B0` | 236 | `fx_model_delay.py` | 4,600 / 4,600 | `fx/fw_delay.rs` (`FwDelay`) |
| Amp drive stage (formerly "table LFO + filter chain") | `0x1228CF` | 146 + 103 per block | `fx_model_lfo.py` | 3,600 / 3,600 | `fx/fw_amp_drive.rs` (`FwAmpDrive`) |
| LFO-modulated dual biquad | `0x122C39` | 98 + 60 per block | `fx_model_allpass.py` | 2,500 / 2,500 | `fx/fw_allpass.rs` (`FwAllpass`) |
| Amp tone stack, 2 frames (formerly "resonant filter") | `0x1243F4` | 60 | `fx_model_resonant.py` | 2,500 / 2,500 | `fx/fw_tone_stack.rs` (`FwToneStack2`) |
| Amp tone stack, 4 frames (formerly "resonant filter") | `0x124497` | 89 | `fx_model_resonant4.py` | 2,200 / 2,200 | `fx/fw_tone_stack.rs` (`FwToneStack4`) |

Role correction ([fx-contexts](fx-contexts.md) s5.2, s5.3): the "table LFO" `0x1228CF` is the **Amp drive stage** (its phase word is `kAmpDrive / 127`, the tables are drive curves) and the "resonant filters" `0x1243F4` / `0x124497` are the **Amp tone stack** (treble, mid, bass words). The equations of this note are unchanged (`fx_model_lfo.py`, `fx_model_resonant*.py`, golden file names keep the model names); only the Rust names and doc comments changed.

Status per item: the **algorithm** (data flow, operation order, field offsets) is `VERIFIED`; the **names and units** of the context words (cutoff, feedback, ...) are `EMPIRICAL` where an equation shows the role and `ASSUMED` where only the data flow is known; **which ARM parameter writes which word** is still undecoded for every routine ([fx-type-map](fx-type-map.md) s6). The Rust ports therefore take the firmware's context words as input and ship no musical defaults.

## 2. Method and tooling

```bash
cargo build -p nord-sharc --release
python tools/fx_models.py check delay core2_sram.bin --scenarios 1000 --seed 5      # all five names
python tools/fx_models.py trace delay core2_sram.bin                                # per-instruction compare
python tools/fx_symbolic.py delay                                                   # closed form listing
python tools/fx_golden.py resonant resonant4 > crates/nord-core/src/fx/tests_resonant_golden.rs
```

* `tools/fx_pe.py`: the pair-register machine. SIMD pairs (`PEx` = left, `PEy` = right), 40-bit registers as exact `Fraction`s rounded to a 32-bit mantissa, truncating stores, all reads before all writes in an instruction. Semantics follow the oracle (`crates/nord-sharc/src/core`), among them: loads through `I1` are broadcast (`BDCST1`), through any other index they are pairs `(word, word + 1)`; `Sn = Rm` moves `Rm` into `Sn` and `Sm` into `Rn`; `Rn <-> Sm` exchanges only `PEx.Rn` with `PEy.Sm`; `RECIPS` uses the measured seed table; `FPACK` / `FUNPACK` are the 16-bit short floats.
* `tools/fx_oracle.py`: scenario harness. A scenario is a flat memory image with every word the routine reads; the model runs first (and reports the addresses it wrote), then `sharc-fx` runs the same scenarios in batches of 24 (`--calls`, `--fill N:`) and the written words are compared. A read of an unfilled word raises in the model, so a scenario can never rely on stale oracle memory.
* `sharc-fx` gained `--trace` (pc and R, S, I, M, L, B files before every executed instruction), `@file` arguments, and the stop pc is pushed as the return address so that a delayed `RTS (DB)` (the 4-frame resonant filter) ends the call after its two slots. `tools/fx_models.py trace` checks the model against every trace row, which localises a mismatch to one instruction; this found no mismatch in the final models.
* A mutation check changed one coefficient word of the delay model: 48 of 48 scenarios differ, so the comparison is not vacuous. Inputs with exact zeros of either sign (240 scenarios per routine) also match.

All scenarios are synthetic: random floats in plausible ranges, random packed delay words, random tables. The context a real instance has is unknown (s7).

## 3. Delay `0x1206B0`

Two stereo frames per call, `x = [l0, r0, l1, r1]` at `I0`, rewritten in place (`I0 += 4`). `c[o]` is the word at `I1 + o`; pairs are `(c[o], c[o + 1])` = (left, right). `K = 0x3F7DA3C2 = 0.99078`; `sm(t, s) = t + K (s - t)` is the one-pole step used for every smoothed parameter (per call, so the time constant is `1 / (1 - K) = 108` calls = 4.5 ms at 48 kHz).

**Memory outside the context.** `DM(0xC03D4)` and `DM(0xC03D5)` are pass pointers to the write and read delay memory; a 4-word output buffer at `c[0xF]` (sends); scratch pairs `DM(0xC02C8..0xC02CB)` (internal); an interpolation table at `c[6]` read through a signed 10-bit index.

| Stage | Equation | Words |
|---|---|---|
| Level ratio | `R = max(min(max(P7L, P7R), 1), 0.01) / max(P9L, P9R, 0.01)` (reciprocal seed and two Newton steps); `ratio_c = clamp(R (1 - kt) + state_c kt, lo, hi)`, stored back | `P7` = `c[7..8]`, `P9` = `c[9..10]`, `state` = `c[0xC..0xD]`, `kt` = `c[0xE]`, `hi` = `c[0x24]`, `lo` = `c[0x25]` |
| Parameter smoothers | `g = sm(target, state)`; only the left result is read back, as a broadcast; both results are stored | dry `g_d`: target `c[4]`, state `c[-0x20]`; shape `g_f`: `c[5]`, `c[-0x1F]`; input `g_in`: `c[-0x1B]`, `c[-0x14..-0x13]`; matrix `m_ll m_rr m_xl m_xr m_rl m_lr`: targets `c[-0x1A..-0x15]`, states `c[-0x12..-0xD]`; all-pass pair `a1`, `a2`: targets `c[-0xB]`, `c[-0xA]`, states `c[-7]`, `c[-6]`; stored gains `(1 + a1) / 2` at `c[-9]` and `(1 + a2) / 2` at `c[-8]` |
| Feedback all-pass | the previous sends `o` (output buffer) pass two first-order all-passes per channel: `v = (1 + a1)/2 o; y1 = s1 + v; s1 = a1 y1 - v;` then `u = (1 + a2)/2 y1; y2 = s2 + u; s2 = u - a2 y2`; `y2` is the fed-back signal `fed_n` | states `c[-5..-4]` (stage 1, left/right), `c[-3..-2]` (stage 2) |
| Meter of the fed-back signal | `P9_c = max(P9_c c[0xB], |fed_0|, |fed_1|)` | `c[9..10]`, decay `c[0xB]` |
| Write | `v = fed ratio_c`; `idx = FIX(clip(0.3 v, c[-0xC]) BY c[-0x1E])`; table lookup `T = lerp(table, idx >> 22, frac)` with the fraction taken from the bits selected by `c[-0x1C]` (position bits 5..0, length bits 11..6) times `2^c[-0x1D]`; sample `s = v (1 - g_f) + 3.5649 T g_f`; the two channels are packed `fpack(s_L) \| fpack(s_R) << 16` and stored at `write_base + c[0x10]` (frame 0) and `+ 0x14` (frame 1) | `3.5649 = 0x406429FB`, `0.3 = 0x3E99999A` |
| Read | weight `w = ((M[read + c[0x12]] << c[0x13]) & 0xFFFF) 2^-15`; `tap = w S[i] + (1 - w) S[i + 1]` per lane (`S[i]` at `read + c[0x11]`; lanes are `funpack` of the low and high 16 bits); frame 1 reads the same pattern `0x28` words later | scratch `DM(0xC02C8..)` = `w, 1 - w` per frame |
| Damping filter | `y = b0 tap + z; z = b1 tap + a y` per channel, two frames in sequence; output `x g_d + y` | `b0 b1 a` = `c[-1] c[0] c[1]`, state `c[2..3]`; taps parked at `c[0x1D..0x20]` |
| Sends | `send_L(n) = x_L g_in + (x_L + x_R) m_xl + tapL_prev m_ll + tapR_prev m_rl`, `send_R(n) = x_R g_in + (x_L + x_R) m_xr + tapR_prev m_rr + tapL_prev m_lr`, with the taps of the **previous call** (`c[0x1D..0x20]`); stored in the output buffer | frame order `l0 r0 l1 r1` |
| Meter of the sends | `P7_c = max(P7_c c[0xB], |send_c0|, |send_c1|)` | `c[7..8]` |

Units: `w` is a 16-bit fraction (1.0 = `0x8000`), samples are 16-bit floats (`fpack` keeps 11 fraction bits, range 2^-17 .. 2^9), the sends and the meters are linear amplitudes. `FPACK` rounds to nearest even; the Rust `fw_short_float.rs` is the same arithmetic.

Closed form: `tools/fx_closed_delay.py` (agrees with the model to 5e-6; the packed line words exactly). Rust: `FwDelay::process(&mut DelayIo)`; golden tests `fx/tests_delay_golden.rs` (3 calls, state carried, sends, delay memory and context compared; worst relative error below 1e-6, packed words exact).

## 4. Table LFO `0x1228CF`

> **Role correction (2026-11-13, [fx-contexts](fx-contexts.md) s5.2):** the ARM writes `kAmpDrive / 127` into `c[-0x20]`, the word this section calls the LFO phase target. The stage is the **Amp drive**, the two tables are drive curves and the equations below are unchanged. Likewise `0x1243F4 / 0x124497` (s6) are driven by the Amp treble, mid and bass ids (tone stack).

Per call: a phase step, two table LFO gains, then a loop of `ctx[0x15]` blocks of two stereo frames (146 instructions with one block, +103 per further block). Words are floats unless noted; `K` as above.

| Stage | Equation | Words |
|---|---|---|
| Phase | `ph = sm(c[-0x20], c[-0x1F])`, stored in `c[-0x1F]` and `c[-0x1E]`; `P = FIX(ph BY c[-2])` (31 = Q31, saturates to all ones); `idx = P >> 24` (unsigned byte), `f = (P & 0xFFFFFF) / 2^24` (`FDEP 7:24` then `FLOAT BY c[-1] = -31`) | `c[-2]` int, `c[-1]` int |
| Gains | `G1 = lerp(A, idx, f) c[-0xD]`, `G2 = lerp(B, idx, f) (ph c[-0xC] + c[-0xB])`, each stored twice (`c[-0xA..-9]`, `c[-8..-7]`); `lerp(T, i, f) = T[i] + f (T[i + 1] - T[i])` | tables `A`, `B` at the pointers `c[-0x15]`, `c[-0x14]` (257 entries); the firmware `A` is the `-cos` quarter wave `DM 0xC0448` ([fx-dsp-map](fx-dsp-map.md) s5) |
| Mix | `m = clip(20 ph, c[0])`, stored at `c[-0x1D..-0x1C]`, and `c[0] - m` at `c[-0x1B..-0x1A]` | `20.0 = 0x41A00000` |
| Section 1 (per channel, per frame) | transposed DF II: `y = b0 x + z1; z1' = b1 x + z2 - a1 y; z2' = b2 x - a2 y` with `b = c[7..9]`, `a = c[0xA..0xB]`, states `c[3..4]` (`z1`) and `c[5..6]` (`z2`); `y` is stored at `c[-0x19..-0x16]` | all channels share the broadcast coefficients |
| High-pass | `a_n = y G1; e_n = p (a_n - a_{n-1}) - q e_{n-1}` with `p = c[-0x13]`, `q = c[-0x12]`; states `c[-0x11..-0x10]` (`a`) and `c[-0xF..-0xE]` (`e`) | |
| Waveshaper | `W = FIX(clip(e, c[1]) BY c[-2])`; `idx2 = W >> 22` (signed 10-bit, `FEXT 22:10 SE`), `f2 = (W & 0x3FFFFF) / 2^22`; `T = lerp(C, idx2, f2)` with `C` at the pointer `c[2]` | |
| Mix with dry | `z = T G2 m + y (c[0] - m)` | |
| Section 2 | the same TDF II with `b = c[0x10..0x12]`, `a = c[0x13..0x14]`, states `c[0xC..0xD]` and `c[0xE..0xF]`: `y2` | |
| Output high-pass | `v = 2 y2; out = p (v - v_prev) - q out_prev` with states `c[-6..-5]` (`v`) and `c[-4..-3]` (`out`); the frames are written in place | |

The right channel of a section runs the same recurrence on its own state words (the second word of each pair). Closed form: `tools/fx_closed_forms.py::lfo` (agrees with the model to 1e-4, 2e-3 where a table index sits on a rounding boundary). Rust: `FwLfo::process(&LfoTables, &mut [f32])` (the caller supplies the three tables; the block count is the slice length); golden `fx/tests_lfo_golden.rs` (worst relative error 1.5e-5 where the high-pass difference cancels).

## 5. LFO-modulated dual biquad `0x122C39`

A free-running counter word (`DM(c[-0x1D])`, pair) drives the LFO; the loop runs `c[4]` blocks of two stereo frames.

| Stage | Equation | Words |
|---|---|---|
| Integer phase | `c[-0x1F] = c[-0x1F] + c[-0x20]` (mod 2^32), stored in `c[-0x1F]` and `c[-0x1E]` | ints |
| LFO value | `x = int32(counter_L c[-0x1C] - c[-0x1B])` (`Rn = Rx * Ry (UUI)`, low word), `u = 2 \|x\| 2^-31 - 1` (triangle), `s = a1 u + a3 u^3 + a5 u^5` with `a = c[-0x13..-0x11]` (odd polynomial); the **right** channel's value is the constant `c[-0x16]` (the routine swaps `R7` with `S0`) | |
| Smoothers | `A_L = sm(c[-0x1A], c[-0x18])`, `A_R = sm(c[-0x19], c[-0x17])`, `S_L = sm(s, c[-0x15])`, `S_R = sm(c[-0x16], c[-0x14])`, all with `k = c[3]`, stored back | |
| Gain | `g = clip(S_L A_L, S_R) + A_R` (the later broadcast reads see the right words just stored) | |
| Biquads | per channel and frame, two TDF II biquads on `x`: `N` (`b0 b1 b2 a1 a2` = `c[-7], c[-6], c[-5], c[-4], c[-3]`, states `c[-2..-1]` and `c[0..1]`) and `F` (`c[-0x10..-0xC]`, states `c[-0xB..-0xA]` and `c[-9..-8]`): `y = b0 x + z1; z1' = (b1 x + z2) + a1 y; z2' = a2 y + b2 x` | |
| Output | `out = g ((1 - m) x + m y_F) + m y_N` with `m = c[2]` | |

Closed form: `tools/fx_closed_forms.py::allpass` (agrees to 6e-5). Rust: `FwAllpass::process(timer, &mut [f32])`; golden `fx/tests_allpass_golden.rs` (worst error below 3e-6).

## 6. Resonant filters `0x1243F4` and `0x124497`

Both: parameter smoothers `sm(target, state)` with `k = c[-0x11]`, a state-variable section whose band-pass integrator `w` is clipped to `c[-0xE]` in the feedback path, trapezoidal averaging `(s + s_prev)/2`, and a one-pole shelf; left and right run the same recurrence on `(c[o], c[o + 1])`. State words per channel: smoother `u` `c[-0xD]`, previous `c[-9]`, band `w` `c[-7]`, low `z` `c[-5]`, shelf `q` `c[-0xB]`.

`0x1243F4` (2 frames, parameters smoothed first; `A B C D E F` are the smoothed `c[-0x1E] c[-0x1D] c[-0x15] c[-0x16] c[-0x1A] c[-0x19]`, targets `c[-0x20] c[-0x1F] c[-0x17] c[-0x18] c[-0x1C] c[-0x1B]`; `G = (lim - E) / 2`, stored at `c[-0x13]`, and `(lim - F)/2` at `c[-0x12]`):

```
u += a (x - u)               a = c[-0x14]
s  = x + A u
z += B w
w  = clip(w + B ((s + x_prev)/2 - z - C w), lim)
y  = s + D w
r  = G y;  d = r - q
out = y + F d;  q = r + E d;  x_prev = x
```

`0x124497` (4 frames, unrolled; parameters smoothed after the frames with the same targets; `P = c[-0x13]`, `R = c[-0x19]`, `S = c[-0x1A]`, `B = c[-0x1D]`, `C = c[-0x15]`, `D = c[-0x16]`, `H = c[-0x1E]`):

```
r = x P;  d = r - q;  s = x + d R;  q = r + d S      (shelf on the input)
z += w B
w = clip(w + B ((s + s_prev)/2 - z - w C), lim);  s_prev = s
y = s + w D
u += a (y - u)               a = c[-0x14]
out = y + u H
```

The delayed return of `0x124497` runs `f1 = f1 + f8; DM(4, I0) = r0` and `DM(6, I0) = r1` in its two slots; the model includes them. Closed forms: `fx_closed_forms.py::resonant2`, `::resonant4` (agree to 1e-4 and 3e-5). Rust: `FwResonant2::process(&mut [f32; 4])`, `FwResonant4::process(&mut [f32; 8])`; golden `fx/tests_resonant_golden.rs` (worst relative error 3e-6 over outputs and final context).

With an all-zero context both resonant routines pass the input through (unit test).

## 7. Rust parity

The ports live in `crates/nord-core/src/fx/` (`fw_delay.rs`, `fw_amp_drive.rs`, `fw_allpass.rs`, `fw_tone_stack.rs`, shared `fw_words.rs`, `fw_short_float.rs`) and are exported (`FwDelay`, `FwAmpDrive`, `FwAllpass`, `FwToneStack2`, `FwToneStack4`) but **not wired into the default signal path** (the opt-in `FirmwareFx` switch of [effects-and-routing](../specs/effects-and-routing.md) s4.0c wires the delay mix law and the compressor): the effect classes of `fx.rs` (`Chorus`, `Flanger`, `Phaser`, `Vibe`, `Delay`, amp resonance) are driven by UI parameters, and the ARM words that would fill these contexts are not decoded. Allocation-free and lock-free (`process` takes plain slices); the context is the firmware's own word layout so a context captured from the DSP image loads unchanged. Golden vectors are generated, not hand-written (`tools/fx_golden.py`), and a Python test checks that the committed Rust files equal the generator output.

## Implications

* The firmware's FX algorithms for these five routines are known exactly; what `fx.rs` approximates with textbook structures (a feedback delay with a one-pole tone control, an LFO modulated delay, RBJ filters) can be replaced class by class once the ARM parameter mapping is decoded.
* The delay is a ping-pong send matrix with a level-normalised, waveshaped, 16-bit-float feedback path, not a plain feedback line; its damping filter is a first-order section on the interpolated tap.
* All routines smooth every parameter per call with the same pole `K = 0.99078` (time constant 108 calls of 2 or 4 frames), so a UI change takes about 4.5 ms (delay, 2 frames per call) to settle.

## Follow-ups

* Decode the ARM writers of the context words (`kDlyTime`, `kDlyFeedback`, LFO rate, filter cutoff / resonance) so that targets and states can be named and the ports wired as selectable effect classes.
* The delay time words of the ring variant `0x120AB5` are decoded in [fx-delay-time](fx-delay-time.md) (oracle impulse measurements, no instruction-level model yet). Model the second delay variants `0x1208AF` and `0x120AB5`, the reverb multi-tap lists (`0x123ABE` / `0x123C0F`, `0x12314A` / `0x12329B`, `0x123DDA` / `0x123466`) and the phaser `0x12379B` with the same tools; `fx_pe.py` already covers the instruction forms they use except `RSQRTS`.
* Record real context words of an installed instance (ARM emulation of `Edt_Effects_*` handlers, [arm-fw-emulation](arm-fw-emulation.md)) and add them as an end-to-end check of the ports.

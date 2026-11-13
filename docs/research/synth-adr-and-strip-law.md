# Synth ADR record chain, strip word 3 to R, and the 0x1257B2 ladder

- **Checked:** 2026-11-13
- **Sources (local, never committed):** ORG core 2 image (OS v1.66, `python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram DIR`), ADI listing (`tools/adi_dis.py`, range `0x240600..0x254000`, listing offset = pc - `0x120300`), `sharc-org` complete passes (`crates/nord-sharc`, new `--rwatch` and `--trace` options), `tools/synth_adr_oracle.py` (driver), `tools/synth_adr_model.py` (models), `tools/synth_ladder_poles.py`. Builds on [synth-stage-equations](synth-stage-equations.md) and [synth-voice-control](synth-voice-control.md).
- **Question:** Who consumes the ADR rate coefficients and how does the envelope run per pass; what is the strip word to `R` (cutoff) law; which stage does `kFltType` select; what are the equations of `0x1257B2`?

Tags (`.agents/rules/provenance-in-code.md`): **VERIFIED** = reproduced bit for bit or within the stated error by a command in "Reproduce", **EMPIRICAL** = fitted or read from a listing and checked on a few runs, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Consumer of the nine ADR coefficients | The envelope generator **is in the ORG core 2 pass**: the loop at `pc 0x1274EB..0x127AFF` (listing `0x71EB..0x77FF`), 8 slots (`I2 = DM(0xC10B2 + 0x18 s)`, `I3 = DM(0xC10B3 + 0x18 s)`), 3 chains per slot, SIMD pair PEx/PEy = controller words `DM(0xC1DCC + 2 s)` and `+1`. It reaches the coefficients **through pointers** (`DM(F-18)`, `DM(F-6)`, `DM(F+8)`), which is why the earlier absolute-address search found no reader. `--rwatch 0xC0DC0:0x240` lists the load sites. | VERIFIED (run) |
| Per-pass law | A chain is a list of 4-word records `[a, b, thr, link]`: `y' = y a + b`; `y' < thr` clamps to `thr` (to 0 and restart when `thr < 0`); `y' > 1` clamps to 1 and follows `link`. The 40-bit register arithmetic reproduces the oracle bit for bit on three scenarios. | VERIFIED |
| What the nine words are | attack record `[1, cA, 0, ->decay]` (**linear ramp**, `cA` per pass), decay record `[1-cD, 0, 0.5, ->self]` (**exponential decay to the sustain level 0.5**), release record `[1-cR, 0, 0, ->self]` (exponential to 0). The sustain level is the decay record's `thr`, not an ADR word. | VERIFIED (shape), EMPIRICAL (0.5 default source) |
| Time base | one control step per pass; the installer rewrites the coefficients every **16th** pass. Pass length in samples not isolated (4 samples per stage call). | VERIFIED (16), ASSUMED (pass = control tick) |
| Strip word to `R` | `R` of channel 0 comes from strip word **3** (the PEy lane; word 2 is the oscillator pitch, PEx): `R = clamp(0.36083 + 0.0079301 i, 0, 1.0111)` with the word's top byte `i` (clip +-0x6A000000) and linear interpolation on the low 24 bits; `fc = fs atan(R)/pi`. | VERIFIED (19 pairs, 3.3e-6), EMPIRICAL (closed form of the table, 1.0e-6) |
| `kFltType` to stage | **Not resolved.** The installer only special-cases word-1 id `0x120C68`; the type to stage id map needs the ARM edit path with a loaded preset (idle ids write the same word for every value). | open |
| `0x1257B2` | Impulse response is an **order-4 linear system with four real poles** (fit error 1e-8 .. 1e-10); two poles are fixed by `R`, two follow the resonance index, they couple at high index. Closed-form coefficient law not derived; pole table in section 5. | VERIFIED (order, poles), open (equations) |
| Rust | `synth/fw_adr_chain.rs` (record step) and `synth/fw_strip_cutoff.rs` (word to `R`), opt-in, not wired; default and `render_hash` unchanged. | VERIFIED (golden words) |

## 2. The envelope loop (listing `0x71EB..`)

Frame `F = I3` of slot 0 is `0xC1188`; PEx words sit at `F + o`, PEy words at `F + o + 1`. Slot record `S = I2`. Controller word `C = DM(0xC1DCC + 2 s)`.

```text
sgn = signext3(C >> 24 & 7)        T = signext24(C >> 8)
P   = DM(F-30) if sgn == 0 | DM(F-28) if sgn > 0 | DM(F-26) if sgn < 0    (start records)
if sgn < 0:  DM(F+20) = 0
if T != DM(S-32):  DM(F-18) = P;  DM(F-6) = DM(F-14);  DM(F+8) = DM(F+0)  (re-trigger)
DM(S-32) = T
chain 1:  rec = DM(F-18)   y = DM(F-20)   L = DM(F-22) = 1.0
          y' = y a + b                                (r8, r9 = rec[0], rec[1]: LW load)
          if y' < thr (rec[2]):  y' = thr;  if thr < 0: y' = 0
          if y' > L:             y' = L;    DM(F-18) = rec[3]
          DM(F-20) = y'
chain 2:  rec = DM(F-6)   y = DM(F-8)    same law, limit f0 = DM(0xC1370) = 1.0, pointer DM(F-6)
chain 3:  rec = DM(F+8)   same law (state at F+... , not traced further)
```

The three chains are the amp, filter and oscillator ADR (ASSUMED from the order of the strip words `w26..w34`; the chain-2 state `DM(F-8)` follows the second attack coefficient in the oracle). The frame holds further smoothed outputs (`DM(F-12)`, `DM(F-4)`) that were not decoded.

Facts established with the oracle (section 3):

- The coefficient words of chain 0 are `DM(0xC0DC9)` (attack `b`), `DCC` (decay `a`), `DD0` (release `a`); chain 1 `DDD, DE0, DE4`; chain 2 `DE9, DEC, DF0`. The sustain level is `DM(0xC0DCE) = 0.5`, written once by the image (a decay record `thr`).
- The installer (`0x12082F` module and the pass copy `0x126F0F`) adds a **modulation word** to each strip word before taking the top byte (`DM(i0-18..)`). With `DM(0xC1DCC) = 0` the attack index of strip 0 word 26 `0xC0000000` is -64 (`cA = 0.092162`); with the controller word `0x01000000` the same strip word gives index 0 (`cA = 4.505e-5`). The mapping controller word to modulation is **not decoded**, so strip word to coefficient is VERIFIED only at zero controller state ([synth-stage-equations](synth-stage-equations.md) s7 table).
- The installer rewrites the records every 16th pass (pass 0, 16, 32): pokes into the chain survive 15 passes.
- A change of `T` with `sgn = 0` starts the **release** record (`y' = 0.9 y` in the scenario); `sgn > 0` selects the fast pre-release record `[0.961, -5e-4, -1e-5]` (negative `thr`: it drives `y` to 0 and the module code then puts the pointer on the attack record: gate-on retrigger); `sgn < 0` selects `DM(F-26)`.

## 3. Oracle scenarios (golden words)

`python tools/synth_adr_oracle.py` (slot 0, controller `0x01000100` at pass 0, records poked every pass: attack `[1, 0.1, 0, ->decay]`, decay `[0.8, 0, 0.5, ->decay]`, release `[0.9, 0, 0, ->self]`, fast `[0.5, -0.03, -0.02, ->self]`).

| Scenario | Controller | Level after passes 1..15 | Model |
|---|---|---|---|
| A | `0x01000100` | 0.1 ramp to 1.0 (`0x3F7FFFFD`, clamp `0x3F800000` at pass 11), 0.8 0.64 .. 0.5 held | bit exact |
| B | `0x00000200` before pass 8 | ramp until pass 7 (`0x3F333332`), then `0.9 y` (`0x3F2147AC` ...) | bit exact |
| D | `0x04000200` before pass 6 | ramp, `0.5 y - 0.03` (`0x3E6147AD`, `0x3DA3D709`, `0x3C23D706`), 0 at pass 9 (restart), ramp again | bit exact |

The multiply and the add are **separately rounded to a 32-bit significand** and the store truncates (`0.2 + 0.1` gives `0x3E999999`, not the nearest `0x3E99999A`): the model uses `tools/sharc40.py`, the Rust port `round32` and a truncating store. Tests: `tools/tests/test_synth_adr_model.py`, `synth::fw_adr_chain::tests`.

## 4. Strip word 3 to `R`

Pass control block `pc 0x125F80..0x125FEE` (listing `0x5F80..`), one copy per channel (`DM(0xC0B83 + 0x26 c)` strip pointer, default `0xE0020` = strip 0 for every channel):

```text
sum_x = w2 + m0 + (w4 + m1) lfo << 2 + ...        sum_y = w3 + m0' + (w5 + m1') lfo << 2 + ...     (PEx / PEy)
x lane: clip +-DM(0xC1478)=0x58000000, idx = bits 23..31, frac = low 23 bits -> ratio table DM(0xB069B + idx)  -> DM(0xC1F79)
y lane: clip +-DM(0xC1479)=0x6A000000, idx = bits 24..31, frac = low 24 bits -> R table     DM(0xB0BA2 + idx)  -> DM(0xC2924 + 1)
```

The earlier sweep moved word 2 and saw `R` constant at 0.36083 because **R comes from word 3**; the store `DM(0xC2924) = r9` writes the same `R` to both lanes (`DM(0xC2924)` and `DM(0xC2925)` are equal in the sweep). The strip words are smoothed every pass toward the common block (`w <- 0.99609375 w + 0.0048828 common`), so the word the pass sees is `DM(0xE0003)` read **after** the pass, not the value written: `R` matches the post-pass word with no offset (`idx_from_R - word/2^24 < 4e-4` on 10 pairs).

| Table (idx -128..127, dumped from the image) | Closed form | Error |
|---|---|---|
| `DM(0xB0BA2 + i)` | `clamp(0.36083 + 0.0079301 i, 0, 1.0111)`: 0 for `i <= -46`, flat 1.0111 for `i >= 82`, other tables beyond +-106 (unreachable after the clip) | 1.0e-6 (EMPIRICAL) |
| `DM(0xB069B + i)` | `0.0060279 2^(i/24)` (24 steps per octave) | 4.2e-6 relative (EMPIRICAL) |

`tools/synth_adr_model.py::cutoff_r` reproduces the 19 oracle pairs to 3.3e-6 (`CUTOFF` in the test); Rust `fw_strip_cutoff_r`. `R` is `tan(pi fc/fs)`: `R = 0.36083` is 0.1103 fs (5.3 kHz at 48 kHz, ASSUMED rate), the flat top `R = 1.0111` is 0.2517 fs.

Open: the ARM value written for a knob position (idle ids are value independent, [synth-voice-control](synth-voice-control.md) s3), the `w5`, `w7`, LFO and glide terms of the sum, and which channel carries the filter for a loaded preset (all eight channels read strip 0 by default). Not attempted: capturing real strip words with `tools/arm_edit_capture.py` (a preset edit needs the key-down/preset harness of [synth-voice-control](synth-voice-control.md) s9).

## 5. `0x1257B2`: four real poles

The stage is a SIMD coupled form with six coefficient fields and cross-lane swaps; instead of decoding it, the impulse response was identified (`tools/synth_ladder_poles.py`, 384 samples, order-4 least squares, max residual 1e-8 .. 1e-10):

| `R` | idx | poles | DC gain |
|---|---|---|---|
| 0.1 | 0 | 0.75016 0.85715 0.96774 0.99778 | 1.2715 |
| 0.1 | 64 | 0.75019 0.85704 0.93446 0.95237 | 1.0 |
| 0.1 | 100 | 0.42972 0.71384 0.75253 0.85716 | 1.0 |
| 0.3 | 64 | 0.28754 0.65876 0.93424 0.95250 | 1.0 |
| 0.3 | 100 | 0.30573 0.43573 0.65839 0.72018 | 1.0 |
| 0.3 | 120 | -0.12644 0.27001 0.27694 0.65877 | 1.0 |

Two poles depend on `R` only (0.75019/0.85715 at `R = 0.1`, 0.28754/0.65877 at `R = 0.3`, constant for idx <= 64) and two on the index (almost `R` independent); above idx 100 all four couple. A 4-pole low pass with all-real poles and resonance feedback is the ladder reading. The frame fields after the prologue (`R = 0.3`, idx 64): `-24 = 0.94415 = 1 - (+12)`, `-22 = 0.49315`, `-20 = 1.644`, `+14 = 1.97063`; the `R` pair follows `-22`, `-20` (0.18891, 1.88979 at `R = 0.1`). Closed forms of the poles are not derived: EMPIRICAL table only.

## Implications

- The ADR of the synth is a **record-chain generator in the pass**: linear attack, exponential decay to a sustain level, exponential release, one step per pass, rate tables of [synth-stage-equations](synth-stage-equations.md) s7 give the per-pass coefficients. A Rust port can use `fw_adr_step` with `fw_env_rates` coefficients; the pass length and the controller trigger protocol still decide the time base.
- The cutoff of the synth filter is **linear in the strip word** (R ramp), then warped by `atan`: octave spacing in Hz is not constant, unlike the oscillator pitch (24 steps per octave).
- `kFltType` stays open; do not make the firmware filter family the default (TD-11).

## Follow-ups

1. Decode the controller-word to modulation path (`0x12082F`, `0x120300` module) and the gate/trigger protocol (`sgn`, `T`) with a real key-down; then time base (samples per pass).
2. Capture real strip words for a synth preset edit (ARM harness plus a loaded preset) to close the knob to word map and `kFltType` to stage id.
3. Derive the `0x1257B2` coefficient laws from the six frame fields (trace with `--trace 0x1257B2:0x125800`).

## Reproduce

```bash
export NS4_CCES="C:/Analog Devices/CrossCore Embedded Studio 2.12.1"
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram SRAM_ORG
cargo build --release -p nord-sharc
export NS4_SHARC_ORG=target/release/sharc-org NS4_ORG_IMAGE=SRAM_ORG/core2_sram.bin
python tools/synth_adr_oracle.py                       # scenario A words
# who loads the coefficient words (new --rwatch) and a register trace of slot 0 (new --trace):
sharc-org SRAM_ORG/core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 3 --rwatch 0xC0DC0:0x240
sharc-org SRAM_ORG/core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 1 --pre 0xC1DCC=0x01000100 --trace 0x1274EB:0x12759F
python tools/synth_ladder_poles.py                      # section 5 poles
python -m pytest tools/tests/test_synth_adr_model.py   # golden words, no firmware needed
```

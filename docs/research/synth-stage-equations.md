# Synth stage equations: the Core2 filter stage `0x12566E` (work in progress)

- **Checked:** 2026-11-13
- **Sources (local, never committed):** ORG core 2 image (OS v1.66, `tools/ns4_dsp_ldr.py --sram`), ADI listing (`tools/adi_dis.py`), `sharc-org` passes (`crates/nord-sharc`), `tools/synth_stage_oracle.py`, `tools/synth_filter_model.py`. Builds on [synth-voice-control](synth-voice-control.md), [org-stage-map](org-stage-map.md), [pno-module-chain](pno-module-chain.md).
- **Question:** What are the filter and envelope equations of the synth strip stages, and what does `crates/nord-core/src/synth/` need to match them?

Tags: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = fitted or read from a listing, **ASSUMED** = reading of a purpose.

## 1. Calling convention (VERIFIED, listing and run)

The pass calls the B stage of channel `c` (listing `0x127B01..0x127BA8`, SIMD on, 8 calls per round) with

| Register | Value for channel 0 | Meaning |
|---|---|---|
| `I8` | `DM(0xC0B8C)` | stage entry (the word-1 module, e.g. `0x12566E`) |
| `I3` | `0xC2942` (+ `0x34 c`) | stage **frame**, fields addressed `I3 - 32 .. I3 + 18` |
| `I4` | `0xC1F96` (+ `0x36 c`) | A-stage record: 4 input samples at `I4 - 13, -11, -9, -7`, the filtered samples are written back in place |
| `I5` | `0xC28FA` | common constants: `DM(I5-32) = 0.25`, `-30 = 0.5`, `-28 = 1.0`, `-26 = 2.0`, `-24 = 4.0`, `-22 = 0x3F7FFFFE` |

SIMD: PEx works on the word at the address, PEy on the word at address + 1 (the oracle model, ASSUMED for the hardware); every frame field is a duplicated pair (x, y) in the idle state.

## 2. Filter stage `0x12566E` (VERIFIED, bit-exact)

Two identical sections in series, four poles. Frame inputs: `W28 = DM(I3-28)` (table position: byte 3 = index 0..127, low 24 bits = fraction), `R = DM(I3-30)` (cutoff coefficient), `L = DM(I5-28) = 1.0`.

```text
cin = A[idx] + (A[idx+1] - A[idx]) fr           table A at DM(0xB0D0F)   -> DM(I3-26)
P   = B[idx] + (B[idx+1] - B[idx]) fr           table B at DM(0xB1015)
N   = 1 / sqrt(L + R P + R^2)                   RSQRTS seed + 2 Newton steps (0.5 y (3 - x y^2))
g   = 2 R N  -> DM(I3-22)        k = P N  -> DM(I3-20)
section 1: u = cin x;   section 2: u = out of section 1
  m  = (u + b) / 2          c' = c + g a          v = m - c' - k a
  a' = clip(a + g v, +-L)   b' = u                out = (c' + c) / 2
```

States `a, b, c` per section at `I3-10, -8, -6` and `I3-4, -2, 0`. `tools/synth_filter_model.py` reproduces the three coefficient words and 64 output samples of the oracle exactly (words equal) for nine (index, fraction, R, amplitude) rows (`tools/synth_stage_oracle.py`).

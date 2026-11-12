# SMP interpolation kernel: closed form and unpacker semantics

- **Checked:** 2026-11-12
- **Sources (local, never committed):** the SMP image of OS v1.66 (`bank0_CODE@0x51B574`, 44,596 B) exported with `tools/ns4_dsp_images.py`, flat SRAM image from `tools/ns4_dsp_raw.py --sram`; table at DM `0xE3400` (image offset `0x6D68`); ADI `elfdump` listing of `0x1C07D8..0x1C08E3` (`python tools/adi_dis.py smp.bin 0x380FB0 0x3811D0`, CCES 2.12.1); [pno-dsp-map](pno-dsp-map.md) section 7, [sy-dsp-map](sy-dsp-map.md), [nsmp4-and-npno-cnsp-codec](nsmp4-and-npno-cnsp-codec.md).
- **Question:** Which closed form produces the 1,536-word Q31 kernel the SMP voice loop reads at DM `0xE3400`, is it bit-exact, and do the residual unpacker and 4-tap predictor of that loop match the Rust CNSP decoder?

Tags: **VERIFIED** = reproduced by a command or unit test named in the row; **EMPIRICAL** = a fit; **ASSUMED** = reading of purpose or ARM-side data.

## 1. Verdict

| Question | Answer | Tag |
|---|---|---|
| Kernel closed form | `d = (2 - t) + p/256`, `u(d) = sinc(d) * (0.54 + 0.46 cos(pi d / 3))`, `h[t][p] = u + (1 - sum_t u) / 6`, `word = min(round(h * 2^23), 2^23 - 1) << 8`. A **Hamming window, half width 3, cutoff 1**, with an **additive** per-phase DC correction. | VERIFIED |
| Bit-exact? | **Yes.** 1,536 of 1,536 words equal (0 mismatches, max error 0 LSB of the 23-bit value); 1,535 by plain rounding, the last is the peak `1.0`, which saturates to `0x7FFFFF00` (`2^23 - 1`, not `2^23`). Closest rounding tie: fractional part 0.0019 LSB from 0.5, so libm differences (1e-16) cannot flip a word. | VERIFIED (`python tools/ns4_smp_kernel_fit.py smp.bin`, `test_model_matches_golden_words`, Rust `test_closed_form_golden_words` with two whole-table checksums) |
| Symmetry and gain | `h[t][p] = h[5-t][256-p]`, 0 differences; per-phase tap sum `2^23 +- 2` (rounding of 6 taps), floats sum to 1 within 4e-7. | VERIFIED (`mirror_errors`, `phase_sums`) |
| Residual unpacker | LSB-first bit stream in 16-bit units (one per 32-bit DM word), field `bit = width * n - base`, `FEXT`/`FDEP (SE)` sign extension. Widths 1..16 fit one 32-bit window; the Rust CNSP range (`nbits` 1..16) is covered. | VERIFIED (instruction forms; model tests) |
| 4-tap predictor | `MRF += R0*R1 (SSI)` x4 with history from DM (ring, `L5 = 0x10`) and coefficients from PM: `y = sum c_i h_i + residual`. The FLAC fixed polynomials (order 0..4) are exactly the binomial rows `[0,0,0,0] [0,0,0,1] [0,0,-1,2] [0,1,-3,3] [-1,4,-6,4]`. | VERIFIED (form), ASSUMED (ARM loads these rows, oldest first) |

## 2. How the window was found

The first fits of `sinc(fc d) * W(d)` with every standard window (Kaiser with free beta and half width, Gaussian, Lanczos, Blackman, Blackman-Harris, Nuttall, cos^a, 3 and 4 term cosine sums, raised cosine) stalled at rms 2e-4 to 5e-4, as in [pno-dsp-map](pno-dsp-map.md) section 7 (3-term cosine 1.9e-4, Kaiser 4.6e-4). Per-tap polynomial fits showed why: each unit interval is a smooth function (a degree-9 Chebyshev fit of one row reaches the rounding noise, rms 0.29 LSB), but the **slope jumps at every integer offset by the same amount**:

| Node `d` | -2 | -1 | 0 | 1 | 2 |
|---|---|---|---|---|---|
| left slope minus right slope | 0.008892 | 0.008890 | 0.008890 | 0.008888 | 0.008893 |

A jump that does not scale with the window value (0.5 near `d = 1`, 0.2 near `d = 2`) is **additive**, and a per-phase additive term is `c(p) = (1 - sum_t u) / 6`: the six taps of one phase get the same constant so that they sum to exactly 1. At phase 0+ the six taps of a truncated windowed sinc sum to `1 + eps * W(3) / 3` (`sum_t sinc'(n_t) W(n_t) = W(3) / 3` over the offsets 2, 1, 0, -1, -2, -3), so each tap gets `W(3) / 18` per unit of offset, a jump of `W(3) / 9` across a node; the measured 0.008890 gives `W(3) = 0.0800`, which is `2 * 0.54 - 1`, the edge value of a Hamming window. With that normalisation, an even polynomial window fit drops from rms 1e-4 to the rounding floor (rms 0.285 LSB, max 1.0 LSB at degree 5) and a one-parameter-family scan then picks out the window:

| Window (same sinc + additive-DC form) | rms residual | max residual |
|---|---|---|
| **Hamming `0.54 + 0.46 cos(pi d / 3)`** | **0.286 LSB (pure rounding)** | **1.0 LSB, exactly 0 after saturation** |
| Hann, half width 3.19 (free) | 13,700 LSB | 27,000 LSB |
| Blackman, half width 4.56 (free) | 3,500 LSB | 9,000 LSB |
| cos^3.2, half width 3.94 (free) | 3,200 LSB | 8,600 LSB |
| Kaiser, beta 5.29, half width 3.04 (free) | 3,800 LSB | 9,700 LSB |
| Gaussian sigma 1.35 | 18,600 LSB | 34,000 LSB |
| Lanczos a = 2.69 | 32,600 LSB | 63,600 LSB |

Free parameters of the Hamming family converge on half width 3.000 and `a` = 0.540 (the textbook 0.54/0.46, not the exact 25/46). The kernel is a **cardinal** interpolator (zero at every non-zero integer offset, 1 at 0), as a sinc should be, which is why all earlier fits had `fc = 1.000 +- 5e-4`. Frequency response (zero-phase FFT of the 1,537-point prototype): flat to 0.2 of the source rate, 0.9605 at 0.3, 0.799 at 0.4, 0.499 at 0.5, 0.041 at 0.7, below -55 dB past 0.8.

Reproduce: `python tools/ns4_smp_kernel_fit.py smp.bin --survey` (Hamming 0 mismatches; Hann, Blackman, Lanczos and a wider Hamming tens of thousands of LSB off).

## 3. Unpacker and predictor semantics (SMP `0x1C07D8..0x1C08E3`)

Voice record `I3` (offsets as printed by ADI, `0xFFFFFFxx` = negative):

| Field | Use |
|---|---|
| `-0x1C`, `-0x1E` | width (bits per residual) and sample counter: `bit = width * counter - base` (`R0 = R6*R7 (UUI)`, `R0 = R0 - R1`, `R1 = DM(-0x1B)`) |
| `-0x1A`, `-0x19` | circular stream buffer base (`B1`) and history buffer base (`B5`) |
| `-0x13` | `FEXT` length, applied as `R0 << 6` (`R11 = 6`) |
| `-0x11` | predictor coefficient base (`I9`, `PM`) |
| `-0x1D` | 32-bit position fraction; `R1 >> 24` selects the kernel phase |
| `0`, `+1` | left and right gain |

Per output sample: bit position, unit index `bit >> 4`, offset `bit & 15`; two consecutive memory words are combined `lo | hi << 16` (so each word holds 16 payload bits), shifted right by the offset, `FEXT` `width` bits, `FDEP (SE)` sign-extends; four `SSI` MACs form `pred`, `y = pred + residual` goes back into the history ring; then `I9 = 0xE3400 + (frac >> 24)`, six `SSF` MACs over the ring with stride 256 through the kernel (`R2 = MRF (SSFR)`, rounded), `LSHIFT` by the voice scale (`R15`), times left and right gain, added to the bus. Tap `t` weighs sample `n - 2 + t` at read position `n + p/256`. A second copy of the loop (`R13 = 0x20`, `mode1` bits `0xE00000` set) assembles three units for wider fields and reads the same kernel.

Rust comparison (`piano::smp_unpack` tests): for every width 1..16 and order 0..4, a synthetic residual stream packed MSB-first in big-endian words and decoded by `cbin::sample::decode_nsmp_stk_payload`, and the same residuals packed LSB-first in 16-bit units and decoded by the SMP model with the binomial coefficients, give identical samples; the binomial rows equal `fixed_lpc_predict` for orders 0..4, including full-scale 16-bit history.

Differences found: none in widths or predictor form. Not verified: the ARM-side step that turns the file's big-endian MSB-first words into the DSP's LSB-first 16-bit units, the contents of the PM coefficient vector (ARM data), the end-of-stream handling at the ring edge, and the scale of the decoded integers (the Rust decoder divides by 8192; the DSP shifts by `R15`).

## Implications

- `piano.rs` can use the exact SMP kernel: `piano::q31_kernel::SmpKernel` rebuilds the 6 KB table once from the closed form (no firmware data in the repo). The phase is truncated to 1/256 as the hardware does (`frac >> 24`).
- The shape of the earlier misses (3-term cosine, Kaiser) came from the additive normalisation, not from an exotic window; a division-normalised or un-normalised windowed sinc cannot reproduce this table.
- The default piano interpolation is now the hardware kernel (`SampleInterp::SmpQ31`), with the Catmull-Rom path kept as `SampleInterp::Cubic`.

## Follow-ups

1. Find the ARM code that writes the voice record (width, coefficient pointer, scale `R15`) to confirm the binomial rows and the integer scale.
2. Check how the hardware treats positions below 2 samples from the stream start (history preload) and past the end; the Rust path repeats the edge sample (ASSUMED).
3. Decide whether `synth` `SAMPLES` should also use the SMP kernel for sample playback (the hardware synth path uses the SY1SY2 12-tap set; TD-08).

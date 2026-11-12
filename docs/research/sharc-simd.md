# SHARC SIMD (PEYEN) compute, measured on the simulator

Checked: 2026-11-12. Sources: CCES 2.12.1 ADSP-21489 simulator (headless via `sim_run.py`), probe `docs/research/sharc-seq/gen_simd_probe.py`.
Question: do the model's two processing elements (PEx = R, PEy = S) match the simulator for float and fixed add, subtract, multiply, average, multiply-accumulate and the dual-compute forms, including ASTAT?

## Method
`gen_simd_probe.py` writes `simd.asm`: SIMD on (`PEYEN`), RND32 set and TRUNC clear (what the C runtime leaves), 9 operand pairs (PEx, PEy) x 8 pairs, 15 operations each. Operands come from DM with one SIMD load (`r1 = dm(i1, m5)`, m5 = 2). After each operation the probe stores R4, R8, R6 and `r7 = astat` as (PEx, PEy) pairs: 8 words per case, 8 641 words. `sim_run.py` runs the `.dxe` and `dxe2fixture.py` cuts the code and the operand table out of it; `simd_measured.rs` runs the same machine code on a `Core` and compares every word (`simd_names.json` names the operations).

## Findings (all VERIFIED by `simd_measured.rs`, 8 640 words, 0 differences)
| # | Behaviour | Model before |
|---|---|---|
| 1 | The multiplier flags MN/MV/MU/MI, AF and the sticky bits belong to each element | PEy results updated PEx's ASTAT |
| 2 | `Rn = Rm` copies `Sm` to `Sn` in SIMD mode | `Sn` untouched |
| 3 | `Rn = ASTAT` loads `Sn` with ASTATy; `ASTAT = Rn` loads ASTATy from `Sn` | `Sn` untouched, ASTATy not written |
| 4 | Float ALU ops set AF (ASTAT bit 10), the multiplier leaves it | AF never set |
| 5 | `Ra = Rx + Ry, Rs = Rx - Ry` (fixed and float): AZ, AN, AV, AC are the OR of the sum's and the difference's | flags of the sum only |
| 6 | Float multiply with a NaN operand: all-ones NaN with the sign of the first NaN operand (x before y) | always `0x7FFFFFFF` |

## Not measured (ASSUMED in code)
* A fixed ALU op clears AF (PRM): every case started with ASTAT = 0.
* AS and AI in the dual add/subtract OR; STKY pairing with STKYy for register moves; moves from other universal registers broadcast.

## Implications
Firmware sections run with PEYEN set, so findings 1 to 3 matter for any code that reads ASTAT or accumulates with the multiplier in SIMD mode.

## Follow-ups
Probe SIMD loops and conditions per element, `Rx <-> Sy` swaps, circular buffers in SIMD mode, and SIMD multiply-accumulate rounding of the tonewheel bank (pc 0x122620 on Core 1).

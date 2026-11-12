# SHARC rounding / 40-bit extension measurement (ADSP-21489 simulator)

`round_probe.asm`, `round_probe.c`: probe. `round_measured.csv`: columns `kind,mode1,a,b,v0..v6` (hex), kinds `fix`, `add`, `mul`, `float`; value meaning per kind is in `crates/nord-sharc/src/core/float_measured.rs`. Procedure and findings: `../sharc-float.md`.

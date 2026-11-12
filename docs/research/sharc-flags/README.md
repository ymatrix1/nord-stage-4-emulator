# SHARC float special values, flags and sticky bits (ADSP-21489 simulator)

`gen_flags_probe.py <0..3>` writes `flags_probe_m<i>.asm` / `.c` (one build per MODE1 setting: clear, TRUNC, RND32, ALUSAT; a single build does not fit the simulator's internal data memory). `flags_measured.csv`: columns `kind,mode1,op,a,b,v,resid,astat,stky` (hex). Procedure and findings: `../sharc-float.md`; the checks are `crates/nord-sharc/src/core/float_flags.rs` and the `swap` test in `float_ops.rs`.

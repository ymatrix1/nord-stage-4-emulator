# SHARC sequencer probes (ADSP-21489 simulator)

`gen_loops_probe.py` (arithmetic loops), `dxe2fixture.py` (cuts a probe out of a .dxe), `*_fixture.txt` (machine code), `*_measured.txt` (simulator output), `*_cases.txt` (case list). Procedure and results: `../sharc-loops.md`; checks: `crates/nord-sharc/src/core/loops_measured.rs`, harness `probe_fixture.rs`.

Stack probe: `gen_stack_probe.py`, `stack_fixture.txt`, `stack_measured.txt`, `stack_names.json`; findings `../sharc-stacks.md`, check `stack_measured.rs`.

SIMD: `gen_simd_probe.py`, `simd_fixture.txt`, `simd_measured.txt`, `simd_names.json`; results in `../sharc-simd.md`, check in `crates/nord-sharc/src/core/simd_measured.rs`.

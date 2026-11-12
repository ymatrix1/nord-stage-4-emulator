# SHARC sequencer probes (ADSP-21489 simulator)

`gen_loops_probe.py` (arithmetic loops), `dxe2fixture.py` (cuts a probe out of a .dxe), `*_fixture.txt` (machine code), `*_measured.txt` (simulator output), `*_cases.txt` (case list). Procedure and results: `../sharc-loops.md`; checks: `crates/nord-sharc/src/core/loops_measured.rs`, harness `probe_fixture.rs`.

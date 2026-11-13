# SHARC sequencer probes (ADSP-21489 simulator)

`gen_loops_probe.py` (arithmetic loops), `dxe2fixture.py` (cuts a probe out of a .dxe), `*_fixture.txt` (machine code), `*_measured.txt` (simulator output), `*_cases.txt` (case list). Procedure and results: `../sharc-loops.md`; checks: `crates/nord-sharc/src/core/loops_measured.rs`, harness `probe_fixture.rs`.

## Headless runs (no GUI)

`sim_run.py` runs a probe `.dxe` on the CCES ADSP-21489 simulator through CCES's own `TpsdkServer` API and writes the
output array in the `*_measured.txt` format (about 0.1 s per run, byte-identical to a Memory Browser dump: checked on
the stack probe, 336 words).

```bash
python sim_run.py path/to/probe.dxe --count 336 --output probe_measured.txt
```

It needs CCES 2.12.1 with an active licence (never touched) and re-runs itself under a 32-bit Python 3.6 because
`TpsdkServer` is a 32-bit 3.6 SWIG module: set `NS4_SIM_PYTHON`, or `pyenv install 3.6.8-win32` (pyenv-win), or have
`py -3.6-32`. `--out`/`--exit` take a symbol or address (defaults `_out`, `___lib_prog_term`). Build the `.dxe` as in
`../sharc-loops.md`: `cc21k -proc ADSP-21489 -si-revision any -g -O0 -o x.dxe x.asm x.c`.

Stack probe: `gen_stack_probe.py`, `stack_fixture.txt`, `stack_measured.txt`, `stack_names.json`; findings `../sharc-stacks.md`, check `stack_measured.rs`.

SIMD: `gen_simd_probe.py`, `simd_fixture.txt`, `simd_measured.txt`, `simd_names.json`; results in `../sharc-simd.md`, check in `crates/nord-sharc/src/core/simd_measured.rs`.

Census diff (every firmware instruction against the simulator): `gen_census_probe.py`, `sim_batch.py`, `census_diff.py`, `census_regress.py`, fixtures `census_*.txt`; results `../sharc-census-diff.md`, check `crates/nord-sharc/src/core/census_regress.rs`.

---
# ns4-22y7
title: 'SHARC Nivel 2: suporte a DagMem e DualMove no Cranelift JIT + benchmark DSP'
status: completed
type: task
priority: normal
created_at: 2026-11-16T14:10:41Z
updated_at: 2026-11-16T14:13:41Z
parent: ns4-trkb
blocked_by:
    - ns4-1os5
---

## Context

O JIT Cranelift (`ns4-efor` + `ns4-1os5`) ja compilava loops `DO ... UNTIL` com `Compute`, `Nop`, `LoadImm` e `RegMove`, mas loops reais de audio DSP usam `DagMem` (`DM`/`PM` com DAGs/buffers circulares) e `DualMove` (MAC + 2 loads em paralelo).

## Higher Goal

Habilitar blocos e loops contendo `MoveOp::DagMem` (incondicional, SISD) e `DualMoveOp` no Cranelift JIT, com tratamento exato de `Fault` e auto-invalidacao, alem de benchmark em loop estilo MAC/dot-product.

## Acceptance Criteria

- [x] Suporte a `MoveOp::DagMem` (incondicional, SISD) e `DecodedOp::DualMove` (SISD) no Cranelift JIT (`jit.rs`)
- [x] Tratamento seguro de `Fault` (retorno ao interpretador no PC exato da falha) e checagem de escrita em codigo (`code_modified`)
- [x] Testes de equivalencia estado-por-estado (`Core::run` vs `Core::run_jit`) para `DualMove` dot-product com buffers circulares
- [x] Benchmark `bench_jit_dsp_loop` em `--release --features jit`: `14.71` -> `21.16 Msteps/s` (+44%)
- [x] Checks passando (`fmt`, `clippy` com e sem `jit`, `cargo test -p nord-sharc --features jit --lib`, `check_tests.py`)

## Summary of Changes

- Adicionados `jit_dag_mem` e `jit_dual_move` em `crates/nord-sharc/src/core/jit.rs` com `early_exit` para `Fault` e `code_modified`.
- Adicionados `jit_matches_interpreter_on_dual_move_dot_product_loop` e `bench_jit_dsp_loop` em `jit.rs`.

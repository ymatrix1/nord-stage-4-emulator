---
# ns4-h9tv
title: 'SHARC Nivel 1: benchmark baseline, cache de decodificacao e DecodedOp estruturado'
status: completed
type: task
priority: normal
created_at: 2026-11-16T13:38:33Z
updated_at: 2026-11-16T13:44:17Z
parent: ns4-trkb
---

## Context

O interpretador atual de crates/nord-sharc (Core::step) fazia fetch, classify, word48 e bit-slicing a cada instrucao executada, limitando a vazao de sharc-ref/sharc-pass e sem expor uma representacao estruturada (M1) para um futuro backend JIT/AOT.

## Higher Goal

Acelerar o nucleo SHARC com um interpretador pre-decodificado (Nivel 1) e estruturar o formato intermediario (DecodedOp) reutilizavel pelos Niveis 2 (JIT) e 3 (AOT), mantendo 100% de fidelidade contra os testes e o oraculo.

## Acceptance Criteria

- [x] Benchmark/medicao baseline reproduzivel de steps/s em loop DSP sintetico (release)
- [x] Cache de decodificacao por PC em Core com invalidacao automatica em escritas na memoria de programa/codigo
- [x] Representacao estruturada inicial (DecodedOp / ComputeOp) para os hot paths (compute, nop/idle, dispatch) usada no dispatch de Core::step
- [x] Todos os testes de nord-sharc e gates (fmt, clippy, test, check_tests.py) passando com ganho mensuravel em release

## Out of Scope

- Backend Cranelift JIT (Nivel 2) e tradutor AOT (Nivel 3), que serao beans subsequentes sobre esta base
- Emulador ARM

## Summary of Changes

- Criado crates/nord-sharc/src/core/decode_cache.rs com ComputeOp, DecodedOp e DecodeCache (4096 slots direct-mapped, alocacao lazy, invalidacao por faixa SRAM em dm_write e O(1) por code_gen em load_isa/isa_resync).
- Refatorado Core::step em exec.rs e finish_step em branch.rs para despachar via DecodeCache/DecodedOp e evitar alloc/cond_for desnecessarios em instrucoes incondicionais (cond == 31).
- Adicionado bench_step_throughput (#[ignore]) em exec.rs: 44.78 Msteps/s (baseline) -> 83.25 Msteps/s (+86% em release).
- Verificado com cargo fmt, cargo clippy -p nord-sharc --all-targets --all-features -- -D warnings, cargo test -p nord-sharc --lib (287 passed) e python tools/check_tests.py (799/799 Rust).

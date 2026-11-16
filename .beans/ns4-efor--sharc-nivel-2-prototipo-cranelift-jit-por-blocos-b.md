---
# ns4-efor
title: "SHARC Nivel 2: prototipo Cranelift JIT por blocos basicos C\\\\ do \\Core\\ para operacoes complexas e mantendo 100% de paridade com o interpretador.\n\n## Acceptance Criteria\n\n- [ ] Feature \\jit\\ em \\crates/nord-sharc/Cargo.toml\\ (\\cranelift-jit\\, \\cranelift-module\\, \\cranelift-native\\ opcional, sem impactar builds padrao)\n- [ ] Compilador JIT de blocos basicos sobre \\DecodedOp\\ (lowering direto de \\Nop\\ e \\FixedAlu\\ add/sub/pass/logic + trampoline \\\extern \\C\\\\ para \\ComputeOp\\ e fallback para interpretador)\n- [ ] Testes de equivalencia estado-por-estado entre \\Core::run\\ (interpretador) e \\Core::run_jit\\ em loops \\DO ... UNTIL\\, branches e compute\n- [ ] Benchmark \\\bench_jit_throughput\\ em \\--release --features jit\\ medindo o ganho sobre os 83.25 Msteps/s do Nivel 1\n- [ ] Checks do repo passando (fmt, clippy com e sem \\jit\\, test, check_tests.py)\n\n## Out of Scope\n\n- Lowering inline de \\\float\\ 40-bit e \\Move\\/\\DualMove\\ (proximos PRs)\n- AOT estatico (Nivel 3)"
status: completed
type: task
priority: normal
created_at: 2026-11-16T13:58:14Z
updated_at: 2026-11-16T14:04:03Z
parent: ns4-trkb
blocked_by:
    - ns4-h9tv
---

## Context

O perfil de samply apos o Nivel 1 (ns4-h9tv) mostrou ~55% do tempo no overhead do loop interpretado (`Core::run`/`step`/`finish_step`) e ~39% no dispatch de compute.

## Higher Goal

Implementar um backend JIT hibrido opcional (feature `jit` via `cranelift-jit`) em `crates/nord-sharc` que compila blocos basicos a partir de `DecodedOp` para codigo nativo, chamando helpers `extern "C"` do `Core` para operacoes complexas e mantendo 100% de paridade com o interpretador.

## Acceptance Criteria

- [x] Feature `jit` em `crates/nord-sharc/Cargo.toml` (`cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`, `cranelift-module`, `cranelift-native` opcionais)
- [x] Compilador JIT de blocos basicos sobre `DecodedOp` (lowering inline de `Nop` e `FixedAlu` add/sub/pass/and/or/xor + trampolines `extern "C"` para `FloatMul`/`ComputeOp`/`AV` + auto-loop nativo para `DO ... UNTIL LCE`)
- [x] Testes de equivalencia estado-por-estado entre `Core::run` e `Core::run_jit` (loops `DO ... UNTIL`, overflow `AV` e `ALUSAT`)
- [x] Benchmark `bench_jit_throughput` em `--release --features jit`: 161.06 Msteps/s (vs 83.25 Msteps/s do Nivel 1 e 44.78 Msteps/s do baseline)
- [x] Checks passando (fmt, clippy com e sem `jit`, test --features jit, check_tests.py)

## Out of Scope

- Lowering inline de `float` 40-bit e `Move`/`DualMove` (proximos PRs)
- AOT estatico (Nivel 3)

## Summary of Changes

- Adicionada feature opcional `jit` (Cranelift 0.116) em `crates/nord-sharc/Cargo.toml`.
- Criado `crates/nord-sharc/src/core/jit.rs` com `JitEngine`, lowering IR de `FixedAlu`/`Nop`, auto-loop para `DO ... UNTIL LCE`, `Core::run_jit`, testes de equivalencia e `bench_jit_throughput`.
- Integrada invalidacao por faixa em `decode_cache::invalidate_sram_cache` e visibilidade `pub(super)`/`pub(in crate::core)` necessaria em `core/mod.rs`, `exec.rs`, `fixed_alu.rs`, `float_ops.rs`, `sysregs.rs`.

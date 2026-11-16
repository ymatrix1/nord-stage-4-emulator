---
# ns4-j38k
title: |-
    SHARC Nivel 2: lowering inline de LoadImm/RegMove no JIT e flag --jit em sharc-run C\ no Cranelift JIT, e os binarios CLI ainda nao expunham uma forma de acionar Core::run_jit.

    ## Higher Goal

    Eliminar chamadas extern \C\ para LoadImm (R/I/M/L) e RegMove (R->R) no JIT e adicionar Core::run_auto + flag --jit em sharc-run.

    ## Acceptance Criteria

    - [ ] Lowering inline em Cranelift IR para MoveOp::LoadImm (ureg 0..64: R/I/M/L) e MoveOp::RegMove (R->R preservando fx)
    - [ ] Core::run_auto(&mut self, max_steps) disponivel com e sem feature jit
    - [ ] Flag --jit em sharc-run + teste unitario de parsing/execucao
    - [ ] Testes de equivalencia para LoadImm (R/I/M/L) e RegMove no JIT
    - [ ] Checks passando (fmt, clippy com e sem jit, cargo test --features jit, check_tests.py)
status: completed
type: task
priority: normal
created_at: 2026-11-16T14:15:09Z
updated_at: 2026-11-16T14:17:38Z
parent: ns4-trkb
blocked_by:
    - ns4-22y7
---

## Context

`LoadImm` e `RegMove` ainda chamavam trampolines `extern "C"` no Cranelift JIT, e os binarios CLI ainda nao expunham uma forma de acionar `Core::run_jit`.

## Higher Goal

Eliminar chamadas `extern "C"` para `LoadImm` (`R`/`I`/`M`/`L`) e `RegMove` (`R -> R`) no JIT e adicionar `Core::run_auto` + flag `--jit` em `sharc-run`.

## Acceptance Criteria

- [x] Lowering inline em Cranelift IR para `MoveOp::LoadImm` (`ureg 0..64`: `R`/`I`/`M`/`L`) e `MoveOp::RegMove` (`R -> R` preservando `fx.tag` e `fx.lo`)
- [x] `Core::run_auto(&mut self, max_steps)` disponivel com e sem feature `jit`
- [x] Flag `--jit` em `sharc-run`
- [x] Teste `jit_inline_load_imm_iml_and_reg_move_preserve_state`
- [x] Checks passando (`fmt`, `clippy` com e sem `jit`, `cargo test -p nord-sharc --features jit --all-targets`, `check_tests.py`)

## Summary of Changes

- Lowering inline de `LoadImm` (`R`/`I`/`M`/`L`) e `RegMove` (`R -> R`) em `crates/nord-sharc/src/core/jit.rs`.
- Adicionado `Core::run_auto` em `crates/nord-sharc/src/core/mod.rs` e `pub(super) tag` em `float_ops.rs`.
- Adicionada flag `--jit` em `crates/nord-sharc/src/bin/sharc_run.rs`.

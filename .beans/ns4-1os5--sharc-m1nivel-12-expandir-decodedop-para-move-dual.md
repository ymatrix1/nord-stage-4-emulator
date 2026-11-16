---
# ns4-1os5
title: 'SHARC M1/Nivel 1+2: expandir DecodedOp para Move, DualMove e Flow'
status: completed
type: task
priority: normal
created_at: 2026-11-16T14:05:00Z
updated_at: 2026-11-16T14:09:28Z
parent: ns4-trkb
blocked_by:
    - ns4-efor
---

## Context

`DecodedOp` ja cobria `ComputeOp` (`T2a`/`T2b`/`T2c`), `Nop` e `Idle`, mas `Move`, `DualMove` (`T1a`/`T1b`) e `Flow` (`T8a`/`T9a`/`T9b`/`T11a`/`T11c`/`T12a`) ainda re-extraiam campos via `bits(w, ...)` em tempo de execucao.

## Higher Goal

Expandir `DecodedOp` com variantes estruturadas (`MoveOp`, `DualMoveOp`, `FlowOp`) para eliminar bit-slicing em runtime no interpretador e expor esses operandos diretamente para o JIT/AOT.

## Acceptance Criteria

- [x] Variantes estruturadas `MoveOp`, `DualMoveOp` e `FlowOp` em `decode_cache.rs` com decodificacao unica em `DecodedOp::from_insn`
- [x] Dispatch direto em `Core::step` via `exec_move_op`, `exec_dual_move_op` e `exec_flow_op`
- [x] Suporte no JIT (`jit.rs`) para `MoveOp::LoadImm` e `MoveOp::RegMove` incondicional
- [x] Testes unitarios para `MoveOp`/`DualMoveOp`/`FlowOp` e equivalencia JIT vs interpretador
- [x] Checks passando (`fmt`, `clippy` com e sem `jit`, `cargo test -p nord-sharc --features jit --lib`, `check_tests.py`)

## Out of Scope

- Lowering inline de DAGs/`DM`/`PM` em Cranelift IR

## Summary of Changes

- Adicionadas `DualMoveOp`, `FlowOp` e `MoveOp` em `crates/nord-sharc/src/core/decode_cache.rs` + testes unitarios.
- Adicionados `exec_dual_move_op` (`dualmove.rs`), `exec_flow_op` (`branch.rs`) e `exec_move_op` (`moves.rs`), conectados em `Core::step` (`exec.rs`).
- Adicionado suporte a `MoveOp::LoadImm` e `MoveOp::RegMove` no JIT (`jit.rs`) + teste `jit_matches_interpreter_on_load_imm_and_add_block`.

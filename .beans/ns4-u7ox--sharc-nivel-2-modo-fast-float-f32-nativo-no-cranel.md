---
# ns4-u7ox
title: "SHARC Nivel 2: modo fast_float (f32 nativo) no Cranelift JIT + --fast-float C\\ (jit_float_mul) era o principal gargalo restante no JIT (161.06 Msteps/s).\n\n## Higher Goal\n\nAdicionar um modo opt-in \\\fast_float: bool\\ em \\Core\\ (e flag \\--fast-float\\ em \\sharc-run\\) que emite \\\fmul\\/\\\fadd\\/\\\fsub\\ \\\f32\\ nativos em IR Cranelift no JIT, mantendo o modo 40-bit exato como padrao.\n\n## Acceptance Criteria\n\n- [ ] Campo \\pub fast_float: bool\\ em \\Core\\ (padrao \\\false\\) usado na chave de cache do \\JitEngine\\`n- [ ] Lowering inline em IR Cranelift de \\FloatMul\\ (\\"
status: completed
type: task
priority: normal
created_at: 2026-11-16T14:56:29Z
updated_at: 2026-11-16T14:58:51Z
parent: ns4-trkb
blocked_by:
    - ns4-j38k
---

## Context

No perfil de samply, a emulacao de float 40-bit via `extern "C"` (`jit_float_mul`) era o principal gargalo restante no JIT (`161.06 Msteps/s`).

## Higher Goal

Adicionar um modo opt-in `fast_float: bool` em `Core` (e flag `--fast-float` em `sharc-run`) que emite `fmul`/`fadd`/`fsub` `f32` nativos em IR Cranelift no JIT, mantendo o modo 40-bit exato como padrao.

## Acceptance Criteria

- [x] Campo `pub fast_float: bool` em `Core` (padrao `false`) usado na chave de cache do `JitEngine`
- [x] Lowering inline em IR Cranelift de `FloatMul` (`0x30`) e `FloatAlu` `FADD`/`FSUB` (`0x81`/`0x82`) quando `fast_float == true`, atualizando `flags.az`, `flags.an`, `flags.av`, `flags.ai`, `flags.as_`, `fx.lo` e `AF` (`sys.astat_extra[0]`)
- [x] Flag `--fast-float` em `sharc-run`
- [x] Teste `jit_fast_float_matches_interpreter_on_normal_f32_ops` + `bench_jit_fast_float_throughput` em `--release --features jit`: **769.00 Msteps/s** (**17.17x** sobre o baseline de 44.78 Msteps/s)
- [x] Checks passando (`fmt`, `clippy` com e sem `jit`, `cargo test -p nord-sharc --features jit --lib`, `check_tests.py`)

## Summary of Changes

- Adicionado `pub fast_float: bool` em `Core` (`mod.rs`) e flag `--fast-float` em `sharc_run.rs`.
- Adicionado `emit_fast_float_op` e chave `(start_pc, loop_end, simd, fast_float)` em `crates/nord-sharc/src/core/jit.rs`.
- Adicionados `jit_fast_float_matches_interpreter_on_normal_f32_ops` e `bench_jit_fast_float_throughput`.

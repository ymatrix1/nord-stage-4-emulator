---
# ns4-c882
title: 'Perf: audio block cost toward the PRD budget (TD-14)'
status: in-progress
type: task
created_at: 2026-11-14T20:14:18Z
updated_at: 2026-11-14T20:14:18Z
parent: ns4-xc5n
---

## Context
`ns4 bench` (B13, `ns4-4ok5`) measured the block render cost on a Windows 11 machine: worst case (7 layers, 88 keys held, every FX rack on, master compressor, 4-output routing) 3.9 to 4.4 ms per 128-frame block at 48 kHz (deadline 2.667 ms, below real time on one core), typical program 1.05 to 1.1 ms, PRD budget 0.8 ms (TD-14). The rules in `.agents/rules/dsp-and-realtime.md` apply: no allocation or lock in `process_block`, deterministic rendering.

Also: `audio::control::tests::midi_controllers_reach_the_shadow_but_notes_do_not` overflowed the 2 MB debug test-thread stack.

## Higher Goal
The emulator runs the worst case comfortably in real time and a typical program within the PRD budget, without changing a bit of the audio output (`ns4 bench` prints a bit-exact output hash that gates every optimisation).

## Acceptance Criteria
- [x] A real profile of `ns4 bench` (sampling helper on Windows without admin rights, symbolised from the PDB, libm callers resolved) names the hotspots
- [x] Every optimisation is bit-identical: hashes of `ns4 bench --scenario worst|typical|idle` equal before and after (`0xc7ee671ec250785e`, `0x64043873f4f68dd0`, `0xf65ba453b6c42325`), checked against a build of the pre-change source
- [x] One commit per optimisation, each with before/after numbers and the unchanged hash
- [x] Zero allocations and locks in the audio path kept (`tests/no_alloc_audio_path.rs`, `nord-plugin` `host_lifecycle` green)
- [x] Worst case in real time on the test machine: mean 2.2 to 2.5 ms per block (was 3.9 to 4.4), 1.1x to 1.2x realtime
- [x] Typical program within the PRD budget: 0.67 to 0.70 ms (was 1.05 to 1.1)
- [ ] Worst case mean under 1.6 ms and p99 under 2.6 ms (mean 2.2 to 2.5 ms; p99 4 to 10 ms on a host shared with other jobs)
- [ ] Worst case within the 0.8 ms PRD figure (needs bit-changing options, see TD-14)
- [x] The debug stack overflow in `midi_controllers_reach_the_shadow_but_notes_do_not` is fixed by boxing the control-thread engines, no `#[ignore]`, no `RUST_MIN_STACK`
- [x] PRD budget status, TD-14 and this bean updated

## Out of Scope
- Changes that alter the audio output (table or polynomial sines, SIMD across layers with different summation order, multi-threaded layer rendering): listed in TD-14 with the evidence they would need
- Compiler flags that narrow the supported CPUs (`target-cpu`, AVX2); fat LTO with one codegen unit was measured and gave no gain
- Plugin or GUI performance

## Summary of Changes
- `nord-gui`: `ProgramControl::shadow`, the test rig's live engine and the test clone are boxed (122 KB `Stage4Engine` copies on the debug stack).
- `synth/fw_coef_memo.rs`: per-voice memo of the firmware filter prologue and of the strip cutoff law (bit-exact).
- `organ/drawbar_sum.rs`, `organ/drawbar.rs`: drawbar gains computed once per frame, foldback as a 256-entry compile-time table of the same function.
- `organ/wheel_cache.rs`, `engine/mix.rs`: Organ B takes Organ A's legacy tonewheel waveforms when model and all 91 phases match bit for bit.
- `organ/mod.rs`, `organ/tonewheel.rs`: a silent organ layer only advances its phases (no sines, no pickup/crosstalk stage).
- `phase_wrap.rs`: `wrap_pos`, an exact inline replacement of `x % m` for phases, applied to 28 per-sample wraps.
- `piano.rs`, `synth/render.rs`: constant decay `exp` values once per frame, vibrato `powf` skipped at 2^0.
- `fx/fw_cabinet.rs`, `fx/fw_amp_stage.rs`: the 400-tap cabinet convolves four frames and both channels per pass (linear history, same split and addition order as the circular buffer).
- `fx/mod.rs`: `apply_morph` borrows the morph table.
- `bench.rs`, `ns4 bench --scenario worst|typical|idle`.
- Result on this machine (release, noisy host, alternating runs): worst case mean 3.9 to 4.4 ms to 2.2 to 2.5 ms (p50 3.65 to 2.1); typical 1.05 to 1.1 to 0.67 to 0.70 ms; idle with all racks 1.5 to 1.9 to 0.8 to 0.9 ms. Details and what remains: `docs/TECH_DEBT.md` TD-14, `docs/HOST_COMPATIBILITY.md` 7.1.

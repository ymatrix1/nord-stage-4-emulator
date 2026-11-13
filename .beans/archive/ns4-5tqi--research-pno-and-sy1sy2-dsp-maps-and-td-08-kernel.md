---
# ns4-5tqi
title: 'Research: PNO and SY1SY2 DSP maps and TD-08 kernel check'
status: completed
type: task
created_at: 2026-11-12T21:23:44Z
updated_at: 2026-11-12T21:23:44Z
parent: ns4-trkb
---

## Context

The PNO (piano core 1) and SY1SY2 (synth) DSP images are the reference for a 1:1 replication of `crates/nord-core/src/piano.rs` and `synth.rs`. The SY1SY2 image holds 18,432 floats at DM `0xE3400` (TD-08).

## Higher Goal

Guide the Track B replication: know what each DSP image computes before porting it.

## Acceptance Criteria

- [x] `docs/research/pno-dsp-map.md`: piano core 1 structure, vectors, mailboxes, buffers, record layouts, routine census
- [x] `docs/research/sy-dsp-map.md`: synth voice structure, command FIFO, kernel set meaning
- [x] Kernel table checked numerically (`tools/ns4_sinc_check.py`, tests), TD-08 evidence updated
- [x] `tools/sharc_routines.py` routine census with tests
- [x] Python checks green

## Out of Scope

- Changing `crates/nord-sharc/src/core/*` or `crates/nord-core` (window swap in `sinc.rs` is a follow-up)
- Running the voice routines against hardware or ARM-side message decoding

## Summary of Changes

Two research docs, two tools with tests, TD-08 and dsp-image-hunt 5.15 updated. Kernel window is Kaiser beta 8.5 (VERIFIED, 1 ulp); SMP (sampler/piano voice) uses a 6-tap Q31 kernel and the CNSP fixed-LPC decode; PNO core 1 has no sample voices.

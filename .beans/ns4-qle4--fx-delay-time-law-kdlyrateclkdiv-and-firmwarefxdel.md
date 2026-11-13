---
# ns4-qle4
title: 'FX: delay time law (kDlyRateClkDiv) and FirmwareFx::delay_time'
status: completed
type: task
created_at: 2026-11-13T16:45:57Z
updated_at: 2026-11-13T16:45:57Z
parent: ns4-trkb
---

## Context
`kDlyRateClkDiv` writes `c[0x15]`, `c[0x16]`, `c[0x19]` of the delay context (fx-contexts s5.1), but the delay routine `0x1206B0` does not read them; the engine kept its own 128-step time table. TD-10 and OQ21 listed the delay time as undecoded.

## Higher Goal
Wire every firmware-verified FX parameter law end to end behind opt-in switches (ROADMAP 7b.4), without changing defaults or `render_hash`.

## Acceptance Criteria
- [x] ARM half: UI value to `c[0x15]` closed form (handler `0xC010C514`, table of 1501 thresholds, `K = 43.402339`, clamp 61438), 264 captured values reproduced (`tools/fx_delay_time.py check`)
- [x] DSP half: variants `0x1206B0` / `0x1208AF` / `0x120AB5` / `0x120D15` classified; ring variant echo law `trunc(c[0x15]) + 2` and the tap cross-fade measured in the `sharc-fx` oracle
- [x] Tool with tests (`tools/fx_delay_time.py`, `tools/tests/test_fx_delay_time.py`)
- [x] Rust `fx/fw_delay_time.rs` + `fw_delay_table.rs` behind `FirmwareFx::delay_time` and `LayerFxChain::delay_rate_raw`; default off, `render_hash` unchanged
- [x] Spec, research note, TD-10, OQ21, ROADMAP updated
- [ ] Reverb table DM `0x9500A..` and `kCmpResponse` (not attempted; follow-up)

## Out of Scope
Tempo sync path, the 45 ms tap cross-fade in the engine, the full `0x120AB5` instruction model, reverb, `kCmpResponse`.

## Summary of Changes
See `docs/research/fx-delay-time.md`: closed form, provenance, oracle table. New `fx/fw_delay_time.rs`, `fx/fw_delay_table.rs`, `fx/tests_fw_delay_time.rs`, `tools/fx_delay_time.py`.

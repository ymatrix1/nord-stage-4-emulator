---
# ns4-5mix
title: Arp record player search, program arp mode bits, GUI step tie editor
status: completed
type: task
created_at: 2026-11-14T17:15:55Z
updated_at: 2026-11-14T17:15:55Z
parent: ns4-xc5n
---

## Context

Follow-ups of ns4-l0km (PRs #287, #301): (1) find the record player that walks the pattern compiler's `{type, tick, pan, acc}` records to confirm the slot duration in time; (2) decode the arp mode bits (`+136`) of a program layer so Poly / Gate programs load in their mode; (3) a tie editor (`Q`) in the GUI pattern edit pages.

## Higher Goal

Arpeggiator / Gate programs and edits sound and behave like the hardware (1:1 parity).

## Acceptance Criteria

- [x] Search the firmware for the record player (`tools/arm_xref.py`, `tools/arm_emu.py`); result and limits in `docs/research/synth-arp-patterns.md` section 8. **Not found**: nothing but the compiler reads the records; the free-running clock (`0xC01E6368`) and its rate update (`0xC011A51C`) were located and replayed (`tools/ns4_arp_gate.py clock`).
- [x] Re-evaluate the engine tie/gate timing tags. **No change on purpose**: the player was not found, so a replay test of the slot duration is impossible and the tags stay EMPIRICAL; the new evidence and the open timing question are tracked in ns4-rwhu.
- [x] Decode `+136` mode, `+139` KB sync, `+140` range, `+171` direction, `+173` zig zag, `+174` Master Clock (`cbin/synth_arp.rs`, lossless pack) and apply them (`synth_params_arp.rs`); synthetic fixtures; validation against the factory backup with `tools/ns4_arp_modes.py` (1152 layers, no unnamed mode value).
- [x] GUI tie editor: the GATE dial of ARP page 3 has a detent past 8 eighths for `Q`; `PatternParam::Tie` (engine `set_step_tie`), `step_tie` mirror, headless tests. No host parameter exists for the tie words, so the plugin editor ignores it (additive id left for later).
- [x] Docs, OPEN-QUESTIONS, provenance tags.

## Out of Scope

`panel/bay_program` and `panel/ctl.rs` (OLED lane, see ns4-xo2v); changing the engine step time or the rate mapping (ns4-rwhu); a host parameter for the step ties; KB HOLD (a global bit outside the layer block); hardware recordings.

## Summary of Changes

- `cbin/synth_arp.rs`, `synth_params_arp.rs`: arp mode fields decoded and applied; a Gate or Poly program loads in that mode when its RUN bit is on (RUN off still loads as Off).
- `tools/ns4_arp_modes.py` (+ tests): histogram and plausibility check of the fields over a backup.
- `tools/ns4_arp_gate.py clock` (+ tests): replay of the rate update for all 128 dial values.
- `audio/edits/synth/pattern.rs`, `ui_state/synth/pattern.rs`, `layer.rs`: `PatternParam::Tie`, tie detent of the GATE dial, panel mirror of preset ties.
- Docs: research note section 8, `synth-engine.md`, `cbin-and-presets.md`, OPEN-QUESTIONS, README.

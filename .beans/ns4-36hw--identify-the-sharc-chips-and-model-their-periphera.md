---
# ns4-36hw
title: Identify the SHARC chips and model their peripherals
status: in-progress
type: task
created_at: 2026-11-12T22:36:49Z
updated_at: 2026-11-12T22:36:49Z
parent: ns4-trkb
---

**Context**
The ORG/PNO/FX DSP images address `0x31xxxxxx` registers and L2 at `0x20000000`; SY1SY2/SMP boot over a 2148x SPI-slave kernel. The oracle modelled no peripherals.

**Higher Goal**
Track B oracle that runs the DSP images with the ARM-side protocol.

**Acceptance Criteria**
- [x] `docs/research/sharc-chip-identity.md` names the chips with confidence and evidence
- [x] SEC0, SPI2, DMA, TRU, LP1, L2 models in `crates/nord-sharc/src/core/periph/` with unit tests
- [x] 2148x SPI-slave boot DMA: SY1SY2 and SMP kernels load the stream and reach the idle loops
- [x] ORG Core1 SPI/DMA init completes under `--sc57x`
- [ ] Simulator probes P1 to P8 measured (ASSUMED behaviours promoted or corrected)

**Out of Scope**
SPORT/DAI audio DMA, `0x28xxxxxx` core aliases, `sharc-org` wiring.

**Summary of Changes**
See the PR.

# Research: Nord Stage 4 Mainboard & Panel Photo Analysis

- **Checked date:** 2026-11-10
- **Sources (local, git-ignored):** `.reference/mainboard/*.webp` (5 repair-post photos, third-party images, not redistributed); `.reference/11870-Lg.jpg`, `.reference/12916-Lg.jpg` (Syntaur listing, Stage 3-family).
- **Question:** What silicon and controls does a real Nord Stage 4 contain?

## 1. Board identification

| Marking | Where | Meaning |
|---|---|---|
| `Nord Stage 4 Main Vers 0.9 CLA45D` | Main board silkscreen | Stage 4 main board revision 0.9 |
| `Moderkort NS4 0.95 DS40095` | Main board label (`Moderkort` = mainboard) | Production label, v0.95 |
| `NORD STAGE 4 Panel 1 Rev B Ver 1.4` | Lower panel board | Drawbars, 2 sliders, OLED, encoder, buttons |
| `NORD STAGE 4 Panel 2 Rev B Version 1.3` | Upper panel board | ~20 encoders, OLED, buttons |
| `Moderkort NS3.2.15`, `NORD STAGE 3SW Panel 1` | `11870-Lg.jpg`, `12916-Lg.jpg` | **Stage 3-family**, not Stage 4 |

## 2. Main board silicon

| Item | Evidence | Confidence |
|---|---|---|
| TI `AM1705DPTP4` (ARM926EJ-S, 176-pin HLQFP, no DSP core) | Legible package marking | High |
| ≥ 4 Analog Devices QFP DSPs | `ANALOG DEVICES` legible on Stage 4 photo (`657785317_*.webp`); on sibling `NP4 Main v 0.72` (`c0840e60-74b3-11eb-baf8-a528ac652aed.jpg`) paired with `AM1705DPTP4`, chips read `ADSP-21489 KSWZ-4B` (`#1823`, `4259498.1B-0.2`, 400 MHz SHARC®, 176-LQFP) | Medium on NS4 (High on NP4) |
| 1 large central QFP | Logo illegible | Unknown |
| 8 + 2 memory ICs | Visual only | Low (type unknown) |
| Buck-converter + trimpot mods on 3 DSPs | Wired on, non-stock | High (repair mod) |

Open: four DSP chips vs three firmware images (`DSPO`, `DSPS`, `DSPF`). See [ns4-firmware-teardown.md](ns4-firmware-teardown.md). The architecture treats DSP partitioning as high-level emulation ([D2](../DECISIONS.md)) so this does not block any phase.

## 3. Panel inventory (feeds [visual-style-and-panel.md](../specs/visual-style-and-panel.md))

- Panel 1: 9 drawbar sliders with LED ladders, 2 additional sliders with LED ladders, OLED, 1 push encoder, button matrix, red Shift-style button.
- Panel 2: ~20 rotary encoders with LED rings, OLED, large button/LED groups.

## 4. Next evidence to find

- FCC filing for Clavia (internal photos).
- Zoomed chip-top photos of the four DSPs and the central chip.
- AM1705 datasheet: TI SPRS657.

## 5. Service manual sources

- `pdf-nord-stage-3-service-manual-overview_compress.pdf` is byte-identical (same SHA-256) to `785351857-Nord-Stage-Service-Manual-v1-1.pdf`: it is the **original Nord Stage** manual (rev 1.1), not Stage 3.
- `pdf-nord-stage-2-service-guide-overview_compress.pdf` (NS2) and `pdf-nord-electro-2-service-manual_compress.pdf` are genuine but cover older hardware; use only for architecture lineage, not Stage 4 facts.


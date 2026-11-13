# B3 tail on the full boot state: ported table, chords, hardware clip

- **Checked:** 2026-11-13
- **Sources (local, never committed):** [arm-boot-rederive](arm-boot-rederive.md) (boot log, corrected tail), `docs/research/data/keydown-organ-keys.csv`, `tools/pno_full_pass.py`, `tools/organ_partials.py`, `tools/emu_vs_hw.py` (video `_zksT69H30c`, factory `.ns4b`, local only), `crates/nord-core/examples/organ_ref.rs`, `ns4` release binary.
- **Question:** with the oracle on the corrected state, does the Rust B3 path (tail table plus key weights) reproduce the oracle chords, and does the hardware clip allow the default flip (D22)?

Tags: **VERIFIED** = reproduced by a command, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. What changed in the repository

| Item | Change |
|---|---|
| [data/b3-tail-gain.csv](data/b3-tail-gain.csv), `crates/nord-core/src/organ/b3_tail.rs` | replaced by the 34-point table of [arm-boot-rederive](arm-boot-rederive.md) s2 (65.4 Hz to 9,952 Hz, smooth low-pass: +7.7 dB at 65 Hz, +8.2 at 131, +4.4 at 440, -2.2 at 1 kHz, -15.7 at 10 kHz), smooth enough that the log-frequency interpolation between points is exact to the table's own scatter (single-wheel and multi-wheel lines agree within 0.1 dB) |
| Tests that encode oracle values | `gain_interpolates_in_log_frequency_and_clamps` (440 Hz +4.42 dB, 300 Hz between +6 and +7 dB), `key_weights_replace_the_legacy_drawbar_gains_on_the_b3_wheels_path` (peak 0.03..0.15 for C4 at 888), `the_tail_puts_the_chord_lines_where_the_oracle_has_them` (oracle lines of wheels 52, 60, 64, 71: 0.02919, 0.02119, 0.01979, 0.01864, all within 1 dB) regenerated |
| Defaults, `render_hash` | unchanged (`B3Wheels` is opt-in; `cargo test --workspace` passes including the render-hash golden) |

## 2. Oracle chords on the corrected state (VERIFIED)

The documented generator was reproduced: boot log (without the two layer-level sentinels, [arm-boot-rederive](arm-boot-rederive.md) s1), the 80 ring `W` words `DM 0x951F1..` zeroed, then the key-down words of the captured keys from `keydown-organ-keys.csv` (model 0, velocity 100), `DM(0xB44C0) = 1.0`, 12,000 passes; Rust `organ_ref --mode b3 --rate 43402.34375 --frames 12000`; `organ_partials.py b3`. Checked first on the earlier state: C4 E4 (G4 not captured) at 888800000, fundamentals within **1.26 dB**, which matches [organ-b3-tail](organ-b3-tail.md) s4 and shows that the +8.2 dB offset of [arm-boot-rederive](arm-boot-rederive.md) s3 belongs to `organ_fb_pass.capture_chord` (live key-down capture instead of the captured table; the exact word that adds the 8.2 dB was not isolated), not to the Rust path and not to the state.

| Chord (captured keys) | Drawbars | Lines | Max |dB| fundamentals | Max |dB| all lines |
|---|---|---|---|---|
| C4 E4 (60, 64; G4 not captured) | 8888 | 8 rows x 4 harmonics | **0.06** | **0.54** |
| C4 E4 A5 (60, 64, 81) | 8888 | 12 rows x 4 | **0.13** | **0.52** |
| C3 C#4 E6 (48, 61, 88) | 8888 | 12 rows x 4 | 5.55 | 0.9 except the two lines below |

The two lines outside 1 dB in the third chord are wheel 36 h2 on wheel 48 (-5.55 dB) and wheel 49 (-1.72 dB): a harmonic of a lower wheel lands on a higher wheel's fundamental, so the sum depends on the relative start phase of two free-running wheels (the same cause as before, not modelled). Every other line of the three chords, including the harmonics up to the 4th, is within 0.9 dB (`b3-tail` fit at 0.1 dB).

**Oracle criterion (at most 1 dB per line on three chords): holds for chords 1 and 2, fails on two start-phase lines of chord 3.**

## 3. Hardware clip (`tools/emu_vs_hw.py --only organ_b3 --confirmed-only`, VERIFIED run)

Emulator minus hardware, dB re RMS (`b3wheels` = `--tone-source b3wheels`, tail on by default):

| Variant | sub | low | mid 0.4-1.5k | pres | brill | RMS | crest | score |
|---|---|---|---|---|---|---|---|---|
| default | +17.5 | -0.2 | +0.3 | -5.7 | +0.5 | +14.9 | 5.9 dB (hw 16.3) | 4.83 |
| **b3wheels, corrected tail** | +16.5 | +1.2 | **-1.1** | -6.7 | -2.2 | **+8.0** | 12.8 dB (hw 16.3) | 5.54 |
| earlier tail (note of PR #158 / `organ-b3-tail` s5) | | | -11.9 | | | | 23.1 | 8.07 |

The mid band is now within 1.1 dB (the 12 dB hole of the earlier table is closed) and the crest factor moves from 23.1 dB to 12.8 dB against 16.3 dB on the hardware (-3.5 dB). **RMS is +8.0 dB (emulator louder) with the rotary at unity, the sub band is +16.5 dB and presence -6.7 dB.** The emulator peak reaches 0 dBFS (clipping) on this clip.

## 4. Decision (D22): `B3Wheels` stays opt-in

| Criterion | Result |
|---|---|
| Oracle: at most 1 dB per line on 3 chords | holds on 2, fails on the 2 start-phase lines of the third |
| Video: mid within about 3 dB | **holds** (-1.1 dB) |
| Video: crest within about 3 dB | borderline, -3.5 dB |
| Video: RMS within about 3 dB, unity rotary | **fails**, +8.0 dB |
| Video: sub, presence | fail (+16.5, -6.7 dB) |

So the flip is not made. The failing items are not in the tone source: the oracle chain now matches the Rust one to 0.1 dB, so the +8 dB RMS and the sub excess are level and low-end effects after the PNO output (the organ layer level and the rotary, [b3-vox-video-gap](b3-vox-video-gap.md) section 4, were assumed 1.0), or the clip's own mastering. The next measurement is the layer-level gain law (below) and the output stage, not the tail.

## 5. Layer-level gains: what is known

The words `DM 0xB2ECF`, `0xB2EDE`, `0xB2EED`, `0xB2EFC` and `0xB2FF0`, `0xB2FFF`, `0xB300E`, `0xB301D`, `0xB302C`, `0xB303B` are all written by the module-parameter sender `0xC022E3E8` at boot: first `1.0` (or `1.3` for `0xB2FFF`), then a negative value (`-0.080`, `-0.065`, `-0.050` = `0xBD4CCCCD`, the constant the level handler `0xC011AC10..0xC011AC9C` uses for a zero level). Measured in the oracle: output is linear in a positive word and silent for any value at or below 0. So **negative = the layer or slot is switched off, positive = its gain** (image default 1.0). `Edt_OrgCom_kLayerLevelOrgA/B` (`0xB1`, `0xB2`, 0..127, default 127) is the setter: for the default level on an active layer the value is 1.0 (ASSUMED for the value mapping; the handler squares a scaled level, `(v * 0x34010204)^2` as a double product, and calls `0xC023195C`, not decoded). Running the handler in the harness was not possible: `0xC011AC10` is the shared tail of a larger routine (it uses `ip` and `r3` set by its caller) and has no caller of its own. No layer-level law is added to the engine; `Farfisa` stays silent in the oracle even with every word of the same sender removed (its processor `0x122B85` needs further state: open).

## 6. Vox and Farfisa line-by-line

| Path | Result |
|---|---|
| Vox (`pno-pass-install-vox.script` + the 12 key weights, documented method) | earlier state: fundamentals within 0.16 dB, lines outside 1 dB only at the coincident frequencies (-3.40 and -2.06 dB). **With the boot log in front: Rust is +4.2 to +4.3 dB louder on every fundamental (oracle 0.00525 against 0.00861 at row 24)**, shape unchanged (coincident rows +0.76 / +2.09 dB). A uniform -4.25 dB comes from a state the earlier replay lacked; the Vox layer's own level word was not found (open). Rust Vox unchanged |
| Farfisa | silent in the oracle with the boot log, with the key-down words of model 2 (`keydown-organ-models.csv` has 30 weight rows in `0x95FFC..0x96173`) and with all 19 words of `0xC022E3E8` removed: the processor needs state beyond the layer gains (lattice table `0x965FC..` is only partly written at boot). No line comparison possible; unchanged |

## Implications

- The Rust B3 tail now reproduces the oracle chords to 0.1 dB on pure fundamentals, and the 12 dB mid hole against the video is gone.
- The default flip needs the output-stage level (RMS +8 dB, sub +16.5 dB): not a tone-source change.

## Follow-ups

1. Find the level between the organ layer and the output (layer level word law, rotary gain) and re-run `emu_vs_hw.py`.
2. Find the Vox level word that costs 4.25 dB on the corrected state and correct the Rust Vox level in the same way as the tail.
3. Farfisa processor state beyond the layer gains.

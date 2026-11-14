# The two boot words `DM 0xB2ECF` / `0xB2FF0`, and the B3 tail on the corrected state

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the boot log of [arm-bulk-tables](arm-bulk-tables.md), `tools/organ_b3_proc.py`, `tools/organ_fb_pass.py`, `tools/organ_partials.py`, `tools/pno_full_pass.py`, `sharc-pass`, `crates/nord-core/examples/organ_ref.rs`, the ARM harness (`arm_edit_capture.py`, `arm_model_paths.py`). Builds on [arm-boot-state](arm-boot-state.md). Derived data: [data/b3-tail-gain-bootstate.csv](data/b3-tail-gain-bootstate.csv) (34 points, derived numbers only).
- **Question:** what does the real model install do with `DM 0xB2ECF` (`0xBDA3D70B`) and `0xB2FF0` (`0xBD851EB8`); with the answer applied, what are the B3 tail and the B3 Bass lines, and how do the Rust paths compare?

Tags: **VERIFIED** = reproduced by a command, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. The two words are layer-level gains (VERIFIED measurement, source of the value ASSUMED)

| Evidence | Result |
|---|---|
| Writers (boot, code hook at `0xC022E3E8`) | `0xB2ECF` is written four times, once per organ chain object (`0x90FFAB40`, `0x90FFD720`, `0x9101D3D0`, `0x9101FF50`; DM `0xB2ECF`, `0xB2EDE`, `0xB2EED`, `0xB2EFC`), all `0xBDA3D70B`, from the call at `0xC022F044` inside the layer constructors (`0xC0122380`, `0xC012245C`); `0xB2FF0` and `0xB2FFF` (`0xBD851EB8`) from `0xC022EDD0`. Each is written once, never again |
| Real model install | the harness `install_model` for B3, Vox, Farfisa, Bass (settled snapshot, 5 installs) sends **no word in `0xB2E00..0xB3100`** and `step_chain` does not either: the install does not rewrite them |
| What the DSP does with them (`sharc-pass`, Bass chord, 2,000 passes, other state unchanged) | output peak is **linear in the word**, 1.0: 0.0679 (= the image default, same as not writing it), 0.5: 0.0339, 0.08: 0.00543, `B2FF0` 0.065: 0.00441; **0, -0.08, -0.5, -0.065: silent**. So both are gains whose image default is 1.0, and the boot writes a negative sentinel |
| Who sets the real value | the layer level edit ids `Edt_OrgCom_kLayerLevelOrgA/B` (`0xB1`, `0xB2`, default 127, [arm-edit-params.csv](arm-edit-params.csv)); the handler at `0xC011AC10..0xC011AC98` forms `(v * 0x34010204)^2` as a double product, stores it in the layer record (`[record + (7 layer + slot + 0x34) * 4]`) and calls `0xC023195C`. It returns early unless `[obj + 0x64]` is 2 or 3, which is why the harness dispatch of `0xB1/0xB2` sends nothing (`b3-vibrato-tail.md`, PR #158, s1) |
| Decision | for the default program (level 127) the real value is the image default 1.0, so **the oracle state is the full boot log with these two words left at the image default** (the form used for every number below). The exact mapping `v -> gain` of `0xC023195C` is not decoded: ASSUMED `(v / 127)^2` from the formula shape (v = 127 gives 1.0) |

## 2. B3 tail re-derived on the corrected state (VERIFIED, `organ_b3_proc.py`)

Method of [organ-b3-tail](organ-b3-tail.md) s3 with the prefix replaced: full boot log (29,758 PNO core 1 words) instead of the constructor re-run and ten settle windows (22,587), same key-down capture, same `subset` / `measure` (6 triples: keys 48 65 96, 40 55 70, 60 75 90, 45 62 100, 52 80 104, 36 57 84; 12,000 passes, `DM 0xB44C0 = 1.0`). The same pipeline on the old prefix reproduces the committed table (65 Hz: +0.36 dB against +0.385). Result [data/b3-tail-gain-bootstate.csv](data/b3-tail-gain-bootstate.csv):

| Hz | committed `b3-tail-gain.csv` (dB) | corrected state (dB) | difference |
|---|---|---|---|
| 65 | +0.4 | +7.7 | +7.3 |
| 131 | -4.5 | +8.2 | +12.7 |
| 196 | -8.2 | +7.9 | +16.0 |
| 262 | -11.8 | +6.9 | +18.8 |
| 392 | -17.3 | +5.1 | +22.4 |
| 523 | -22.2 | +3.4 | +25.6 |
| 784 | -26.1 | +0.4 | +26.5 |
| 1,047 | -24.1 | -2.2 | +21.9 |
| 1,568 | -26.2 | -4.4 | +21.8 |
| 2,093 | -22.8 | -6.1 | +16.7 |
| 3,136 | -21.1 | -8.4 | +12.7 |
| 4,186 | -18.2 | -9.8 | +8.4 |
| 6,000 | -14.4 | -11.8 | +2.5 |

The corrected response is a smooth low-pass (about +8 dB up to 330 Hz, falling to -15.7 dB at 10 kHz); the deep notches near 800 Hz and 1.6 kHz of the committed table are gone. The committed table was measured without the 21,000 boot words that the earlier prefix lacked (filter data written outside the constructor), so it describes a state the real device never has. The mid band at 0.4 to 1.5 kHz, the 12 dB gap against the video of `b3-vox-video-gap.md` (PR #156), is where the old and new tables differ by 22 to 26 dB: the new table moves the Rust path toward the recording, to be checked by `tools/emu_vs_hw.py`, which was **not** run here.

The committed `b3-tail-gain.csv`, `b3_tail.rs` and the three tests that encode old oracle lines (`gain_interpolates_in_log_frequency_and_clamps`, `key_weights_replace_the_legacy_drawbar_gains_on_the_b3_wheels_path`, `the_tail_puts_the_chord_lines_where_the_oracle_has_them`) are **unchanged**: the third needs oracle line amplitudes of the exact documented chord (captured keys 60, 64 only, all other `W` zeroed), and the documented chord generator was not reproduced (below).

## 3. Rust against the corrected oracle, three chords (EMPIRICAL)

Oracle: boot log (corrected) + `organ_fb_pass.capture_chord` (model 0, key-down of the drawbar and percussion items) + `DM 0xB44C0 = 1.0`, 12,000 passes; Rust: `organ_ref --mode b3 --rate 43402.34375 --frames 12000`; `organ_partials.py b3` (fundamentals of the wheel lines, absolute dB, Rust minus oracle). The chord generator used here sits a constant **+8.2 dB** off the documented one (the earlier prefix gives +8.2 dB on every pure fundamental, where [organ-b3-tail](organ-b3-tail.md) s4 reports 0 to 1.5 dB), so the **change between states** is the measurement, not the absolute level.

| Chord | Lines | Rust - oracle, earlier state | Rust - oracle, corrected state | Corrected state plus the new tail table applied to the Rust line |
|---|---|---|---|---|
| C4 E4 G4, 8888 | 11 fundamentals 48..79 | +8.2 to +13.5 dB | -4.5 to -18.0 dB, falling with frequency | **+8.1 to +8.6** on 8 lines, +9.7 to +13.9 on 3 lines (wheels 67, 72, 79 coincide with harmonics of lower wheels) |
| C4 E4 A5, 8888 | 12 lines | +7.7 to +11.6 | -4.5 to -16.9 | +8.1 to +8.3 on 9 lines, +10.2 to +11.8 on 3 |
| C3 C#4 E6, 888 | 9 lines | +2.7 to +11.5 | +0.9 to -17.0 | +8.1 to +8.3 on 6 lines, +2.6, +6.3, +10.2 on 3 |

Reading: with the committed tail the Rust path is 4.5 to 18 dB too quiet against the corrected oracle, growing with frequency (the tail difference of s2); applying the new tail to the Rust line removes the tilt and leaves the same constant +8.2 dB and the same harmonic-coincidence outliers that the earlier state shows. So **the shape gap is closed by the new table; the phase-dependent lines (wheel 36 h2 on 48 and similar) stay open; the absolute offset belongs to the chord generator.** Because the documented generator was not reproduced, the criterion "at most 1 dB per line on three chords" is **not shown to hold**: no claim is made that it does. Defaults and `render_hash` are untouched (no Rust change).

## 4. B3 Bass lines on the corrected state

C4 E4 G4 at 888000000, full log with the two words at the image default (`organ_fb_pass.py` chord, 8,000 passes): **262.7 / 328.4 / 391.2 Hz at 0 / -1 / -2 dB, peak 0.0744**, next line 522.5 Hz at -38 dB, no -14 dB side lines. The earlier-state lines were 0 / -3 / -6 dB with peak 0.0083 and side lines at +/- 7 Hz at -14 dB ([organ-farfisa-bass](organ-farfisa-bass.md) s2 and s4.2 are superseded by this state). The 9 x level and the vanished side lines come from the boot words the earlier prefix lacked. Farfisa and Vox remain silent (key-down weights).

## Implications

- The oracle chain the Rust B3 tail was fitted to was missing most of the boot; the corrected response is 7 to 26 dB higher and smooth.
- Replace `b3-tail-gain.csv` and `b3_tail.rs` with the corrected table only together with a reproduction of the documented chord generator (so the three chord tests can be regenerated) and the video check; neither was done here.
- `Edt_OrgCom_kLayerLevelOrgA/B` is the missing setter of the two gains; the exact value mapping of `0xC023195C` is the next decode.

## Follow-ups

1. Decode `0xC023195C` and apply the level handler (`[obj + 0x64]` in {2, 3}) in the harness so the two words get their real value instead of the image default.
2. Reproduce the documented chord generator (keydown-organ-keys.csv words, `W` zeroed) on the corrected state, regenerate `the_tail_puts_the_chord_lines_where_the_oracle_has_them` and replace the table in the same change; then run `tools/emu_vs_hw.py --only organ_b3`.
3. Re-run the Vox and Farfisa line comparisons (their oracle scripts need the corrected prefix and the key-down weights).

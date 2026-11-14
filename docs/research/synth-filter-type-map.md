# Synth filter type: what `kFltType` installs on the strip

- **Checked:** 2026-11-14 (second pass)
- **Sources (local, never committed):** OS v1.66 `bank0_CODE.bin` / `bank0_SRAM.bin` in the ARM harness (`tools/arm_edit_capture.py` settled snapshot, `docs/research/arm-edit-params.csv`), `capstone` ARM listings of the apply function `0xC00171B0` region and of `0xC01CCF84`, ORG core 2 image with `sharc-org` (`tools/synth_stage_oracle.py`, `tools/synth_filter_lp12.py`), `docs/research/data/program-sweep.json`. Builds on [synth-wave-stages](synth-wave-stages.md) and [pno-module-chain](pno-module-chain.md) s5.
- **Question:** Which strip stage (strip word 1) does each `Edt_Syn_kFltType` value install?

Tags follow `.agents/rules/provenance-in-code.md`. Only addresses, indices and derived values are recorded.

## 1. Short answer

The type value is the **index of the word-1 stage table** (identity), with filter-off = index 6 (null stage). The ARM harness shows the type reaching the strip only as a selector field, not the table index field the sender reads, so the identity is established by the stage responses and the off case, not by one traced store.

| Item | Result | Tag |
|---|---|---|
| Edit ids | `kFltType` `0x133` (0..5, default 1), `kFltEnable` `0x132`, `kFltDrive` `0x137`, `kMode` `0xFA` | VERIFIED (`arm-edit-params.csv`) |
| Apply case | `0xC0017B4C`: `[strip + 0x74] = enable ? type : 6` (run: `kFltEnable = 0` gives 6, `= 1` keeps the type 0..5), then `bl 0xC01CCF84` with `r0 = layer + 0x68` | VERIFIED (write watch over `kFltType` 0..5 and `kFltEnable` 0, 1) |
| Word 1 sender | `0xC01CCF84`: word 1 = `((byte & 0x7F) << 24) \| table[[strip + 0x7C]]`, table `bank0_CODE@0x2F72F0` (7 ids); callers only `0xC0017B60` (type / enable / mode) and `0xC0017C10` (`kFltDrive`). The index field read by the sender is `+0x7C`, the selector written by the apply case is `+0x74`; in the harness `+0x7C` stays 1, so word 1 stays `0x0612566E` for every value. No `Edt_Syn*` id writes `+0x7C` (full write-watch sweep of the filter ids, `kMode`, `kOscWave`) | VERIFIED (run); the link `+0x74 -> +0x7C` not traced |
| Off = index 6 | the selector becomes 6 when the filter is disabled and table entry 6 is the null stage `0x125B79` (the stub the oracle uses for an idle slot): a match that fits the identity reading | VERIFIED (run), reading ASSUMED |

## 2. Table and measured behaviour (VERIFIED, magnitude response of an impulse in `sharc-org`, frame `R = 0.1`, table position index 64)

| Value / index | Stage | Response | Panel type (decoder names, `cbin/synth_preset.rs`) | Layers | Model |
|---|---|---|---|---|---|
| 0 | `0x1253BD` | -10.3 dB at 2 fc, -24.6 dB at 4 fc (12 dB per octave), resonant peak 6 dB at fc | `Lp12` | 218 | **bit exact** (1280 of 1280 words, [synth-stage-models-2](synth-stage-models-2.md)), Rust `FwFilterType::Lp2` |
| 1 | `0x12566E` | -21.3 dB at 2 fc, -49.3 dB at 4 fc (24 dB per octave) | `Lp24` | 118 | ported earlier (Lp4) |
| 2 | `0x12549D` | **solved in [synth-stage-models-9](synth-stage-models-9.md)**: four one-pole sections with a shaped, DC blocked feedback (`0xC02DE` is the shaper table, not a delay line); it computes its own coefficients one call ahead from `DM(-30)` (a log-frequency position) and `DM(-28)`; the earlier "not a low pass" came from a frame without the coefficient words | `Ladder24` (raw 2) | 68 | **bit exact** (1840 of 1840 words), Rust `FwFilterType::Ladder` (opt-in) |
| 3 | `0x1257B2` | flat within a few dB (boost 2 to 6 dB at low resonance index, a -15 dB notch at index 126): an equalizer / notch like module, not a low pass | decoder maps raw 3 to `Lp12` (no factory use known) | - | open |
| 4 | `0x125913` | band pass | `BandPass` | 24 | ported earlier (Bp4) |
| 5 | `0x125A50` | high pass | `HighPass` | 14 | ported earlier (Hp4) |
| 6 | `0x125B79` | null stage | filter off | - | n/a |

The identity reading therefore matches all four types whose stage is a filter (0, 1, 4, 5) in order and name; the two other table entries are not filters of the usual kind, so what the `Ladder24` layers run is entry 2 (settled in batch 9: the first impulse used a frame without the coefficient words the stage stores for the next call). `FwFilterType::from_model` keeps `Ladder24 -> Lp4` (ASSUMED) and now maps `Lp12 -> Lp2`.

## 3. What would close it

1. Trace the store that copies the selector (`+0x74`) into the sender index (`+0x7C`, `layer + 0xE4`), probably at a preset-load or strip commit path outside the edit-id cases.
2. Done (batch 9): `0x12549D` is a ladder with its own coefficient prologue, bit exact; open: the ARM cutoff word for type 2.
3. Characterise `0x1257B2` (notch / peaking) and decide if any panel value selects it.

## Implications

- The most used filter, Lp12 (218 of 442 filter layers), has a verified stage; 374 of 442 filter layers (84.6 %) now run a modelled stage.
- No filter default changed (D22).

## Follow-ups

1. Items 1 to 3 above.

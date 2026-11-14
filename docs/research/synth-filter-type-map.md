# Synth filter type: what `kFltType` installs on the strip (partial)

- **Checked:** 2026-11-14
- **Sources (local, never committed):** OS v1.66 `bank0_CODE.bin` / `bank0_SRAM.bin` in the ARM harness (`tools/arm_edit_capture.py` settled snapshot, `tools/arm_params.py`, `docs/research/arm-edit-params.csv`), `capstone` ARM listings of the apply function `0xC00171B0` region and of `0xC01CCF84`, `docs/research/data/program-sweep.json`. Builds on [synth-wave-stages](synth-wave-stages.md) (the same apply path for the Wave type) and [pno-module-chain](pno-module-chain.md) s5 (strip word 1 table).
- **Question:** Which strip stage (strip word 1) does each `Edt_Syn_kFltType` value install (LP12 / LP24 / Ladder / BP / HP / ...)?

Tags follow `.agents/rules/provenance-in-code.md`. Only addresses, indices and derived values are recorded.

## 1. Short answer

The map from `kFltType` to a stage id is **not closed**. What the runs establish:

| Item | Result | Tag |
|---|---|---|
| Edit id | `Edt_Syn_kFltType` = `0x133`, range 0..5 (six values), default 1; `kFltEnable` `0x132`, `kFltDrive` `0x137` (0..3), `kFltKbdTrk` `0x136` | VERIFIED (`arm-edit-params.csv`) |
| Value handler | `0x133` (and `0x132`, `kMode` `0xFA`) reach the apply case at `0xC0017B4C`: `[layer + 0xDC] = (word + 0x8000) >> 16` (the type, `pc 0xC0017B5C`), then `bl 0xC01CCF84` with `r0 = layer + 0x68` (the strip object). Value 0..5 gives `[strip + 0x74] = 0..5` in the harness | VERIFIED (write watch over the six values) |
| Word 1 sender | `0xC01CCF84` builds strip word 1 = `((byte & 0x7F) << 24) \| table[[strip + 0x7C]]`, table `bank0_CODE@0x2F72F0` (7 ids). The index is the strip field `+0x7C` (= `layer + 0xE4`), **not** the field the `kFltType` case writes (`+0x74`). In the harness it stays 1 for every `kFltType` value, so word 1 stays `0x0612566E` | VERIFIED (run: `[strip + 0x7C]` forced to 0..6 gives the table below) |
| Writer of `strip + 0x7C` | not found: no `Edt_Syn*` id writes `layer + 0xE4` in the harness (write watch over the `Flt` / `Drive` / `Mode` ids, the full `Edt_Syn*` sweep was still running at the time box) and the only static stores to a `+0x7C` field in `0xC0010000..0xC0030000` are `0xC00175D8` (`[layer + 0x7C]`, another field) and struct copies | open |
| Callers of the sender | only `0xC0017B60` (type / enable / mode case) and `0xC0017C10` (`kFltDrive` case) | VERIFIED (scan of the code) |

So the harness cannot show which stage a type selects: the index field is set by a path the edit ids alone do not reach (a preset-load or filter-model path is the candidate, ASSUMED).

## 2. Index to stage table (VERIFIED, run of the sender with `[strip + 0x7C]` forced)

| Index | Word 1 stage | Routine | Status in this repo |
|---|---|---|---|
| 0 | `0x1253BD` | prologue of `0x12566E` with table `0xB0F94`, same loop (ASSUMED, not run) | open |
| 1 (idle) | `0x12566E` | 4-pole low pass | ported, `fw_filter.rs` (LP4) |
| 2 | `0x12549D` | 182 instructions, 77 multiplies, `fext` / `fdep` (delay-line style module in the PNO chain) | open |
| 3 | `0x1257B2` | ladder-like low pass (pole table in [synth-stage-equations](synth-stage-equations.md)) | partial |
| 4 | `0x125913` | band pass | ported (BP4) |
| 5 | `0x125A50` | high pass | ported (HP4) |
| 6 | `0x125B79` | null stage | n/a |

The factory bank uses (program-sweep, layers): Lp12 218, Lp24 118, Ladder24 68, BandPass 24, HighPass 14 (decoded names; the decode has five names for the six `kFltType` values). A candidate reading, **ASSUMED** and not run: `kFltType` value = table index (LP4 at 1 is the default; BP4 at 4 and HP4 at 5 fit the order of the names), with the 12 dB low pass at 0 or 2. Nothing here changes `FwFilterType::from_model`, which stays the existing mapping (TD-11, OQ22).

## 3. What would close it

1. Find the writer of `strip + 0x7C`: watch `layer + 0xE4` over a preset load in the ARM harness (the `Edt_Syn*` sweep above only covers the edit ids), or trace which function stores a `0..6` value before `0xC01CCF84` runs on a loaded preset.
2. Or run the DSP side: install each of the six stages in slot B of the oracle pass with the strip words of one filter setting and compare the magnitude response with the panel types.
3. Model the open stages (`0x1253BD`, `0x12549D`) against the oracle once their role is known.

## Implications

- The filter family already ported (LP4, BP4, HP4) is the table entries 1, 4, 5; the 12 dB low pass of the factory majority (218 layers) is not identified as a stage.
- Do not flip any filter default (D22): the map is partial.

## Follow-ups

1. Close item 1 of section 3, then update `FwFilterType::from_model` and OQ22.

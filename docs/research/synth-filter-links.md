# Synth filter chain: the three open links, measured

Checked: 2026-11-16.

Sources: real firmware run in the unicorn ARM harness (`tools/arm_synth_filter_probe.py`, factory programs recalled at boot); ORG core 2 pass on the SHARC oracle (`tools/synth_filter_links.py`, `sharc-org`); disassembly of `bank0_CODE` (capstone, `sharc-disasm`); Part A video clips scored by `tools/emu_vs_hw.py`. Notes read: [synth-filter-type-corners](synth-filter-type-corners.md), [synth-filter-type-map](synth-filter-type-map.md), [synth-cutoff-voice-law](synth-cutoff-voice-law.md), [synth-knob-laws](synth-knob-laws.md), [synth-super-lp24-clips](synth-super-lp24-clips.md), [org-system-run](org-system-run.md). No firmware, sample or audio content is committed; the CSVs hold derived numbers only.

Question: D108 left three captures open before the per-type cutoff factors (EMPIRICAL) could be replaced by firmware-derived words: (A) how `kFltType` selects the stage and whether any per-type factor exists, (B) what the resonance word `W28 = DM(0xC2926)` holds for a sounding voice, (C) whether the stage runs once or twice per SY frame (`F` or `2 F`).

## 1. A: `kFltType` to stage (VERIFIED run)

Boot the full machine with a factory program in the Store record so the firmware recalls it, watching the strip object `layer + 0x68` (`+0x60 .. +0x90`) and the word-1 sender `0xC01CCF84`:

```
C=target/fw_unpacked
uv run --with unicorn python tools/arm_synth_filter_probe.py $C/bank0_CODE.bin $C/bank0_SRAM.bin \
    --eeprom target/oled/eeprom.bin "<factory program>.ns4p" --out target/synth_filter/p0.json \
    --strips 0x90682ce8 0x9068a0f8 0x9068d438
uv run python tools/synth_filter_type_link.py target/synth_filter/p?.load.json \
    --csv docs/research/data/synth-filter-type-link.csv
```

(strip addresses are the three synth layers of this firmware build, found by the discovery run of the same tool). Six programs (Crunch Synth, Aeon Pad Whl, 4 Osc Funk Bass, Bright Vibe Mk I, Filter Bank Ld, Replica Lead; 18 layers): word 1 of the ORG core 2 strip equals `STAGE_TABLE[file type]` in **18 of 18** layers ([synth-filter-type-link.csv](data/synth-filter-type-link.csv)).

| Fact | Evidence |
|---|---|
| The table is the identity: 0 `0x1253BD`, 1 `0x12566E`, 2 `0x12549D`, 3 `0x1257B2`, 4 `0x125913`, 5 `0x125A50`, 6 `0x125B79` (off) | table at `bank0_CODE@0x2F72F0` read by the sender; the 18 rows |
| Filter off: selector and index are 6, stage `0x125B79` | Bright Vibe layer B, Replica Lead layer B |
| `+0x74 -> +0x7C` is a block copy `stm r3,{r0,r1}` at pc `0xC01BDD60` (`[strip+0x74/+0x78]` to `[strip+0x7C/+0x80]`, `lr 0xC00A03F8`) | write watch, boot and load |
| The constructor sets both fields to 6 (pc `0xC01EDDDC` / `0xC01EDDE0`); the program recall writes `+0x74` at `0xC0017B5C` (apply case `0xC0017B4C`) and the drive `+0x78` at `0xC0017C0C` | write watch |
| No per-type factor exists in the ARM words (cutoff `c1`, resonance `c7`, envelope amount `c13` are type independent) or in the stage table | sweeps `e0` / `e1`, [synth-knob-laws](synth-knob-laws.md) s3 |

New finding: raw type 3 selects the stage `0x1257B2`, which the Rust engine does not model (`filter_model_of_type` maps raw `0 | 3` to LP12). The factory counts by raw type over the 988 filter-on layers are 1: 603, 0: 199, 2: 82, 3: **61**, 4: 27, 5: 16; the earlier note "no factory use known" in [synth-filter-type-map](synth-filter-type-map.md) was wrong.

Not verified: that a panel edit of `kFltType` triggers the same `+0x74 -> +0x7C` copy. The edit-sweep harness is unreliable for this id (dispatching it through `apply_value` with a Q16 cell gave selector values 0, 2, 0xfffff051, 2340: the value read at `[r2]` is not the harness cell), so no evidence is taken from it. The program recall path is clean.

## 2. B: resonance word to table position (VERIFIED oracle)

`kFltRes` writes the float `w = knob / 127 - 0.5` to common word 7 (`DM(0xE02EF)`, layer A). The pass smooths it once per pass, `w10 <- 0.995 w10 + 0.005 w`, so the table position after pass `n` is

    idx(n) = 64 + 128 w (1 - 0.995^(n + 1))

```
cargo build --release -p nord-sharc
uv run python tools/ns4_dsp_ldr.py target/DSP/dsp_image_2_ORG.bin --sram DIR
NS4_SHARC_ORG=target/release/sharc-org NS4_ORG_IMAGE=DIR/core2_sram.bin \
    uv run python tools/synth_filter_links.py res --csv docs/research/data/synth-res-position.csv
```

| knob | oracle n=59 | model n=59 | oracle n=1499 | model n=1499 | settled `128 knob / 127` |
|---|---|---|---|---|---|
| 0 | 47.377 | 47.377 | 0.035 | 0.035 | 0.000 |
| 32 | 55.754 | 55.754 | 32.269 | 32.269 | 32.252 |
| 64 | 64.131 | 64.131 | 64.504 | 64.504 | 64.504 |
| 96 | 72.508 | 72.508 | 96.738 | 96.738 | 96.756 |
| 107 | 75.388 | 75.388 | 107.818 | 107.819 | 107.843 |
| 127 | 80.623 | 80.623 | 127.965 | 127.965 | 128.000 |

The oracle equals the model to 0.001. The old law `64 + 33.25 w` ([synth-filter-type-corners](synth-filter-type-corners.md) s2) is the transient at pass 59 (`128 (1 - 0.995^60) = 33.25`): it was read after 60 passes, long before the smoother settles. A 6000 pass run gives 127.999 at knob 127 (clipped by the table end) and 120.944 at knob 120 (`128 * 120 / 127`). The settled position is **`128 knob / 127`, clipped at 127.999**, within one index of the engine default `TablePos::from_resonance` (`127 r`).

A sounding voice adds nothing. The position is identical (to 0.01 over passes 10 .. 1499) with `c13` (filter envelope amount) 0, 0.5, 1.0, with `c13 = 1` plus a filter envelope decay, with `c2` / `c3` = 0.5, with `c8` (LFO amount) = 0.5, with strip word `s0w20` in {1.0, 0, 2.0, 0.5} and with pokes of `s0w10`. The ARM writes `c8 = 0.5` for kLFOAmt 127, `c13 = knob / 127` for the envelope amount, and never `s0w10` or `s0w20` (apart from a load constant `s0w20 = 1.0`).

Consequence: the engine default `TablePos::from_resonance` was right to 1 index and the opt-in `--synth-fw-resonance` / `--synth-filter-law` (47 .. 81) were wrong, which is why they scored worse in D108.

## 3. C: stage calls per pass (VERIFIED calls; pass = 2 frames ASSUMED)

```
NS4_SHARC_ORG=... NS4_ORG_IMAGE=... uv run python tools/synth_filter_links.py calls \
    --script flat.script --csv docs/research/data/synth-stage-calls.csv
```

`flat.script` is the real ARM load script (`docs/research/org-system-run.md`) with the pass column zeroed; the 16 slot records `DM(0xC0B8C + 38 k)` are poked with the stage entry and `sharc-org --trace` counts the entry per pass.

| stage | steps per pass | calls per pass |
|---|---|---|
| LP12 `0x1253BD` | 5737 | 16 |
| LP24 `0x12566E` | 6377 | 16 |
| Ladder24 `0x12549D` | 7113 | 16 |
| type 3 `0x1257B2` | 6649 | 16 |
| BP `0x125913` | 6281 | 16 |
| HP `0x125A50` | 6281 | 16 |

Every stage entry runs once per slot per pass. Earlier work ([synth-super-lp24-clips](synth-super-lp24-clips.md) s7): each call handles 4 samples and the oscillator advances `note / (2 F)` per sample, so the stage rate is `2 F = 86,804.69 Hz`; a pass is two frames (envelope display time ratio 1.00, [synth-knob-laws](synth-knob-laws.md) s3). The display Hz is `1.971 x F atan(R) / pi`, so at `2 F` the engine factor is `2 / 1.971 = 1.0147`. The step counts say how heavy each stage is (slot work), not a different rate. Still ASSUMED: that a pass is two frames on the ear (a hardware corner sweep at a fixed `R` would confirm).

## 4. Engine path

`--synth-fw-filter` (opt-in, no default change, `render_hash` unchanged) is the shortcut for `--synth-filter-law 1.015`: one nominal cutoff for every type at the `2 F` unit and the settled resonance position `min(128 r, 127.999)` through `fw_filter_law::resonance_for_table`. `--synth-fw-resonance` now applies the same settled position. Tests: `fw_filter_law.rs` (settled law, clip, within one index of the default map), `render_opts.rs` (`synth_fw_filter_is_a_shortcut_for_the_measured_chain`).

## 5. Clips

`tools/emu_vs_hw.py`, Part A video, audio-detected notes, gap score (lower is better), 12 synth and layered clips, one run per variant, the same `ns4` release build (this PR). Variants come from `docs/research/data/emu_vs_hw_segments.json`; `filter-law-2f` is `--synth-filter-law 1.015`, `synth-fw-filter` is `--synth-fw-filter` (the same unit, the new variant name).

```
uv run python tools/emu_vs_hw.py --video <Part A .webm> --backup <factory .ns4b> --ns4 target/release/ns4 \
    --workdir target/clips/work --out target/clips/result.json --only synth_lead synth_crunch synth_silkpad \
    synth_filterbank synth_sineplk synth_megaline synth_horizon synth_12db synth_phat3 synth_80s synth_leaky pianopad_grand
```

| Clip | default (fitted) | `--synth-filter-law 1.015` (D108: 3.88) | `--synth-fw-resonance` (D108: 3.20) | `--synth-fw-filter` |
|---|---|---|---|---|
| `synth_lead` | **0.80** | 3.50 | 0.79 | 3.50 |
| `synth_crunch` | **0.89** | 0.89 | 0.90 | 0.89 |
| `synth_silkpad` | **1.64** | 4.51 | 1.64 | 4.51 |
| `synth_filterbank` | 5.39 | 7.61 | **5.32** | 7.61 |
| `synth_sineplk` | **4.73** | 7.14 | 4.73 | 7.14 |
| `synth_megaline` | **2.00** | 2.60 | 2.01 | 2.60 |
| `synth_horizon` | **1.89** | 3.18 | 1.89 | 3.18 |
| `synth_12db` | 1.39 | 2.72 | **1.35** | 2.72 |
| `synth_phat3` | **2.38** | 2.40 | 2.38 | 2.40 |
| `synth_80s` | **0.90** | 1.37 | 0.92 | 1.37 |
| `synth_leaky` | **1.95** | 2.21 | 1.95 | 2.21 |
| `pianopad_grand` | **2.19** | 2.20 | 2.19 | 2.20 |
| mean of 12 | **2.18** | 3.36 | 2.17 | 3.36 |

Reading:

- The default reproduces the D108 baseline (2.18).
- The **settled resonance position** removes the whole `--synth-fw-resonance` regression: 3.20 to 2.17, the same as the default within noise (two clips 0.04 to 0.07 better, none worse than 0.02). The corrected map is what the engine default already had (within one table index), so this is the expected result and a confirmation of link B, not a gain.
- The **uniform `2 F` cutoff** still scores worse than the fitted factors (3.36 against 2.18; D108 3.88, the improvement is the Ladder24 clips and `synth_horizon`, which no longer suffer from the transient resonance). Link C does not close the gap either. It is not the resonance (now neutral) and not the type link (identity, no factor). What remains is the **cutoff factor per row and type** of `p_table.rs` / `p_type.rs`: the clips with the largest loss under the uniform factor are the LP24 and SUPER ones (`synth_lead` 0.80 to 3.50, `synth_silkpad` 1.64 to 4.51, `synth_sineplk` 4.73 to 7.14, `synth_filterbank` 5.39 to 7.61), exactly where the fitted row factors (0.45 .. 2.0) and the SUPER LP24 4.4 act. Those factors multiply the display Hz of a program voice; the measured chain has no word that does. The row factor is therefore still a fit of something between the display Hz and the strip `R` that the three links do not cover (the ARM cutoff law per wave row, the oscillator pitch and P words, or the `R` to Hz taper, ASSUMED). The clips stay on the fitted factors (D22).
- Not re-run: the `F` reading (`--synth-filter-law 0.507`): D108 scored it 5.42 against 3.88 for `2 F`, and the stage-rate counts of s3 favour `2 F`.

## 6. Implications

- The `kFltType` link is closed for programs: the type picks the stage by an identity table and nothing else. The fitted per-type factors (`p_type.rs`) are therefore compensation for something outside the three links (see s5), not a firmware word.
- The resonance map of the engine default is right (settled `128 knob / 127`); the 47 .. 81 range of D108 is retired.
- The stage `0x1257B2` (raw type 3, 61 factory layers) needs a Rust model: its coefficient laws are open.

## 7. Follow-ups

- Model the stage `0x1257B2` (oracle trace and corner table like the other four); until then raw type 3 plays as LP12.
- Capture the hardware corner at fixed `R` (a cutoff sweep clip) to settle `F` against `2 F` on the ear.
- Confirm the edit-time `+0x74 -> +0x7C` commit with a harness that dispatches the real UI message instead of `apply_value`.
- The Ladder24 position word `DM(-30)` is untouched here (it stays ASSUMED).

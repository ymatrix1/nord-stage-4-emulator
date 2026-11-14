# Organ tonewheel weights from the real ARM code

> **Correction (2026-11-13, model attribution):** [organ-model-paths](organ-model-paths.md) (VERIFIED, run) shows that the 92-wheel bank `0x122620`, its mix stage `0x12268F` and the weights / drawbar footage mapping are installed by the **Pipe organ models (Pipe1 / Pipe2)**, not by the B3 model: wherever this note says "B3 tonewheel bank" or "B3 tonewheels", read "Pipe organ bank (models Pipe1 / Pipe2)". The bank algorithm, ladder, mapping and weights stay VERIFIED; only the model attribution changes (the model order matches the UI: ASSUMED). B3, Farfisa and B3 Bass install no generator in PNO core 1 (tone source not located, [OPEN-QUESTIONS](../OPEN-QUESTIONS.md)). Rust: `ToneWheelSource::ModelDefault` applies the bank to Pipe1 / Pipe2 only.

Checked 2026-11-13.

Sources: `bank0_CODE.bin` / `bank0_SRAM.bin` v1.66 (disassembly with capstone, `unicorn` runs from the settled steady state of [arm-organ-messages](arm-organ-messages.md) section 10), the SHARC oracle (`sharc-organ`, PNO core 1 image), [organ-oracle-audio](organ-oracle-audio.md), [organ-tonewheel-bank](organ-tonewheel-bank.md). Only addresses, offsets, derived indices and message words are recorded; no firmware bytes.

Question: where do the 92 wheel weights `a`, `b`, `wB` (and `wA`) of the PNO core 1 mix stage come from, and what is the drawbar to per-wheel mapping (footage offsets, foldback, key range, tapering, curve)?

Tools: `tools/arm_organ_weights.py` (`keymap`, `verify`, `curve`, `script`), `tools/organ_weights_replay.py`. Tables: [organ-key-wheel-map.csv](data/organ-key-wheel-map.csv), [organ-weight-curve.csv](data/organ-weight-curve.csv), [organ-weights-replay.csv](data/organ-weights-replay.csv), scripts `data/arm-replay-organ-keys-a.script`, `-keys-b.script`, `-vibrato-on.script`.

## 1. Answer in short

| Question | Answer | Status |
|---|---|---|
| Why were all drawbar words 0 in section 10? | Not missing SPI tables. The weights are **per key**: a key-down handler marks a per-note item active, and the update method sums *active keys x drawbars* into 92 integers. With no key down every sum is 0. The per-note tables (wheel and taper per drawbar, 128 notes) are already built in RAM at boot. | VERIFIED (run) |
| Who sends `a`, `b`, `wB`? | Four **bank engine** objects (one per mix-stage set), key-down handler `0xC022C3A8`, update method `0xC022AFB8`, through the proxy vtable slot `0xC022A594` (selectors 3, 4, 1). | VERIFIED (run) |
| Is `wA` (`0x9566E`) sent by the ARM? | **No.** It is DSP state: the mix stage computes `w = a wB + b wA` and stores it back, so with `a + b = 1` `a` is a one-pole smoothing coefficient per frame and `wA` the smoothed weight. | VERIFIED (oracle run, section 4) |
| Drawbar to wheel map | `wheel = fold(note - 24 + offset[j])`, offsets `-12, +7, 0, +12, +19, +24, +28, +31, +36`, fold by octaves into `0..90` (wheel 91 unused). All 128 x 9 = 1152 indices match. The existing ASSUMED Rust offsets and foldback are therefore **correct**. | VERIFIED (run) |
| Tapering | **None** in this engine: every taper byte (128 x 9) reads 127. | VERIFIED (run) |
| Drawbar level | 7-bit value of the 9-entry curve `0, 3, 6, 13, 22, 39, 62, 87, 127`, **linear** into the weight. | VERIFIED (bit-exact, 9 values) |
| Absolute weight | `wB[w] = 2^-23 * sum over keys, drawbars on w of d7 * ((vel * 127) >> 4)`; one key, drawbar 8, velocity 127: `0.0152607`. | VERIFIED (bit-exact, 128 notes) |
| Percussion | Not in the bank weights. `Edt_Org_Preset_kB3PercEnable` only writes a 6-word coefficient block at DM `0xB44B6..0xB44C0`; the per-key percussion path is a second engine family (section 7, open). | VERIFIED (run), role open |

## 2. The bank engines and how the weights are sent

| Item | Value | Status |
|---|---|---|
| Engine table | `[0xC05A0424 + 4 (s + 1)]` for set `s` = 0..3, vtable `0xC05267B4`; `+0x10` proxy object, `+0x14` item container, `+0x18` set index `s`, `+0x28 + 4 j` the nine drawbar indices (0..8), `+0x4C` dirty byte, `+0x4D` run byte, `+0x4E` vibrato-mode byte | VERIFIED (run) |
| Key-down handler | vtable slot 2 (`0xC022C3A8`): `(this, _, &{note, velocity << 8}, flag & 1)`. Stores `velocity` in the note's item (`+0xC`), marks it active (`+8`), then per drawbar sends one partial `a / b / wB` triple (9 x 3 = 27 messages per key) | VERIFIED (run) |
| Update method | `0xC022AFB8` runs `0xC022AFC4` when `+0x4D` is set; with `+0x4C` set it converts the nine indices through the 9-entry table (section 5), zeroes the 92 accumulators, adds `d7[j] * ((velocity * taper) >> 4)` for every active item and drawbar into the wheel `idx[j]`, and sends all `a`, `b`, `wB` words (**276 messages** = 92 x 3) | VERIFIED (disassembly + run) |
| Sender conversion | `0xC001FF18(word, N)` stores the integer as a float with exponent `135 - clz(N)`, i.e. `N * 2^-23` | VERIFIED (scale fit over 5 velocities, rel err 1.2e-7; 128-note bit-exact check) |
| DM addresses | `a = 0x9594F + 0x65 s + i`, `b = 0x95AE2 + 0x64 s + i`, `wB = 0x957DE + 0x5C s + i`, `i` = 0..91 (set 1 confirmed by a note-on on engine 2: `0x959FC`, `0x95B8E`, `0x95882` for wheel 72) | VERIFIED (run) |
| `a` and `b` values | Full update: `a = 0x3B8BF800` (0.0042715), `b = 0x3F7EE810` (0.9957285) on every wheel. Key-down partials: `a = 0x3C1DF380` (0.0096406), `b = 0x3F7D8832` (0.9903594), independent of note and velocity; `a + b = 1` to float precision | VERIFIED (run) |
| Key-down placeholder | The partial `wB` of a key-down uses the stale 7-bit array (default 64 in every slot: `64 * 1008 * 2^-23 = 0.0076904`); the following update replaces it | VERIFIED (run) |

Where the harness stops being the firmware: the nine drawbar indices at `+0x28` are **poked** (`BankDriver.set_drawbars`). No edit id of `arm-edit-params.csv` writes them (248 `Edt_Org*`, `Edt_Sys*`, `Edt_Effects*`, `Edt_Com*` ids, both organ layers, two values each, 1 M-instruction window, memory-write watch on all four engines: 0 hits). The `Edt_Org_Preset_kDb1..9` ids reach the *other* engine family (section 7). The missing link (probably a preset apply or layer activate path) is open. Likewise nothing in the harness calls the key-down handler: the firmware's key event dispatcher is not driven, so the **velocity the organ really passes is ASSUMED 127**.

## 3. Key to wheel map (`organ-key-wheel-map.csv`)

`python tools/arm_organ_weights.py keymap bank0_CODE.bin bank0_SRAM.bin --cache settled.snap --csv organ-key-wheel-map.csv` reads, for each of the 128 notes, the nine wheel indices and nine taper bytes from the engine's item list. Rule fitted and checked against all 1152 entries:

```text
wheel(note, j) = fold(note - 24 + OFFSET[j])        OFFSET = -12, 7, 0, 12, 19, 24, 28, 31, 36   (16', 5 1/3', 8', 4', 2 2/3', 2', 1 3/5', 1 1/3', 1')
fold(w): while w < 0: w += 12;  while w > 90: w -= 12
```

| Note | 16' | 5 1/3' | 8' | 4' | 2 2/3' | 2' | 1 3/5' | 1 1/3' | 1' |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 0 | 7 | 0 | 0 | 7 | 0 | 4 | 7 | 12 |
| 36 | 0 | 19 | 12 | 24 | 31 | 36 | 40 | 43 | 48 |
| 60 | 24 | 43 | 36 | 48 | 55 | 60 | 64 | 67 | 72 |
| 127 | 79 | 86 | 79 | 79 | 86 | 79 | 83 | 86 | 79 |

The threshold 90 is the only value in 70..109 that reproduces the table (checked), so wheel 0 = MIDI 24 and wheels 0..90 are used; wheel 91 never carries a weight (the "91 pairs" of [organ-tonewheel-bank](organ-tonewheel-bank.md) are right, the "92nd is a spare" correction of [organ-oracle-audio](organ-oracle-audio.md) s3 concerns only the mix stage reading all 92). Wheels collide for several drawbars of one key (note 0) and for different keys (a chord): the update **adds** them (section 5, chord check: C4 E4 G4 gives wheel 43 twice the single weight, [organ-weights-replay.csv](data/organ-weights-replay.csv)).

## 4. `wA` is DSP state, `a` the smoothing coefficient

Run with `sharc-organ` (PNO core 1 image, `--script`, `--dump`): `a = 0.1, b = 0.9, wB = 1`.

| `wA` before | calls | `wA` after | prediction `1 - 0.9^(2 calls)` |
|---|---|---|---|
| 1.0 | 3 | `0x3F7FFFFC` (1.0 - 5e-7) | 1.0 |
| 0.0 | 3 | `0x3EEFE6F5` = 0.4686 | 0.4686 (6 frames) |

So per frame `wA <- a wB + b wA` and that is the weight used: the ARM only sends the target `wB` and the coefficient. The ARM values give time constants `1/a` = 104 frames for a key-down and 234 frames for a full update (EMPIRICAL reading; the frame rate of the DSP loop is not tied here). The earlier oracle tool poked `a = weight`, `wB = 1`, which gives the same steady state.

## 5. Drawbar curve and level (`organ-weight-curve.csv`)

Single key 60, 8' drawbar (wheel 36), velocity 127. `wB_bits` is the float the firmware sends, `model_bits` the model below; equal on all 14 rows.

| Drawbar index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| `d7` | 0 | 3 | 6 | 13 | 22 | 39 | 62 | 87 | 127 |
| `wB` | 0 | 3.6049e-4 | 7.2098e-4 | 1.5621e-3 | 2.6436e-3 | 4.6864e-3 | 7.4501e-3 | 1.04542e-2 | 1.52607e-2 |

```text
wB[w] = 2^-23 * sum_{keys k, drawbars j : wheel(k, j) = w} d7[idx[j]] * ((vel_k * taper) >> 4),     taper = 127
```

Velocity: 1, 16, 32, 64, 127 give `1.0598e-4, 1.9227e-3, 3.8455e-3, 7.6909e-3, 1.52607e-2` (the integer shift makes velocity 1 slightly non-proportional). `python tools/arm_organ_weights.py verify` runs all 128 notes with all drawbars at 8 and compares every wheel with the model: **all notes match bit for bit**. Compared with `crates/nord-core/src/organ`: the Rust curve `d7 / 127` is right up to a constant; the absolute scale of the firmware is `1008 * 2^-23 = 1.2016e-4` per `d7` and key at velocity 127.

## 6. Replay into the PNO core 1 image (`organ_weights_replay.py`)

Burst: organ model 3 ([arm-replay-organ-model3.script](data/arm-replay-organ-model3.script): bank increments, mono gains), keys C4 E4 G4 down with drawbars `888000000` (`arm-replay-organ-keys-a.script`: 357 pokes), level `L = 1` (ASSUMED, no ARM message found), 3000 calls; then the drawbar change `008000808` (`-keys-b.script`, update burst only, 276 pokes); then vibrato on (`-vibrato-on.script`: detuned increments from `tools/arm_bank.py --level 1` plus gains 0.75 / 0.375); then model 3 again. 3000 calls (6000 frames) per phase, the last 1500 calls measured.

```bash
python tools/organ_weights_replay.py SRAM/core1_sram.bin --exe target/release/sharc-organ \
    --model3 docs/research/data/arm-replay-organ-model3.script \
    --keys-a docs/research/data/arm-replay-organ-keys-a.script \
    --keys-b docs/research/data/arm-replay-organ-keys-b.script \
    --vibrato-on docs/research/data/arm-replay-organ-vibrato-on.script --csv phases.csv
```

| Phase | Peak (lane x) | Partial amplitude / `wB` (exact bank phases) |
|---|---|---|
| 0 model 3, keys, `888000000` | 0.1151 | 0.998 .. 1.025 (8 wheels; wheel 43 carries `wB = 0.0305`, the sum of two drawbars) |
| 1 drawbars `008000808` | 0.1334 | 0.998 .. 1.006 (9 wheels) |
| 2 vibrato on | 0.1205 | 0.52 .. 1.12 (detuned pair, both lanes) |
| 3 model 3 again | 0.1097 | 0.91 .. 1.005 |

With the ARM weights the audio amplitude of each wheel equals `wB` times about 1.0 (the wheel waveform peak is about 1 in the exact bank): the ARM messages alone make the PNO core 1 bank play the chord with the right drawbar pattern, the colliding wheel doubled, and the drawbar change takes effect after the smoothing time. Status: the mapping is VERIFIED; the absolute output level depends on `L` (ASSUMED 1) and the group gains.

## 7. Second engine family and open items

The organ layers also own four *record* objects (`[0xC055F4B8 + 4 s]`, 128-item lists with **non-uniform taper bytes** `0x25 .. 0xFF`, 79 wheels from index 24; update `0xC00BE348`, key-down `0xC00BF890`, key-up `0xC00BF610`). The `Edt_Org_Preset_kDb` ids land in these records (section 6 of [arm-organ-messages](arm-organ-messages.md)). They send through `0xC022DDC8` into eight 80-word arrays (`0x951F1 .. 0x95470`, stride `0x50`; bases per selector pair `0x951F1, 0x95291, 0x95331, 0x953D1` and the same `+0x50`) and a voice block `0x954E6 .. 0x954F9`, `0x9559C..F`, `0xB4246..0xB4251`. A key-down on record 0 sends 54 messages (floats 0.29 .. 0.64 at `0x9513A .. 0x9515E`, `0x95209 .. 0x95215`). **Not the bank weights**, consumer and role (per-key tapering, key click, percussion or the second tone source) not identified; the item tables with the 79-wheel taper are the likely place to look.

Percussion enable (`Edt_Org_Preset_kB3PercEnable`) writes DM `0xB44B6 .. 0xB44C0` (6 words): off `0x3F7FC654, 0x3F7FFF58, 0x3A694800, 0xB7C00000, 0, 0x3F800000`; on the same four, `0x3EB23278` at `0xB44BE` and `0x3F333332` (0.7) at `0xB44C0`. Reading as filter / envelope coefficients is ASSUMED.

## 8. Rust `WeightMode::Firmware` against the oracle

Checked 2026-11-13. `organ_ref --weights firmware` (`OrganLayer`, percussion, crosstalk, rotary, scanner, key click off; 43,156.4 Hz so that frames equal the DSP frames) against `sharc-organ` driven with the ARM messages of this note: key-down at call 0 (`wB` target, `a = 0x3C1DF380`, `b = 0x3F7D8832` on the held wheels), key-off at frame 8192 (all 92 wheels `wB = 0`, `a = 0x3B8BF800`, `b = 0x3F7EE810`), level `L = 1` (ASSUMED), `wA` starting at 0, the mix-stage smoother states (`0x959B1..B2`, `0x95B40..41`, `0x95B44..45`) poked onto their targets (the harness boots them at 0, which gives an extra startup ramp of the oracle that the running firmware does not have).

```bash
cargo build --release -p nord-sharc --bins -p nord-core --example organ_ref
python tools/organ_weight_compare.py SRAM/core1_sram.bin --oracle target/release/sharc-organ     --rust target/release/examples/organ_ref [--notes ..] [--drawbars ..] [--velocity ..] [--detuned]
```

| Scenario | Level rust/oracle | SNR steady (per lane) | Attack envelope (max / rms, % of peak) | Release envelope |
|---|---|---|---|---|
| C4 E4 G4, 888000000, vel 127 | 1.0000x, 1.0000x | 44.5 / 44.5 dB (57 dB over frames 0..2000) | 0.09 / 0.03 | 0.03 / 0.00 |
| same, detuned table (stereo lanes) | 1.0001x, 1.0001x | 44.6 / 44.5 dB | 2.01 / 0.26 (the oracle starts on the detuned gains, Rust ramps from the exact ones) | 0.03 / 0.00 |
| C4 E4 G4, 808030400, vel 64 | 1.0001x | 45.3 / 45.3 dB | 0.09 / 0.03 | 0.02 / 0.00 |
| C2 C3 E7, 888888888 (high wheels fold) | 1.0010x | 26.9 / 26.9 dB | 0.16 / 0.07 | 0.10 / 0.01 |

Findings:

- The absolute scale is right: with `L = 1` the Rust output equals the oracle output at gain 1.0000 (the old chain was 0.0792x of an oracle fed unit weights, which is 5.2x louder than the firmware weights `0.0152607 * ` table amplitude).
- The ramp with `a = 0.0096` (attack) and `a = 0.0043` (release) reproduces the oracle envelope to 0.1 % of peak; the one-pole recurrence per frame is therefore right, including the post-update use of `wA`.
- The steady SNR is limited by bank phase drift (SNR falls from 57 dB in frames 0..2000 to 43.5 dB in 6000..8000): the 12-TET ladder of the Rust bank against the exact increment words at the assumed frame rate; the wheel gains themselves are exact (pickup calibration bypassed). High notes drift more (26.9 dB for the 888888888 chord).
- Not decided by the evidence: whether the update that follows a key-down in the firmware resets `a` to 0.0043 within a few ms (then the real attack is slower than 0.0096 and the Rust ramp too fast), the key-up `a` (assumed 0.0043), the velocity source and `L`. The default therefore stays `WeightMode::Legacy`; at `L = 1` the firmware level is 14.4 dB below the old chain.

## Implications

- `crates/nord-core/src/organ/drawbar.rs`: `DRAWBAR_OFFSETS` and `foldback_tonewheel` are now VERIFIED (tags updated, test `matches_the_firmware_key_tables`). Wheel 91 stays unused; `NUM_TONEWHEELS = 91` is right for the weights.
- The oracle's `chord_weights` (footage, foldback, `d7 / 127`) was right in shape; the ARM scale is `d7 * 1008 * 2^-23` per key at velocity 127.
- `wA` is DSP state, not a missing ARM write: [organ-oracle-audio](organ-oracle-audio.md) s3 row `wA` and the "not reachable" remark are resolved.

## Follow-ups

1. Rust to-do list (organ weights) - (a), (b), (c), (d) done in section 8 and `docs/specs/organ-engine.md` 3.1.5 (`WeightMode::Firmware`, opt-in; (b) as the opt-in absolute scale with `L` ASSUMED 1); original wording: (a) add a firmware weight mode to `organ/firmware_bank.rs`: `wB[w] = 2^-23 sum d7 * ((vel * 127) >> 4)` with per-key accumulation (chords add on shared wheels) and one-pole smoothing `w += a (wB - w)` with `a = 0.0096` on key-down and `0.0043` on drawbar changes; (b) replace `drawbar_gain` normalisation `0.08 * db_bus_scale` by the firmware scale once the output level `L` and group gains are fixed; (c) foldback: keep the current rule, add a test against `organ-key-wheel-map.csv`; (d) tapering: none in the bank engine (all 127), drop any tapering idea for B3; (e) percussion: model the 6-word coefficient block (section 7) after its consumer is found.
2. Find the writer of the engine drawbar indices at `+0x28` (preset apply, layer activate, or a copy from the record family) and the organ velocity source; drive the real key event dispatcher instead of the key-down handler.
3. Identify the consumer and role of the 79-wheel record family (`0x951F1 ..`, `0x954E6 ..`) in the PNO core 1 image, and the source of `L` (`0x95B42`).
4. Vibrato on: the observed messages for `Edt_Org_Preset_kVibratoEnable` stay zero; the replay uses the `0xC022D9F0` level-1 state (increments and gains) of [arm-organ-messages](arm-organ-messages.md) section 7.

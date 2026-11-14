# Piano level after the voice: where the +12 dB against the video comes from

- **Checked:** 2026-11-13
- **Sources (local, never committed):** Part A demo video and the OS v1.66 factory backup under `.reference/`; `target/release/ns4` (`render-patch`); `tools/emu_vs_hw.py --only piano_dry` and `--only organ_pipe`; `crates/nord-core/src/piano/{smp_voice,smp_page,downstream}.rs`; `tools/arm_edit_capture.py` (booted ARM, filter `kLayerLevelPno|kPedalNoiseLevel|kStringResonanceLevel|Fnc_Volume|kLayerLevelOrg`); [smp-voice](smp-voice.md), [arm-boot-rederive](arm-boot-rederive.md), [pno-full-pass](pno-full-pass.md).
- **Question:** After the firmware voice level ([piano-sound-load](piano-sound-load.md) s9 to s11), which stage of the signal path (SMP output stage, SMP-to-mix bus, layer level, master, piano post stages) makes the emulator piano about 12 dB hotter than the hardware recording on Velvet Grand?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| SMP output stage | `bus += (y << 17) * gain >> 31`; a full-scale stroke (`y = 8191`) at unity gain is 0.49994 of Q31, so a bus word read as a fraction of `2^31` is `(y / 8192) * 0.5` | VERIFIED (`smp_voice.rs` bit-exact against the DSP image; test `downstream::bus_unit_matches_the_engine_unit_times_the_firmware_scale`) |
| Receiver convention of the bus (`2^31` = full scale) | not decoded; the natural reading of a Q31 word | ASSUMED |
| Engine sample path | `y / 8192 * level * 0.55` (legacy constant, no recorded source); the bus convention gives `0.5`, i.e. the engine voice is **0.83 dB hot**, not 12 dB | VERIFIED difference, ASSUMED target |
| Engine stages after the voice (Velvet Grand C4, velocity 100, two voices, gains 0.4506 and 0.4454, strokes peak 0.8467) | layer-only peak 0.41607 against `0.55 * 0.8467 * (0.4506 + 0.4454) = 0.4172`; engine output peak 0.41591 (L), 0.36196 (R). So layer level 1.0 (`level = 127`), master compressor (`amount 0`), per-layer FX (all off), timbre `Off`, dyn comp 0, unison 0 add **0 dB**; string resonance changes the 1 s RMS by less than 1e-5 (0.09952 with and without) | VERIFIED (run, scratch probe) |
| Layer level word | `Edt_PnoCom_kLayerLevelPnoA/B` (`0xB7`, `0xB8`, 0..127, default 127): the ARM client sends **no** DSP message in the booted-ARM capture (same as the organ ids, [arm-boot-rederive](arm-boot-rederive.md) s1: the handler returns early for the harness object); the Velvet Grand program stores 127 (`level 1.0`). The organ law `(v / 127)^2` is ASSUMED to apply; at 127 it is 1.0 either way | VERIFIED (run, no message), ASSUMED (law) |
| `Edt_Sys_kPedalNoiseLevel`, `kStringResonanceLevel` (-6..6, default 0), `Fnc_Volume` (default 127) | no message in the capture; defaults are 0 dB and full volume, so they do not explain a constant offset at defaults | VERIFIED (run) |
| Absolute level of the voice on the hardware bus | C4, velocity 100: peak 0.38 of full scale (-8.4 dBFS) from the bus convention, 0.416 (-7.6 dBFS) in the engine | ASSUMED (receiver scale) |
| Why the video comparison reads +12 dB | the replay velocity, the note set, and the unknown gain of the recording, section 3 | VERIFIED (run) |

**There is no 12 dB downstream gain in the engine.** The only measurable downstream difference is the 0.83 dB sample scale, shipped as the opt-in `PianoLayer::firmware_bus_scale` (`ns4 render-patch --firmware-piano-bus`, `piano/downstream.rs`).

## 2. Single-note level and the velocity law (VERIFIED, run)

`render-patch` of one note, Velvet Grand C4, 1.5 s held, default engine:

| Velocity | Peak dBFS | RMS dBFS (3 s) |
|---|---|---|
| 40 | -23.6 | -36.9 |
| 100 | -7.6 | -24.5 |
| 127 | -3.1 | -21.1 |

The firmware law spans 20 dB between velocity 40 and 127 and the strokes themselves get brighter with velocity.

## 3. Why the segment reads +12 dB (VERIFIED, run; `tools/emu_vs_hw.py --only piano_dry`, hardware RMS -24.5 dBFS)

`render-patch` replays every note at one fixed velocity (100). The hardware performance velocity is unknown, and the recording's absolute gain is unknown.

| Replay | RMS delta emu - hw (dB), all detected notes | RMS delta, `--confirmed-only` | brilliance 3..8 kHz delta | centroid delta Hz |
|---|---|---|---|---|
| velocity 100, default | +12.2 | +9.7 | +5.4 / +6.3 | -71 / -31 |
| velocity 100, `--firmware-piano-bus` | +11.4 | +8.9 | +5.4 / +6.3 | -71 / -31 |
| velocity 100, legacy level | +11.8 | +9.3 | +5.6 / +6.4 | -61 / -26 |
| velocity 64, default | +4.8 | +2.3 | -15.3 / -14.0 | -176 / -142 |
| velocity 64, `--firmware-piano-bus` | +4.0 | +1.4 | -15.3 / -14.0 | -176 / -142 |
| velocity 50 (scratch) | +1.4 | -1.1 | n/a | n/a |

(Note set: first value all detected notes, 299 events; second video-confirmed notes, 169 events; the first column of the table in [piano-sound-load](piano-sound-load.md) s11 is the all-notes run.)

- RMS follows the replay velocity at about 0.37 dB per velocity step (+12.2 at 100, +8.5 at 80, +4.8 at 64, +1.4 at 50, all notes); the hardware RMS matches the engine at about velocity 55 to 60.
- Brilliance moves the other way: +5.4 dB at velocity 100, -15.3 dB at 64, so the **spectrum** says the performance sat near velocity 90 while the **level** says near 55. Both cannot hold for an emulator whose absolute scale is right, so the recording is quieter than the absolute hardware scale by roughly 8 to 10 dB (video capture and mastering gain, not an engine gain).
- Cross-engine yardstick: the organ Pipe path is the firmware bank, absolute-scale VERIFIED against the oracle full pass ([pno-full-pass](pno-full-pass.md): gain 1.00014, 45.3 dB SNR), has no velocity, and reads **+7.3 dB** (`organ_pipe` default, re-run today `--confirmed-only`: +7.3; with `--firmware-fx` +6.2; reverb and the synth section are on in that clip). Against it the piano (confirmed notes) is +2.4 dB (+1.6 dB with the bus scale), inside the 3 dB target. The same clip-level offset applies to both because they share the recording chain.
- The replay at velocity 100 also clips: the segment peak is 0 dBFS in the master clamp (`MasterBus` clamps at 1.0) with 12 dB crest against 24 dB on the hardware. The master clamp is a symptom of the velocity, not a missing attenuation.

## 4. Decision

- `firmware_bus_scale` stays **opt-in**: the oracle verifies the bus word (`y << 17`, Q31 gain) but not the receiver's full-scale convention, and the video check improves only by the 0.83 dB of the constant (+12.2 to +11.4 dB, centroid and brilliance unchanged) while RMS stays far from the 3 dB target at the fixed velocity 100 (D22 requires both). `render_hash` is unchanged.
- No other stage is changed: layer level, master, string resonance, dyn comp, unison are exact at defaults.

## 5. Reproduce

```bash
cargo build --release -p nord-core --bin ns4
uv run python tools/emu_vs_hw.py --video <Part A .webm> --backup <factory .ns4b> \
    --ns4 target/release/ns4 --workdir <scratch> --only piano_dry [--confirmed-only]
# variants of docs/research/data/emu_vs_hw_segments.json: default, legacy-piano-level,
# piano-bus, vel-64, vel-64-piano-bus
```

## Implications

- The remaining piano level gap of the video comparison is a property of the comparison (fixed replay velocity, unknown recording gain), not of a stage of the engine; the +0.83 dB constant is the only engine-side difference found.
- To make the video a level reference, the performance velocity per note must be estimated (the onset strengths of `video_ref.py` are relative only) and the recording gain fixed with the Pipe yardstick.

## Follow-ups

1. Decode the receiver of the SMP bus (SMP serial output -> PNO/FX input slot) to turn the Q31 convention from ASSUMED to VERIFIED; then `firmware_bus_scale` can become the default.
2. Trace the float-to-Q31 gain conversion in the SMP interrupt (the oracle runs use `0x7FFFFFFF` for unity).
3. Decode the layer-level mapping `v -> gain` (`0xC023195C`) for the piano ids `0xB7/0xB8` with a program that stores a level below 127.
4. Replay per-note velocities estimated from the audio and compare the spectrum and the level together.

# B3 clip with routing on: the presence gap, the rotary drive scale, the organ level

- **Checked:** 2026-11-13
- **Sources (local, never committed):** [engine-routing](engine-routing.md) (PR #210, `--firmware-routing`), [organ-output-level](organ-output-level.md), [b3-tail-bootstate](b3-tail-bootstate.md), `tools/emu_vs_hw.py` (clip `_zksT69H30c`, local only), `crates/nord-core/src/organ/rotary.rs`, `ns4 render-patch`. The Vox finding (PR #213, +14 dB before the FX chain) and the Pipe finding (PR #191, legacy bus +14.39 dB) are quoted from the coordinator, not re-measured.
- **Question:** with `--firmware-routing`, `b3wheels` and the corrected tail the clip reads sub -1.6, RMS +3.8, presence -12.5 dB: what is the presence gap, and is there one organ output level that explains B3, Vox and Pipe?

Tags: **VERIFIED** = reproduced by a command, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Presence -12.5 dB: drive? | **Not the drive.** The rotary drive is passed to the engine in the preset unit (0 to 10; this program stores 2.44) while `rotary.rs` uses `gain = 1 + 3 * drive` with the engine default 0.2 (unit 0..1): the engine saturates the organ with gain 8.3 (+18.4 dB) instead of 1.73. The new opt-in `--rotary-drive scaled` (drive / 10) removes that: RMS +3.8 -> **-9.3 dB**, crest 15.6 -> 18.5 dB (hardware 16.3), peak -1.6 -> -11.8 dBFS, but presence stays **-12.0**, brilliance -4.7 | VERIFIED (run) |
| Tail | `--b3-tail off` gives presence -7.4, brilliance +0.2, mid +3.0, RMS -0.6: the corrected tail (low-pass, -6 dB at 2 kHz, -10 dB at 5 kHz) costs about 5 dB of the presence band; the hardware is brighter than the oracle's PNO output chain alone | VERIFIED (run) |
| Vibrato, speed, reverb, rotary on/off | **not rankable here.** The clip driver applies `edits` through a JSON round trip of the program, which **drops the routing** (a no-op edit moves sub from -1.6 to +16.6 dB and presence from -12.5 to -11.3) and changes the program; the earlier non-routing variants (rotary off: presence -2.8 against -6.7 with the rotary on, so the engine rotary costs about 4 dB there) are the only evidence | VERIFIED (negative) |
| One organ output level for B3, Vox, Pipe | **not derived.** Levels needed: Vox about +14 dB before the FX chain (PR #213), Pipe +14.39 dB (legacy bus 0.08 against the ARM weight unit 0.01526, PR #191), B3 with routing: +0 dB with the engine's saturating drive law (RMS +3.8) but **+9.3 dB with the corrected drive scale**. B3 and Vox differ by 4.7 dB; the layer levels of this program (0.795 each, `(101/127)^2 = 0.6325`, -4.0 dB) are of that size, so a model-independent gain plus the program's squared layer level is a candidate, not a result | ASSUMED |
| Default flip (D22) | **not done**: presence -12.0 to -12.5 dB fails (section 4) | decision |

## 2. The rotary drive scale

`cbin/organ_preset.rs` stores `rotary_drive` as `(7-bit / 127) * 10` and `engine/program.rs` copies it unchanged into `OrganLayer::rotary_drive`, whose default is 0.2 and whose law is `tanh((1 + 3 d) x)`. For this program `d = 2.44` gives a gain of 8.3. The scale is not recorded anywhere as 0..10 in the engine; the manual's Drive knob is 0..10, so `d / 10` is the natural engine unit (EMPIRICAL: not checked against a drive sweep of the hardware). `render_opts::apply_rotary_scaled` rescales both organ layers (clamped to 0..1) and is reachable as `--rotary-drive scaled` in `ns4 render-patch`; the default render is unchanged (`render_hash` golden unchanged, no default path touched).

| Variant (routing + b3wheels + corrected tail) | sub | low | mid | pres | brill | RMS | crest (hw 16.3) | peak dBFS |
|---|---|---|---|---|---|---|---|---|
| `route_b3` (engine drive law) | -1.6 | +1.1 | +2.2 | -12.5 | -4.7 | +3.8 | 15.6 | -1.6 |
| `route_scaled` (`--rotary-drive scaled`) | -1.1 | +1.1 | +2.1 | -12.0 | -4.7 | -9.3 | 18.5 | -11.8 |
| `--b3-tail off` | -4.7 | -0.3 | +3.0 | -7.4 | +0.2 | -0.6 | | |

With the right drive scale the emulator is 9.3 dB quiet (the drive's tanh was supplying 13 dB of "level"), and the crest moves from 0.7 dB below the hardware to 2.2 dB above it.

## 3. What is left of the presence gap

The shape that remains is a treble deficit: presence -12.0 and brilliance -4.7 dB re RMS with routing on. Contributions measured: the corrected tail about -5 dB (presence), the engine rotary about -4 dB (earlier non-routing runs), the rest unexplained (about -3 dB). The rotary of the hardware is a firmware unit in the effects DSP ([fx-reverb-leslie](fx-reverb-leslie.md), [fx-type-map](fx-type-map.md) s9: the Leslie drive row is a null stub in the type map), so its horn path (treble) is the next suspect; an engine horn that is 4 dB too quiet or too heavily low-passed by the Doppler lines would give exactly this. Not measured: the real rotary response (no oracle chain of the Leslie routines was run here).

## 4. Decision (D22): `B3Wheels` and `FirmwareRouting` stay opt-in

| Criterion (routing on) | Result |
|---|---|
| sub within ~3 dB | holds (-1.6 / -1.1) |
| mid within ~3 dB | holds (+2.2) |
| crest within ~3 dB | holds (15.6 or 18.5 against 16.3) |
| RMS within ~3 dB | holds with the engine drive law (+3.8, just over), fails with the corrected scale (-9.3) until the organ output level is found |
| **presence within ~3 dB** | **fails (-12.0 to -12.5)** |
| brilliance | -4.7 |
| oracle chords <= 1 dB per line | 2 of 3 chords ([b3-tail-bootstate](b3-tail-bootstate.md)) |

The routing zone-code table is EMPIRICAL ([engine-routing](engine-routing.md)). The failing criterion is presence (and, with the right drive scale, RMS).

## Implications

- The engine's rotary drive is 10 x too high for stored programs; the opt-in flag shows what a correct scale does to level and crest, and why an organ output gain is needed once it is applied.
- The treble gap sits after the PNO output (rotary horn, cabinet), not in the tone source or the tail.

## Follow-ups

1. Make the clip driver's `edits` keep the routing (zone codes survive the JSON round trip) so rotary, vibrato, speed and reverb can be ranked.
2. Measure the Leslie horn response from the effects DSP (the oracle chain of the Leslie routines) and set the engine horn gain and crossover from it.
3. Derive the organ output level from the firmware (the layer level `0xB1/0xB2` path, the organ unit output routines `0x124344/0x1239D0`, `0x124270/0x1238FC`, the FX-chain input gain words) rather than fit it; the candidates above (squared layer level, legacy bus against the ARM weight unit) predict B3 +9.3, Vox +14, Pipe +14.4 dB, so the model-dependent part is the open piece.
4. Then flip `B3Wheels` + tail and `FirmwareRouting` together with a re-baselined `render_hash` (D22).

## Update 2026-11-13 (output law)

The Vox word `0x3F1EB100` is the swell word of the default pedal (`Fnc_Swell = 100`), the layer level law is `(v / 127)^2`, and the program rotary drive is now imported in the firmware unit (`preset / 10`): see [organ-output-law](organ-output-law.md) and D25.

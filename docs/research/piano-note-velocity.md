# Piano: per-note replay velocity of the video, the SMP bus receiver, the piano layer level law

- **Checked:** 2026-11-13
- **Sources (local, never committed):** Part A demo video and the OS v1.66 factory backup under `.reference/`; `target/fw_unpacked/bank0_CODE.bin` (ARM, base `0xC0000000`) and the SMP DSP image (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_raw.py --sram`; ADI `elfdump` listing of pc `0x1C0300..0x1C1580` through `tools/adi_dis.py`, CCES 2.12.1); `tools/arm_piano_load.py` (booted ARM, Bright Grand, note 60, velocity 100: the DSP message list); capstone listing of the ARM code; `tools/piano_velocity.py` (new), `tools/emu_vs_hw.py`, `ns4 render-patch` with per-event velocity (new); [piano-downstream-level](piano-downstream-level.md), [piano-sound-load](piano-sound-load.md) s9 to s11, [smp-voice](smp-voice.md), [organ-output-law](organ-output-law.md).
- **Question:** (1) Which DSP code consumes the SMP bus words, and with which full-scale convention? (2) Can the replay velocity of each note of the Velvet Grand clip be estimated from the audio, and does a per-note replay bring `piano_dry` within 3 dB? (3) What is the law of the piano layer level ids `0xB7` / `0xB8` below 127?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| Where the bus is | The record of the main piano voice is `I3 = 0x93928`; its bus pointer (record word `-18` = DM `0x93916`) is written by the ARM as `0x9404E`. DM `0x9404E..0x94065` (24 words, 12 stereo pairs) is cleared with 12 long-word stores at the start of every audio ISR (pc `0x1C078D..0x1C07AE`), then the voice loops add their `(y << 17) * gain >> 31` words | VERIFIED (ARM message capture, listing) |
| Gain word path | The voice loop reads Q31 gains at record `+0/+1`, the ARM writes floats (`level_q23 / 2^23`) nearby. A conversion loop of the same ISR (57 records of stride `0x36` from `DM(0x9403A)`, pc `0x1C06D7`) turns floats into Q31 with `r7 = FIX f7 BY 31`, i.e. `float * 2^31` (unit float = Q31 unity). Whether this loop writes the gains of the piano records was not traced | EMPIRICAL (listing only) |
| First consumer of the bus | The post stage at pc `0x1C0918..0x1C0A51` reads the bus words (pointers `DM(0x9409A) = 0x9406E`, `DM(0x9409B) = 0x9405E`, `DM(0x9409C) = 0xB2033` in the image), adds two of them (`r0 = r0 + r1`), converts with **`f0 = FLOAT r0 BY -29`** (`r11 = 0xFFFFFFE3`, pc `0x1C0940`) and runs one-pole sections (`0x3F7D70A4` = 0.99, `0x3C23D70A` = 0.01) and a gain `DM(0x94098)` (image value 1.0), then stores floats through `I2 = DM(0xB2028) = 0xB2000` and `I4 = 0xB2033`. `FLOAT BY -29` multiplies the integer by `2^-29` (`nord-sharc` `float_conv::from_int`: exponent shifted by the scale) | EMPIRICAL (listing; not run to the end) |
| Full-scale convention | **Not settled.** The first float stage maps a bus word to `word * 2^-29`, so a full-scale stroke at unity gain (`2^30`) is 2.0 there, 4 times (+12.04 dB) the `2^31 = 1.0` reading of `PianoLayer::firmware_bus_scale`. The later stages (the consumer of the ring `0xB2000..` in the PNO / FX path, the master) are not decoded, so the absolute scale at the output stays open and the convention cannot be turned into a default. An oracle run (voice record with its bus pointer at `0x9404E`) left the bus words and the ring at zero after the interrupt, so no number was measured | ASSUMED (unchanged); decode stopped here |
| Default of the piano bus scale | **Stays 0.55 (legacy); `--firmware-piano-bus` stays opt-in.** `render_hash` unchanged | decision D24 (updated) |
| Replay velocity of the clip | The best single replay velocity for the Velvet Grand clip is about **88** (spectrum score 1.43 against 2.85 at velocity 100 and 5.10 at 64; brilliance +0.5 dB, centroid -71 Hz, all scored bands within 2.7 dB). Its RMS gap is +7.3 dB, the same as the Pipe yardstick (+7.3 dB), so the **piano level is 0.0 dB against the yardstick** (-0.8 dB with the 0.5 bus constant) | EMPIRICAL (run) |
| Per-note velocities | **Not recoverable with this method**: the per-note replay is worse than the uniform one (score 2.93 and 3.48 against 1.43), see section 3 | EMPIRICAL (run) |
| Piano layer level law | Handler `0xC011A7A4` (ids `0xB7`, `0xB8`) is the same routine as the organ's `0xC011AB8C`: same constants `0x34010204` (`1 / (127 * 65536)`), threshold `0x33D6BF95`, mute sentinel `0xBD4CCCCD` (-0.05), same store `[table + 4 (7 layer + slot + 0x34)]` and call `0xC023195C`, so the law is `(v / 127)^2` | VERIFIED (disassembly), live call not run |
| Second note per process in the ARM harness | not attempted (time) | open |

## 2. SMP bus receiver (partial decode)

Facts from the ARM message list of one note (Bright Grand, note 60, velocity 100, `tools/arm_piano_load.py ... --note 60`): the list pointer entries `0x9251A = 0x93928` (record), `0x93916 = 0x9404E` (bus pointer), `0x93928`-based predictor and stream words (`0x9391D..0x93924`), and per voice a float gain at `0x9251E` (extra voice) and `0x9252A` (main voice). The same ISR clears the 12 bus pairs, runs the voice loops (`0x1C07D8`, [smp-voice](smp-voice.md)) and then the post stage above, which is the first reader of the bus.

What this means for the question "is `2^31` = full scale?":

- The only conversion visible is `FLOAT BY -29`, i.e. `2^29` per float unit. If the downstream chain treats the float 1.0 as full scale, a unity-gain full-scale stroke is 2.0, and the engine would be 12 dB too **quiet**, which contradicts the sign of the video gap; if later stages scale by 0.25 or lower, the two readings agree. The sign and size of the later scaling is not decoded.
- Evidence that the convention is not far off: the Pipe organ path is absolute-scale oracle-verified and reads +7.3 dB against the recording; the piano at velocity 88 reads +7.3 dB as well (section 3), so the piano engine unit and the verified organ unit sit at the same distance from the recording chain (EMPIRICAL, one clip each, the velocity is a free parameter).

## 3. Per-note velocity estimate (`tools/piano_velocity.py`)

Method: for each of the 169 video-confirmed onsets, the first 150 ms are analysed at the note's own partials (`k * f0`, `k = 1..10` below 8 kHz): total level `L` and brightness `B` (partials 4 and up over partials 1 to 3, dB). The emulator renders every distinct note (about 60) at 12 velocities (`render-patch`, same program), the same `L` and `B` are measured, and each note gets the velocity whose `B` and gain-corrected `L` fit best; the recording gain is the single offset (grid search) that minimises the cost of the notes without ringing before the onset. `render-patch --events` now accepts a per-event `velocity`.

| Measurement (Velvet Grand, emulator at the 12 grid velocities, 60 notes) | Result |
|---|---|
| Emulator level at velocity 16 / 64 / 100 / 127 (median over notes, dB, arbitrary reference) | 36.6 / 50.6 / 57.9 / 61.6: 25 dB over the range, 0.37 dB per step in the middle, flat above 118 |
| Emulator brightness at the same velocities (dB) | -16.3 / -11.2 / -9.2 / -8.3: flat below 40 and above 118 |
| Hardware level, 169 notes (5 / 25 / 50 / 75 / 95 percent) | 21.8 / 33.6 / 39.8 / 44.5 / 50.2 |
| Hardware brightness (same percentiles) | -17.5 / -10.1 / -6.0 / +0.3 / +6.6 |
| Notes with partials already sounding 6 dB below the post-onset level | 107 of 169 (ringing repeats, doubled octaves, chords) |

The hardware brightness is above the emulator's **maximum** (-8.3 dB at velocity 124) for more than half of the notes, so the brightness cannot anchor the absolute velocity; the level is a velocity and a gain together (0.37 dB per step, 8 dB equals 22 steps). Two estimators (iterated median gain, then a grid gain) gave a gain of -7.5 dB with median velocity 64 and -12.0 dB with median 76: the estimate depends on the estimator, which is the sign that it is not identified.

Replay against the hardware clip (`render-patch` on the 169 confirmed events, deltas emulator minus hardware, bands re RMS; `cmp` of `emu_vs_hw.py`):

| Replay | RMS dB | centroid Hz | sub / low / mid / pres / brill (dB) | air (not scored) | score |
|---|---|---|---|---|---|
| uniform velocity 100 | +9.7 | -31 | +2.5 / +0.2 / -1.9 / +3.2 / +6.3 | +4.4 | 2.85 |
| uniform velocity 92 | +8.1 | -70 | +2.8 / +0.5 / -2.0 / +1.6 / +1.7 | -0.4 | 1.71 |
| **uniform velocity 88** | **+7.3** | -71 | +2.7 / +0.5 / -2.0 / +1.4 / **+0.5** | -2.9 | **1.43** |
| uniform velocity 88, `--firmware-piano-bus` | +6.5 | -71 | same | same | 1.43 |
| uniform velocity 80 | +6.0 | -109 | +3.1 / +0.8 / -2.4 / -0.7 / -4.8 | -5.2 | 2.35 |
| uniform velocity 64 | +2.3 | -142 | +3.2 / +1.1 / -2.7 / -4.5 / -14.0 | -7.5 | 5.10 |
| per-note estimate, iterated gain (-7.5 dB) | +5.1 | -67 | +2.9 / +0.5 / -2.6 / +2.1 / +6.5 | +13.2 | 2.93 |
| per-note estimate, grid gain (-12.0 dB) | +7.7 | -30 | +2.9 / +0.3 / -2.0 / +3.4 / +8.8 | +12.5 | 3.48 |

Attack (median 10 to 90 % rise): hardware 86.9 ms, emulator 65 to 67 ms in every row (the emulator attack is 20 ms faster; notes overlap, so this is only a hint). Stereo width 0.46 hardware, 0.44 to 0.48 emulator.

Reading:

- Replay velocity moves the spectrum and the level together; a **uniform velocity of about 88** reproduces the recording's spectrum (every scored band within 2.7 dB, brilliance +0.5, centroid -71 Hz) and leaves a pure level gap of +7.3 dB, which equals the recording chain offset seen on the Pipe organ (+7.3 dB) and the level-implied gain of the partial analysis (-7.5 dB). That is the same picture as before ([piano-downstream-level](piano-downstream-level.md) s3: spectrum near 90, level near 55), now with one consistent explanation: the performance was around velocity 88 and the recording is about 7.3 dB below the engine's absolute scale.
- The per-note estimate adds scatter (the loud, bright notes it assigns dominate the RMS-normalised spectrum: brilliance +6.5 and +8.8 dB), so the uniform replay is the better piano reference. The weak points are the ringing notes (107 of 169) and a brightness that the emulator cannot reach.

**Piano criterion (item 4):** holds in the relative sense: at the uniform replay velocity 88, spectrum, brilliance and centroid are within 3 dB and the RMS gap equals the Pipe yardstick (0.0 dB after the common chain offset; -0.8 dB with the 0.5 bus constant). It does **not** hold as an absolute RMS number against the video (+7.3 dB), because the recording chain gain is unknown, and it is not demonstrated per note.

## 4. Piano layer level law (static)

```text
0xC011A7A4(this, event, id, &q16):   id 0xB7: [this+0x64] in {0,1}, [event+0xC] in {7, 0xB}
                                     id 0xB8: [this+0x64] in {2,3}, [event+0xC] in {7, 0xC}
  x = float(*value) * 0x34010204      (1 / (127 * 65536))
  if x < 0x33D6BF95 (1e-7): g = 0xBD4CCCCD (-0.05, mute sentinel)  else g = x * x
  [ [this+0x98] + 4 (7 * [this+0x8C] + slot + 0x34) ] = g ; tail call 0xC023195C(record, layer, slot)
```

(`slot = [pool + 4 [this+0x64] + 0x658]`.) Same code as the organ handler ([organ-output-law](organ-output-law.md) s3), so the piano layer gain is `(v / 127)^2`, equal to 1.0 at the stored 127. The engine applies the piano layer level linearly (`engine/mod.rs`, `_ => level`); the squared law is not applied, because it changes every piano program with a layer level below 127 and the live call with a controller object was not run. Open: run the handler with a fake controller (`[this+0x64]`, `[event+0xC]`).

## 5. Reproduce

```bash
# hardware slice and events (emu_vs_hw.py cache): <work>/piano_dry/{hw.wav,events.json}
uv run python tools/emu_vs_hw.py --video <Part A .webm> --backup <factory .ns4b> \
    --ns4 target/release/ns4 --workdir <work> --only piano_dry --confirmed-only   # variants incl. vel-88
uv run python tools/piano_velocity.py --hw <work>/piano_dry/hw.wav --events <work>/piano_dry/events.json \
    --ns4 target/release/ns4 --backup <factory .ns4b> --workdir <work>/ref --out-events <work>/ev_vel.json --confirmed-only
ns4 render-patch "Program/Bank A/Velvet Grand.ns4p" --backup <factory .ns4b> --events <work>/ev_vel.json --wav out.wav
# SMP bus: tools/arm_piano_load.py ... --note 60 (message list); tools/adi_dis.py smp.bin 0x380B00 0x381500
```

## Implications

- The piano default (0.55, firmware level) is within the Pipe yardstick at the replay velocity that matches the spectrum; there is no evidence for an engine-side level change, and the bus-scale default flip remains blocked on the output-side decode (the first float stage is `2^-29`, not `2^-31`).
- A better level reference needs the recording gain fixed independently (one organ or piano note with a known velocity or a calibrated tone) rather than per-note velocity inversion.

## Follow-ups

1. Decode the consumer of the post-stage ring (DM `0xB2000` and `0xB2033`: float words) in the PNO / FX path and the master scaling: the end-to-end bus-to-DAC gain decides `firmware_bus_scale` and whether the engine unit is right in absolute terms.
2. Run the oracle with a unity-gain voice at bus `0x9404E` through the post stage (the bus read as zero after the interrupt in the first attempt; the harness needs the DMA-done flag and the exact post-stage preconditions).
3. Run `0xC011A7A4` with a fake per-layer controller to confirm `(v / 127)^2`, then apply it in `engine/mod.rs` for piano layers behind the firmware level law.
4. Second note per process in the ARM harness (the SMP link handshake: `install_dsp_ready` handles only the first note).
5. Isolated-note references for the per-note velocity (a monophonic passage) to remove the contamination.

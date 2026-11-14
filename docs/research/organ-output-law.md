# The organ output level law: swell word, layer level, bus gain

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66), the boot capture `tools/arm_bulk_tables.py capture` (`boot_log.jsonl`, with the sender pc of every DSP word), an ARM code hook on the callback `0xC0223AEC` during `ast.boot_to_main_loop`, `capstone` disassembly, `tools/arm_edit_capture.py`, `tools/emu_vs_hw.py` (video `_zksT69H30c`, factory `.ns4b`, local only). Builds on [organ-output-level](organ-output-level.md), [organ-vox-click](organ-vox-click.md), [b3-rotary](b3-rotary.md), [pipe-video-gap](pipe-video-gap.md), [arm-boot-state](arm-boot-state.md), [fx-type-map](fx-type-map.md) s9.
- **Question:** is there one organ output level law, derived from the firmware, that lands B3, Vox and Pipe at the hardware level, and which organ firmware paths can become the default?

Tags: **VERIFIED** = reproduced by a command or a test, **EMPIRICAL** = measured on the clips, **ASSUMED** = reading.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| What is the Vox "gain word" `DM 0x95FD1 = 0x3F1EB100` (0.61989)? | **The swell word of the default pedal, not the layer level.** The sender `0xC0223B90` (callback `0xC0223AEC` of the voice engine vtable `0xC0526664`, argument pair `(type 2, value 0x64)` at boot) writes `520 v^2 * 2^-23` for `Fnc_Swell = v` (registry default 100), and `0x7FFFFF * 2^-23` at `v = 127`. `520 * 100^2 * 2^-23 = 0.6198883 = 0x3F1EB100` bit for bit. The twin `DM 0xB44CD` (B3 path) is the same value from the int sender `0xC001FF18` | VERIFIED (disassembly, boot log, code hook, test `swell_word_is_the_boot_word_at_the_default_pedal`) |
| The organ layer level (`0xB1` / `0xB2`) law | **`(v / 127)^2`.** The handler `0xC011AB8C` multiplies the q16 value by `0x34010204` (= `1 / (127 * 65536)`), squares it (`fmul r5, r5`) and stores it at `[table + 4 (7 layer + slot + 0x34)]`; a level of 0 stores the mute sentinel `-0.05` (`0xBD4CCCCD`). The earlier guess `(101 / 127)^2 = 0.6325` for the boot word was wrong only in the source: the boot word is the swell word | VERIFIED (disassembly), the live call with a per-layer controller object was not run |
| Does the engine apply it? | No: `engine/mod.rs` applied the layer level **linearly** (0.795 for the B3 program instead of 0.632), now squared under `OutputLevel::Firmware` | VERIFIED law, engine change |
| The gain between the oracle lanes and the FX chain input | **Not found in the firmware**: no ARM word between the PNO lanes and the FX unit carries it. The FX unit's own input trim words (`c[-0x1B]`) are programme words of the FX modules, not an organ level. It is the one empirical constant, `K = 2.8` (+8.9 dB), centred on the three clips. Update ([piano-receiver-scale](piano-receiver-scale.md) s5, D49): the layer sender writes the organ chain gain words as 1.6 at level 127 (the oracle states leave 1.0), the master level is 1.0; 13.4 dB of the gap against the piano reading remain unexplained | EMPIRICAL |
| Rotary drive unit | The firmware drive is `v / 127` ([fx-type-map](fx-type-map.md) s9, ARM writer `0xC0062D54`); the preset stores `(v / 127) * 10` and `rotary.rs` takes the fraction (`gain = 1 + 3 d`, default 0.2). `apply_program` passed the preset unit, so a stored 2.44 saturated with gain 8.3 instead of 1.73. **Fixed as the default** (`/ 10` on import, `* 10` on export). The firmware has no drive nonlinearity to port (null stub `0x12637B`), so the tanh law and its +18 dB behaviour stay engine-side | VERIFIED unit, ASSUMED law |
| Defaults flipped (D25) | Pipe on the firmware bank takes the firmware level law; **B3Wheels, Vox and FirmwareRouting stay opt-in** (section 5) | decision |

## 2. The swell word (VERIFIED)

```text
0xC0223AEC(this, ?, &type, value)   vtable slot 6 of 0xC0526664; 'type 2' = Fnc_Swell
  -> 0xC0223B90:  v = value & 0xFF
                  r3 = (v == 127) ? 0x7FFFFF : (v * v * 65) << 3 = 520 v^2
                  float(r3) * 2^-23      (the int-to-float of the sender 0xC001FF18)
                  store at DM [table[type] ...] through the PNO comm port
```

Boot callback arguments seen by the hook (the engine objects `0x9021d570`, `0x90223790`, `0x902299b0`, `0x90217350`): types `0xF, 0x10, 0x11` = 0, `2` = `0x64`, `0xD` = `0x7F`, `5` = 0. Type 2 is edit id 2, `Fnc_Swell`, whose registry default is 100 (`arm-edit-params.csv`); type `0xD` carries 127. The write `DM 0x95FD1` has the sender pc `0xC0223C3C` (inside `0xC0223B90`) in the boot log, `DM 0xB44CD` the pc `0xC001FFA0`. Consequences:

- The oracle-faithful Vox chord was 4.15 dB (0.6199) below the unit because the **default pedal is 100 / 127**, not because of a layer level. With the pedal at 127 the word is 1.0.
- The engine models the pedal already (`pedals.organ_swell_gain`, neutral = full), so `OutputLevel::Firmware` removes the baked word from the Vox path (`vox_gain = K / 0.61989`) and leaves the pedal to the engine.
- The manual's B3 floor of 0.1 for the swell is not in this sender (ASSUMED manual figure kept in `pedals.rs`).

## 3. The layer level (VERIFIED, disassembly)

```text
0xC011AB8C(this = layer controller, r1 = event, r2 = id, r3 = &q16):   ids 0xB1 (cases [this+0x64] 0/1, [event+0xC] = 9), 0xB2 (2/3, 10)
  x   = float(q16) * 1.2014794e-7        (= 1 / (127 * 65536))
  out = (x <= 1e-7) ? -0.05 : x * x
  table[7 * [this+0x8C] + [slot] + 0x34] = out ; 0xC023195C(...) sends it
```

The call needs the per-layer controller (`[this + 0x64]`, `[event + 0xC]`) that the harness does not have; the arithmetic is read from the listing, not executed. The squared law is consistent with the swell sender (`520 v^2 / 2^23` is `(v / 127)^2` to 1.8e-4: `520 * 127^2 = 8,387,080` against `2^23`), so the firmware squares both level controls.

## 4. The remaining constant `K` (EMPIRICAL)

Gain still needed between the oracle-faithful organ output (swell and level 1) and the hardware clip (swell at full, which is ASSUMED for the performance; `emu_vs_hw.py`, velocity 100, `--confirmed-only`):

| Clip | Path | Rust RMS vs clip before | Needed `K` (dB) |
|---|---|---|---|
| B3 Chords n Solo (level 0.795, squared 0.632) | `b3wheels`, tail, `--firmware-routing`, drive in the firmware unit | -9.3 dB | +9.3 + 2.0 (squared level) = **+11.3** |
| Vintage Vox J3 (level 1.0) | `vox` | -13.7 dB with the swell word 0.6199 | +14.0 - 4.15 = **+9.9** |
| Space Drama II (Pipe2 + synth, level 0.764, squared 0.583) | firmware weights (ARM unit, `L = 1`) | legacy bus is +14.39 dB over the unit and +7.3 dB over the clip (+8.5 organ alone) | at most 14.4 - 8.5 + 2.3 = **+8.2** |

Centred on the three: `K = 2.8` (+8.9 dB), errors B3 -2.7, Vox -0.7, Pipe +3.3 dB. The spread (3.4 dB) is the performance velocity and pedal that the video does not give; it is not a tuned fit per clip. Where the gain lives in the firmware is open (the FX unit's organ section input, the audio port scale).

## 5. Results (`tools/emu_vs_hw.py`, confirmed notes, emulator minus hardware, dB re RMS)

| Clip | Variant | sub | low | mid | pres | brill | RMS | crest (hw) | score |
|---|---|---|---|---|---|---|---|---|---|
| B3 | default (Legacy, drive fixed) | +17.7 | +0.1 | -0.4 | -7.7 | -1.9 | +6.1 | 14.7 (16.3) | 5.54 |
| B3 | `b3wheels` + routing (drive fixed) | -1.1 | +1.1 | +2.1 | -12.0 | -4.7 | -9.3 | 18.5 | 4.20 |
| B3 | `b3wheels` + routing + `--organ-level firmware` | -1.2 | +1.2 | +2.3 | -19.9 | -13.4 | **-2.7** | **15.9** | 7.62 |
| Vox | default (Legacy) | -3.2 | -0.2 | +8.2 | -1.3 | +2.4 | +8.9 | 15.7 (15.4) | 3.05 |
| Vox | `vox` + `--organ-level firmware` | +2.7 | +3.2 | **+7.6** | -7.5 | -9.6 | **-0.7** | **16.0** | 6.10 |
| Pipe | `--organ-level legacy` (old default) | +4.1 | +3.0 | -0.6 | -5.9 | -11.2 | +7.3 | 14.2 (14.1) | 4.95 |
| Pipe | **default now** (firmware level) | +1.3 | +2.5 | +0.7 | -3.1 | -8.7 | **+3.3** | 13.7 | **3.25** |

Before this change the same B3 default read RMS +14.9 dB with the rotary drive 8.3 (earlier notes); with the unit fixed it reads +6.1 dB.

### Decision (D25)

| Path | Oracle lines | Video RMS | crest | sub | mid | Result |
|---|---|---|---|---|---|---|
| Pipe, firmware bank + level | 0.00 dB, 3 chords | +3.3 (about 3) | -0.4 | +1.3 | +0.7 | **default flipped** (all bands improve; the RMS is 0.3 dB over the bar and the clip also holds the synth) |
| B3Wheels (+tail) | fails: 2 start-phase lines of chord 3 ([b3-tail-bootstate](b3-tail-bootstate.md)) | -2.7 | -0.4 | -1.2 | +2.3 | stays opt-in: oracle criterion; presence -19.9 and brilliance -13.4 dB also fail (the unscaled legacy key click no longer masks a dark tail) |
| Vox | within 0.34 dB | -0.7 | +0.6 | +2.7 | **+7.6** | stays opt-in: mid, presence -7.5, brilliance -9.6 dB |
| FirmwareRouting | n/a | | | sub -17.7 to -1.2 on B3 | | stays opt-in: the zone code table is EMPIRICAL ([engine-routing](engine-routing.md)), no firmware source for the 12 `kKbZone` codes |

`render_hash` (the golden dump uses a B3 layer on the Legacy path and no program) is unchanged: `c7f06ca7ff6f8663be3bf2af772c6d6854f23593808ff3382d1895cf5f5ddbd5`. Real renders change: every program with the rotary on (drive 0.1 times) and every Pipe layer (level).

## Implications

- One structure, three words: `output = K * swell_word(pedal) * level^2 * bank`. Two factors are firmware, one scale is not.
- The Vox "0.6199" and the B3 `0xB44CD` are the swell default, so the Rust Vox path at the real pedal position is 4.15 dB louder than the oracle replay, not equal.
- The `b3wheels` clip result shows the Legacy key click (calibrated against the 0.08 bus) is the only thing that carried B3 treble; with the oracle-scale tone the clip's presence (1.5 to 3 kHz) is a real 20 dB hole to chase (the rotary horn path, [b3-rotary](b3-rotary.md) s3).

## Follow-ups

1. Find where the FX unit's organ section scales its input (`K`): the audio-port DMA descriptors of the PNO to FX hand-over, or the organ unit rows `0xB28xx`.
2. Run `0xC011AB8C` with the real per-layer controller to confirm the squared law dynamically; capture a second swell value (`v = 64`) to check `520 v^2`.
3. Remove or rescale the Legacy key click on the firmware B3 path; re-run the B3 presence.
4. A Pipe-only and a pedal-position-known clip to replace the three-clip centring of `K`.

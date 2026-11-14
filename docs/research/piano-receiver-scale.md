# Piano and organ level: the FX receiver of the SMP stream and the path to the DAC

- **Checked:** 2026-11-14
- **Sources (local, never committed):** FX and PNO images of OS v1.66 (`python tools/ns4_dsp_ldr.py dsp_image_N.bin --sram DIR`, listings with `python tools/adi_dis.py DIR/coreK_sram.bin 0x240000 0x2C0000`, CCES 2.12.1), the L2 descriptor blocks (`coreK_l2.bin`, `0x20004000..`), `target/release/sharc-org` (FX Core1 pass, strict, 0 skipped operations), `target/fw_unpacked/bank0_CODE.bin` with capstone, [piano-bus-convention](piano-bus-convention.md), [dsp-frame-rate](dsp-frame-rate.md) s4, [fx-dsp-map](fx-dsp-map.md), [organ-output-law](organ-output-law.md), `tools/fx_input_path.py` (new).
- **Question:** (1) What receives the SMP serial stream, what multiplies it, and what is the scale up to the codec word? How does the piano level compare with the organ (OQ20, OQ24)? (2) Does the layer level word reach the voice gains (`0xC023195C`)? (3) Stream edges, damper-down strokes.

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| Receiver of the SMP stream | **FX image, both cores, SPORT3B** (DMA channel 7, `DM(0x31022380) = 0x20004260`, CTL `0x31002380`). The descriptor ring at `0x20004260..0x200042FF` (loader stream, L2) is eight 10-word transfers (`CFG 0x44223`, `XCNT 0x0A`), 10 words = the TDM frame of the SMP transmit ring; destinations alternate Core1 / Core2: `0x283008D0`, `0x28B008D0`, `0x283008F8`, `0x28B008F8`, `0x282C02D0`, `0x28AC02D0`, `0x282C02F8`, `0x28AC02F8` (Core1 `DM(0xC0234)`, `DM(0xC023E)`, `DM(0xB00B4)`, `DM(0xB00BE)`). The other RX channels are 2-word x 40-row 2D blocks (PNO output, SPORT1A, 1B, 2A) and a 19 / 21-word block (SPORT0B); PNO has 80 / 40-word RX blocks and no 10-word one | VERIFIED (descriptor words, listing); that the 10-word channel is the SMP stream: EMPIRICAL (frame size equals the SMP ring frame, only 10-word RX in FX and PNO, slots read match ring offsets 0..3) |
| Input stage (FX Core1 `0x1213FB..0x1214B8`, listing offsets `0x13FB..`) | The pass selects the buffer set from `DM(0xC0450)` against `0x28300830` (base `0xC01BC` or `0xB003C`), then copies the SMP slots: `i4 = base + 0x78` (frame A) and `+0x82` (frame B, stride 10), words 0, 1 to `DM(0xC0470..)` and words 2, 3 to `DM(0xC0478..)`. Each word passes `PASS` and `BIT TST ASTATx 0x20` (NaN / Inf becomes 0). **No multiply.** The PNO 2D blocks are summed three-at-a-time into the buses `0xC0458`, `0xC0460`, `0xC0468` (offsets `0, 0x28, 0x50` etc.), also plain adds | VERIFIED (listing) |
| Gain from the SMP words to the master buffers | `sharc-org` (FX Core1, `tools/fx_input_path.py`, words poked every pass): 0.25 on the four SMP words gives `0xC0470 = 0.0123, 0.0698, 0.187, 0.2467` after 50, 150, 400, 1000 passes (a fade-in, like the SMP gate), `0xC2862 = 0.24959` and the master IIR output `0xC2872 = 0.24998` after 1500: the transport is **unity** and the master IIR is unity at DC | VERIFIED (run) |
| Master stage (`0x12284C..0x1228AC`) | `g = smooth(DM(0xB6518)) * DM(0xB6519) * DM(0xB651A)` with a 0.995 one-pole (`0x3F7EB852`); the output words `0xC04D8..` and `0xC04E0..` are the (negated) sum of the master buffers times `g`, clipped at `4 g`. `DM(0xB651A) = 0x3E3F1412 = 0.1866` (image), `DM(0xB6519) = 1.0`, **`DM(0xB6518) = 0` in the image** (output muted); no ARM literal, no boot-log message and no FX store writes it | VERIFIED (listing, image words); the writer is found in s5 (ARM channel-table record, `(v / 127)^2` times a taper) |
| Measured gain with `DM(0xB6518) = 1.0` | SMP words: -0.1869, PNO 2D words: -0.1862 after 5000 / 1500 passes (approach `0.1866` from below with the smoothing and fade ramps; the 0.15 % excess is the IIR overshoot seen in the 0.24998 / 0.24959 ratio). **Piano and organ lanes see the same path** | VERIFIED (run) |
| DAC word (start of the next pass, `0x121321..0x121343`) | `word = fix(clip(2 x, 0.99900)) * 2^31` (`FIX BY 31`, clip constant `0x3F7FBE77`): output words preset to 0.25 give `0x40000000`, -0.2 gives `0xCCCCCCC0`, 0.5 and above give `0x7FDF3B80`. Float full scale of the master buffer is 0.4995 | VERIFIED (run, `fx_input_path.py dac`) |
| Path gain | DAC fraction `= 2 * 0.1866 * x = 0.3732 x`, `x` = ring float (SMP) or lane float (PNO). A full-scale unity stroke (bus `2^30`, ring 2.0) reads 0.7464 of `2^31`; a bus word `2^28` (about the Bright Grand peak) reads 0.1866 | VERIFIED arithmetic on the rows above; the level word `DM(0xB6518) = 1.0` is ASSUMED |

## 2. The piano against the organ (OQ20, OQ24)

Both reach the codec through the same unity path with gain `0.3732`, so the **relative** level is the PNO lane float against the SMP ring float, and no gain `K` sits in the FX unit (OQ24: not between the PNO lanes and the FX input; the 2D PNO blocks are summed, not scaled). With the organ calibration `K = 2.8` ([organ-output-law](organ-output-law.md); lane floats are oracle-faithful):

```text
clip calibration C   = K / 0.3732                 = 7.50   (17.5 dB; recording chain, analog stage)
piano ring float     = 2 * (y / 8192) * g          (bus = y << 17 * g, ring = bus * 2^-29)
piano engine unit    = C * 0.3732 * ring           = 2 K (y / 8192) g = 5.6 (y / 8192) g
engine today         = 0.55 (legacy) or 0.5 (firmware_bus_scale)       (20.2 or 21.0 dB below)
```

So under the verified common path the two clip calibrations disagree by about **20 dB**. Which one carries the error is indicated by the piano stroke peaks: a Velvet Grand C4 stroke at velocity 100 has two voices of gain 0.4506 and 0.4454 and a peak of 0.8467 of `y / 8192` ([piano-downstream-level](piano-downstream-level.md)), so the ring float peaks at 1.517 and the DAC fraction at `0.3732 * 1.517 = 0.566` of full scale (a plausible hardware peak at a loud velocity), against an engine peak of 0.417 (legacy 0.55): the engine is only 2.7 dB (`4 G / 0.55 = 1.357`) below the DAC reading, within the velocity uncertainty of the clip (the replay velocity of about 88 is uniform, [piano-note-velocity](piano-note-velocity.md)). The organ `K = 2.8` would put the same stroke at 4.2 of full scale (12.5 dB over), so the piano **agrees with the unit "engine 1.0 = DAC 1.0"** and the organ `K` is the outlier: the oracle lanes miss about `2.8 / 0.3732 = 7.5` (17.5 dB) of gain that the hardware applies upstream of the FX unit, for instance an ARM-set level (the PNO level word `0x95B42` of [arm-part-activation](arm-part-activation.md), the swell word) that the oracle runs do not set. This is EMPIRICAL (one clip each, velocity free) and not enough for D22; the hardware clips (White Grand, Velvet Grand, Pipe) were not re-run against a changed default.

`receiver_path_sample_scale()` (`piano/downstream.rs`) is `4 G = 0.7464` per `y / 8192` unit at unity gain, 3.5 dB above `FIRMWARE_SAMPLE_SCALE` (0.5) and 2.7 dB above the legacy 0.55. It would replace `firmware_bus_scale` if the engine unit is the DAC fraction (the reading the piano peaks support). It is not wired: the master level word `DM(0xB6518)`, the layer law and the organ `K` are open, and the legacy 0.55 sits inside the clip uncertainty.

## 3. `0xC023195C` and the layer level word (partial)

`0xC023195C(this, layer, slot)` (disassembly in this pass): reads the per-slot cells at `[this + 0x180 + 4 (25 layer + slot)]`, `[+0x248]`, multiplies the layer level word (the `(v / 127)^2` of `0xC011A7A4`, stored at `[table + 4 (7 layer + slot + 0x34)]`) with a float from `[this + 4 (table[idx] + 0x54)]` using the soft-float multiply `0xC0270C80` (and `0xC0270A38` / `0xC0270A3C` for `1 / x` and `1 - x`), and sends **two parameter messages** through `0xC022E2C4` and `0xC022E164` (parameter id `[this' + 8]`, table of 16-byte entries `0xC0547C58`, ids `<= 0x31`), skipping the send when the value equals the last sent word (`[this' + 0x344]`, `+0x40C`). So the layer level reaches the DSP as a message, not as an SMP record write; which chip the id addresses is resolved in s5 (PNO core 1 organ chain gains and FX core 1 / 2 bus gains; the table is RAM filled by the static initialiser, `arm-chan-tables.csv`). `firmware_layer_law` stays opt-in (D22). | EMPIRICAL (disassembly) |

## 4. Not worked on

Stream edge handling, damper-down strokes with the firmware lists, the SMP slew time constant, and the hardware-clip comparison at a new default (nothing was changed that would need one).

## 5. The master level writer, the mixer object and the layer words (follow-up of the same day)

Commands: `uv run --with unicorn python tools/arm_master_level.py run|layer target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap` (settled ARM emulator, [arm-organ-messages](arm-organ-messages.md) s10); the law without firmware: `python tools/arm_master_level.py word 100`. The search for the ARM literal `0xB6518` failed because the ARM never holds a DSP word address: it holds **records** of the channel tables ([arm-param-messages](arm-param-messages.md)), and `DM(0xB6518)` is record 2 of table `0xC0549E28` (`{target 0 = FX core 1, base 0x282D9460, size 0x10}`, `arm-chan-tables.csv`, `0x282D9460 = 0x28000000 + 4 * 0xB6518`).

### 5.1 Writer of `DM(0xB6518)` (VERIFIED, run)

| Item | Result |
|---|---|
| Record users | the literal `0xC0549E48` (= record 2) is loaded at `0xC010BEA8` and `0xC0189A9C`; each hands `(target, base, value)` to the path-2 writer `0xC0062D28` (`0xC0062E10` for path-1 targets) |
| Mixer object | global `[0xC055D720]` = heap object `0x90C81490` in the settled emulator, built by the constructor at `0xC0249F00`: `[+0] = mult = 0.0`, `[+4] = taper memo = 1.0`; the same object is the `table` argument of the layer sender `0xC023195C` (s5.2) |
| Sender A `0xC01897A4` (event with `[event + 8] == 0`, value `v = [event + 0xC]`, 0..127) | `mult = (float(v << 16) * 0x34010204)^2 = (v / 127)^2`, stored at `[mixer + 0]`; sends `mult * [mixer + 4]` to `0x282D9460`. Run: `v = 127, 100, 64, 0` send `0x3F800000, 0x3F1EB866, 0x3E820610, 0` (FX core 1), equal to `master_level_word` bit for bit |
| Sender B `0xC010BE18` (knob index `i`, class vtable `0xC027A77C` slot 6) | `taper = TABLE[i]`, 128 floats at `0xC059FACC` (built by the init loop at `0xC0249F60`): `TABLE[0, 1, 16, 32, 64, 96, 112, 120, 126, 127] = 0, 0.0023, 0.0211, 0.0630, 0.2362, 0.5040, 0.7244, 0.8603, 0.9787, 1.0`; sends `taper * mult`, skipped when the taper equals the memo |
| Result | `DM(0xB6518) = (volume / 127)^2 * TABLE[knob]` = **1.0 at the defaults** (127; `Fnc_Volume` default 127). The image word 0 (muted) stays until the first volume event; the boot harness fires none, and the taper memo starts at 1.0, so the `Fnc_Volume` default sends nothing, which is why the earlier captures saw no message. That the hardware delivers the default volume event at start-up is ASSUMED (the keyboard would be silent otherwise); which controller feeds sender A (registry candidate `Fnc_MIDIVolume`, id `0xD`, default 127) was not traced |

So the ASSUMED 1.0 of D39 is now the firmware default: the verified path gain stays `0.3732` per float at the default master volume, scaled by `(volume / 127)^2 * taper` (`piano::downstream::master_level_word`).

### 5.2 The layer level words: chip, rows, scale (VERIFIED run for the structure, EMPIRICAL for the scale cells)

`0xC023195C(T, layer, slot)` has `T = [layer controller + 0x98]` = the **same mixer object** `0x90C81490` (four controllers with `[+0x64] = 0..3`, `[+0x8C] = 0, 1, 0, 1`). Ids are `[T + 8 + 4 (25 layer + r8)]` with `r8 = slot` (0, 1) or `slot + 13` (2..6), rows of table `0xC0547C58` (16-byte records `{target, base, 0xF3, tag}`, 48 of them, `arm-chan-tables.csv`); the sender writes the `base + 1` word through `0xC022E2C4` and the `base + 2` word through `0xC022E164` (a left / right pair after the pan law, each skipped when equal to its memo at `[T + 0x344 / 0x40C + ...]`).

| Row group | Target | DSP words (`+1`) | Reading |
|---|---|---|---|
| 40 .. 47 | PNO core 1 (target 2) | `0xB2ECF`, `0xB2EDE`, `0xB2EED`, `0xB2EFC`, `0xB2FF0`, `0xB2FFF`, `0xB300E`, `0xB301D` | the organ chain gain words of [arm-boot-rederive](arm-boot-rederive.md) s1 (the boot writes the sentinel there), **same addresses**: layer A slots 0, 1 are rows 40, 41, layer B rows 42, 43 |
| 0 .. 19 | FX core 1 (target 0): `DM(0xC2780 + 15 i)`, rows 15..19 at `0xC2892..` | `+1`, `+2` of each record | per-bus gains: the records point at the input buses (`+0 = 0xC0470, 0xC0478` SMP words, `0xC0458, 0xC0460, 0xC0468` PNO buses, in repeating sets of five; `0xC04B8` in the third set) |
| 20 .. 39 | FX core 2 (target 1): `0xC26E0 ...` | same | layer B |

FX consumer (listing `0x1A5B..0x1A86`, FX core 1; copies at `0x1D59`, `0x22D8`, `0x2527`): with `rec = 0xC27E9` (row 7), `i5 = rec[+0]`, `t = rec[+1]`, `s = rec[+3]`, `c = rec[+5]`; `g = max((1 - c) t + c s, 0)` (one-pole smoother, `c = 0.99` in the image, state written back) multiplies the four lane pairs behind the pointer. A negative word (the sentinel `-0.05` = `0xBD4CCCCD`, or `-0.08`) therefore mutes. The image holds `t = 1.0` in every record, which is why the unity measurement of s1 applies to the unscaled state; the ARM scales these records per layer.

Run of the sender with the "active" flag set (`[T + 7 layer + slot + 0x140] = 1`, memos cleared, level cells at their defaults: layer A 1.0, layer B 0.0; `tools/arm_master_level.py layer`):

| Layer A slot | Words sent | Value |
|---|---|---|
| 0, 1 | PNO core 1 `0xB2ECF, 0xB2ED0` / `0xB2EDE, 0xB2EDF` | **1.6** (`level 1.0 * scale cell [T + 0x150]`) |
| 4, 5, 6 | FX core 1 `0xC28D1/2`, `0xC28E0/1`, `0xC28EF/0` | 1.0 |
| 2, 3 | none (equal to the memo state) | |

The scale cells `[T + 0x150 + 4 k]`, k = 0..11, are `1.6, 1.3, 1.0, 1.0, 1.14, 2.5, 1.0, 1.0, 0.25, 3.25, 1.0, 1.0` (set by the constructor, selected by the byte table `0xC02B8B54[slot kind]`). **The organ chain gain word is 1.6 at layer level 127, not the image default 1.0** that every oracle state assumed ([arm-boot-rederive](arm-boot-rederive.md): "left at the image default"): +4.1 dB on the organ lanes that the oracle runs lack. The other scale cells (2.5 and 3.25 on slot kinds 5 and 9, 0.25 on kind 8, 1.3 and 1.14) belong to FX-side slots whose branch depends on the layer mode flag `[T + layer + 0x5B8]` (0 in the settled snapshot); which kind the piano SMP bus and the organ PNO buses take is **not resolved**.

### 5.3 What this does to the organ `K` and the piano level

| Quantity | Value |
|---|---|
| Gap under the common path (s2) | `K / 0.3732 = 7.5` (17.5 dB) between the organ clip calibration and the piano peaks |
| Found on the organ side | chain gain word 1.6 (4.1 dB); master level 1.0 at defaults; layer level `(v / 127)^2` = 1.0 at 127 |
| Left | `7.5 / 1.6 = 4.7` (13.4 dB), outside the verified chain; candidates are the FX bus scale cells (2.5 and 3.25 would give 8.0 and 10.2 dB; 0.25 on the piano side would give -12 dB), whose slot kind per layer mode is unresolved |

The organ `K = 2.8` therefore **cannot be replaced by a VERIFIED number yet**. Relative to an oracle state with the 1.6 word applied the engine constant would be `2.8 / 1.6 = 1.75` (not applied: the oracle lanes in the engine are rendered at word 1.0 and no hardware clip was re-run). No default changed: `firmware_bus_scale`, `firmware_layer_law`, `piano_firmware_level` and the organ `K` stay as in D22, D25 and D39.

## Decision

D39: the receiver and the common FX path are recorded; `firmware_bus_scale`, `firmware_layer_law` and `piano_firmware_level` defaults stay. D49: the master level writer and the layer word targets are recorded (s5); defaults stay because the organ chain word (1.6) leaves 13.4 dB unexplained and no hardware clip was re-run.

## Implications

- The transport from the SMP ring and from the PNO lanes to the codec is verified unity with one common gain (0.3732). The piano stroke peaks read as the DAC fraction (+2.7 dB against the legacy 0.55); the organ `K` is 17.5 dB above the same reading, so the missing gain is upstream of the FX unit in the organ lane path. A clip with a known velocity and the organ clip under the same recording chain (the DSoundman course has both) is the next test.
- OQ24 narrows: `K` is not applied in the FX unit; it is the lane scale of the PNO core or a state the oracle runs lack.

## Follow-ups

1. Done in s5: the writer of `DM(0xB6518)` and the chip of the `0xC023195C` ids. Open: which controller feeds sender A (`0xC01897A4`), and that the hardware fires the default volume event.
2. Resolve the layer mode flag `[T + layer + 0x5B8]` (which scale cell takes the SMP stream and the PNO buses), read the FX bus gains in an ARM state with the flag set, apply the organ chain word 1.6 in an oracle chord and re-measure `K` (same clip, same recording chain).
3. Re-run the hardware clips (White Grand, Velvet Grand, B3, Pipe) only after 2.
4. Why the descriptor ring alternates Core1 and Core2 destinations (every other frame to each core, or an address trick) and which frames the Core2 pass copies.

# Piano: SMP post stage and ring, the layer level handler run live

- **Checked:** 2026-11-14
- **Sources (local, never committed):** SMP image of OS v1.66 (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_raw.py --sram`; ADI `elfdump` listing of `0x1C0300..0x1C1582` through `tools/adi_dis.py`, CCES 2.12.1); PNO and FX images (`tools/ns4_dsp_ldr.py --sram`, listings through `tools/adi_dis.py`); `target/fw_unpacked/bank0_CODE.bin` (ARM, base `0xC0000000`, capstone); `crates/nord-sharc` (`sharc-ref`); `tools/arm_layer_level.py` (new); [piano-note-velocity](piano-note-velocity.md), [piano-downstream-level](piano-downstream-level.md), [smp-voice](smp-voice.md), [organ-output-law](organ-output-law.md), [pno-full-pass](pno-full-pass.md).
- **Question:** (1) What consumes the SMP post-stage ring and what is the end-to-end bus-to-DAC scale (OQ20, `firmware_bus_scale`)? (2) Does the piano layer level handler `0xC011A7A4` run as the listing says, and do piano layers use it? (3) SMP slew, stream edges, damper-down strokes.

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| Piano layer level handler `0xC011A7A4` (ids `0xB7`, `0xB8`) and the organ handler `0xC011AB8C` (`0xB1`, `0xB2`), run live | Called in the ARM harness with a fabricated controller (`tools/arm_layer_level.py`): the stored word equals `(v / 127)^2` for v = 0, 1, 2, 10, 32, 63, 64, 100, 126, 127 within 1e-6 relative (v = 100 gives 0.6200012, v = 127 gives 1.0), and v = 0 stores the sentinel `-0.05` (`0xBD4CCCCD`). The four ids behave identically; the guards (`[this + 0x64]` in {0, 1} / {2, 3}, `[event + 0xC]` in {7, 0xB} / {7, 0xC}) are the ones read from the listing | VERIFIED (run, `uv run --with unicorn python tools/arm_layer_level.py target/fw_unpacked/bank0_CODE.bin`, 40 of 40 cases ok; test `tools/tests/test_arm_layer_level.py`) |
| Do piano layers use it? | The handler stores at `[table + 4 (7 layer + slot + 0x34)]` and tail-calls `0xC023195C(table, layer, slot)`, the same per-slot gain routine the organ uses (it slews per-slot floats, `[r5 + 0x180]`, `[r5 + 0x248]`). That the stored word reaches the SMP voice gains (record fields `+11..+16`) is not traced | EMPIRICAL |
| Engine | New opt-in `PianoLayer::firmware_layer_law` (`ns4 render-patch --firmware-piano-layer-law`, `piano/downstream.rs::layer_gain`, applied in `engine/mix.rs`): fader gain `(level)^2`. Default off, so `render_hash` is unchanged. The Velvet Grand program stores 127 (level 1.0), so its video segment reads the same with or without the flag (0.0 dB) | VERIFIED law, ASSUMED application (D26) |
| SMP post stage (pc `0x1C0918..0x1C0A70`) | `mode1` SIMD on; `I0 = DM(0x9409A) = 0x9406E`, `I1 = 0x9405E` (`(LW)` pair), `I4 = DM(0x9409C) = 0xB2033`, `I2 = DM(0xB2028)`; per stereo frame `r0 = DM(I0 - 32) + DM(I1 - 32)` (two bus sets, `0x9404E` and `0x9403E`), `f0 = FLOAT r0 BY -29` (`r11 = -29`), two one-pole sections with `0.99` (`0x3F7D70A4`) and `0.01` (`0x3C23D70A`), a gain `DM(0x94098)` (1.0 in the image), a coefficient pair `0.027405769` / `0.99078` at `0x940..`; results go to the ring through `I2` (offsets 0, 2, 4, 0xA, 0xC ...) and `I4` | EMPIRICAL (listing; executed 129 instructions with the bus preset, ring words stayed 0: the state or flag preconditions are not met) |
| Where the ring goes | The ISR end (`0x1C0771`, `0x1C0786`) toggles `DM(0xB2028)` between `0xB2000` and `0xB2014` (20 words = 2 frames of a 10-word TDM frame). The init (`0x1C11D1..0x1C11DD`) sets `DM(0x0C43) = 0xB202C` (DMA chain pointer), `DM(0x0C00) = 0x020C21F0` (serial port control: enable, 32-bit words) and `DM(0x0C05)`, `DM(0x0C04)`. So the ring is the **transmit buffer of the SMP serial port**, a 32-bit TDM stream of float words; the receiver is a SPORT receive DMA of another DSP (FX or PNO input, ASSUMED) | EMPIRICAL (listing) |
| Receiver convention, master scale | **Not decoded.** The FX and PNO listings were scanned for float/fix conversions (`FIX BY`, `FLOAT BY`) but no SMP input stage or final DAC stage was identified in the time box (the hits are in the delay and packed-float code); no run reached a DAC word. A float word of the ring is `bus * 2^-29`: a unity-gain full-scale stroke (`2^30`) is 2.0 there against 0.5 in the `2^31` reading, so the engine unit is 4 times (12.04 dB) below the ring float, and the receiver scale in between is unknown | ASSUMED (unchanged) |
| Consistency check with the organ | The organ PNO lanes (oracle-faithful float, VERIFIED) need `K = 2.8` (+8.9 dB, EMPIRICAL) to reach the clips, and the piano engine unit (legacy 0.55) reads +0.0 dB against the Pipe yardstick at velocity 88. If both reached the FX chain with the same float convention, the piano ring float (4 x engine) would be about 21 dB over the clip-matched level. Indicative only (one clip each, velocity free): either the receiver of the SMP stream attenuates, or the organ lane carries a gain; the organ `K` and the piano side are not independent, and neither fixes the other | EMPIRICAL (arithmetic on the notes) |

## 2. The bus-to-ring arithmetic (listing)

```text
bus words (DM 0x9404E.., 12 stereo pairs)      y << 17 * gain >> 31         VERIFIED (smp_voice.rs)
r0 = busA + busB          ; f0 = float(r0) * 2^-29                          EMPIRICAL (listing)
one-pole 0.99 / 0.01 sections, gain DM(0x94098) = 1.0 (image)                EMPIRICAL (listing)
store floats at DM(I2) = 0xB2000 / 0xB2014, 2 frames per interrupt           EMPIRICAL (listing)
serial port transmit, 32-bit words (DM 0x0C00 = 0x020C21F0)                  EMPIRICAL (listing)
```

The ring words stayed zero in a `sharc-ref` run (`run 0x1C0915`, bus words 2^30 preset in 24 words, flags `DM(0xB2031)`, `DM(0xB2032)`, `DM(0x5201)` set): the ISR sets up registers (`I1`, `I5` and the filter state pointers) before `0x1C0915` that the call does not reproduce, so no gain number was measured. Next step: run the whole audio interrupt (`irq 17`) with a playing voice whose bus pointer is `0x9404E` and the flags the DMA ISR sets.

## 3. Layer level run (VERIFIED)

```bash
uv run --with unicorn --with capstone python tools/arm_layer_level.py target/fw_unpacked/bank0_CODE.bin
# handler 0xc011a7a4 id 0xb7: v=0 stored=-0.05 ... v=100 stored=0.620001197 expected 0.620001197 ok ... v=127 stored=1.0 ok
```

Fabricated: `[this + 0x64]` (slot selector), `[this + 0x8C]` (layer index), `[this + 0x98]` (table base), `[event + 0xC]` (event kind), `r3` -> q16 cell; the tail call `0xC023195C` is stubbed and its `(table, layer, slot)` arguments are used to read the stored cell. The handler code is soft-float arithmetic, so no boot state is needed.

## 4. SMP slew, stream edges, damper-down strokes

Not worked on in this pass (time box); the listing facts of [smp-voice](smp-voice.md) s7 and [piano-dsp-path](piano-dsp-path.md) stand. The ARM slew inputs (record fields `+11..+16`) are written by `0xC00B5F20`, `0xC00B6030`, `0xC00B61AC`; `0xC023195C` (above) is the per-slot slew routine that consumes the layer level word, which is where the time constant would be read.

## Decision

D29 (opt-in layer law; defaults unchanged). `firmware_bus_scale` stays opt-in.

## Implications

- The layer level law of the piano is a verified code path, so a piano program with a layer level below 127 follows `(v / 127)^2` on the hardware if the word reaches the voice gains; the engine offers it as an opt-in.
- The absolute piano level cannot be settled from the SMP side alone: the ring float is 12 dB above the `2^31` engine reading, and the receiver (FX or PNO input) decides. The organ `K = 2.8` and the piano yardstick together indicate that the receiver scale is not a plain `1.0 = full scale` for both.

## Follow-ups

1. Run the whole SMP audio interrupt with a playing voice and read the ring (needs the DMA-ISR flags): gives the exact bus-to-ring gain of the post stage (including the one-pole sections).
2. Find the SPORT receive buffer of the FX or PNO core that carries the SMP stream and the first float multiply on it (the input trim), then the output stage to the codec.
3. Trace `0xC023195C` to the SMP record gain writes to prove (or refute) that the layer level word scales the piano voice gains; then flip `firmware_layer_law` by default (D22).
4. SMP slew time constant, stream start/end history, damper-down strokes (section 4).

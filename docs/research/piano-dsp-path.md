# Piano DSP path: from the ARM key handler to the SMP voice record

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66) read with capstone; `unicorn` runs of `tools/arm_steady.py` / `tools/arm_edit_capture.py` from the settled steady state; `tools/arm_piano_path.py` (new); [smp-voice](smp-voice.md), [sy-smp-audio](sy-smp-audio.md), [arm-organ-messages](arm-organ-messages.md) section 10, [pno-module-chain](pno-module-chain.md); `crates/nord-core/src/piano.rs` and `piano/`.
- **Question:** What does the host firmware do between a piano key press and the SMP voice record that [smp-voice](smp-voice.md) plays bit-exactly, and where does the Rust piano differ?

Tags follow `.agents/rules/provenance-in-code.md`. **VERIFIED** = reproduced by a command below. **EMPIRICAL** = read from disassembly or observed, not independently checked. **ASSUMED** = reading of the purpose.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| SMP voice record geometry | 114 records of 54 words from DM `0x92848`; the loop pointer `I3` of [smp-voice](smp-voice.md) is the record base, so field `f` (`-32..+21`) of record `k` is DM `0x92848 + 54 k + f`. The voice list is 3 groups x 114 words at DM `0x92534`; entry = address of a record | EMPIRICAL (writers `0xC00B6CC0`, `0xC00AF3D4`, `0xC00B7B68`; literal map `data/piano-smp-record-fields.csv`) |
| Pitch law (ARM) | `step_frac = min(T[n] >> octaves, 0xFFFFFF) << 8`, `T[n] = 2^24 * 2^(-n/1200)` (1200-entry table at ARM `0xC02A6168`, plus `T[1200] = 0x800000`), `n = -cents` reduced by octaves. Cents must be `<= 0` (the table is indexed by `-cents`): the ARM keeps step `<= 1` per decoded sample | table VERIFIED (max error 0.4998 of a Q24 unit, `python tools/arm_piano_path.py pitch ...`); writer EMPIRICAL |
| Pitch offset (cents) | `0xC00A1C60`: `100 * (key - r3 - 4) - stroke[+0x5C]` plus a signed per-key byte (`[obj + key + 0x173]`, same role as `NpnoSound.tuning_cents`) plus engine offsets `[eng + 0x24] + [eng + 0x28]`; the `-4` and `r3` (root key?) are not decoded | EMPIRICAL |
| Key event entry | `0xC00BC8AC(this, ?, &{key, ?}, flags, ..., vel)`: flag bit 0 and bit 1 split key-up / key-down; key is replaced by `[0xC0575888]` when `[0xC057588C] != 0` (the fixed-key dev id); velocity argument is shifted right by 3 (`sp+0x78 >> 3`), so the keybed velocity is finer than 7 bit. It calls the voice start `0xC00A9274`, which sends the voice list (`0xC00B6CC0`) and the step (`0xC00B7B68`) | EMPIRICAL |
| Voice-level fields written by the ARM | `-32/-31` step (`0xC00AF3D4` zeroes, `0xC00B7B68` sets `-31`); `-2, -1, +2, +3` envelope fields as floats, with a read-back of `-2` over the SPI path (`0xC00B7758`, ramp factors `0.7` as a double and `0x5622`); `+11, +12, +14, +15, +16` float gain-slew fields (`0xC00B5F20` resets: `+14 = 1.0`, `+12 = +11 = 0`; `0xC00B6030` / `0xC00B61AC` load integers from the tables at `0xC02AE0EC + 0x68` and `+ 0x130` converted to float). The Q31 gains at `0 / +1` are written by the SMP ISR, not the ARM | EMPIRICAL |
| Editing ids and the piano layer | The piano layers (clients 2, 3; vtable `0xC027ACA0`) own the 15 `Edt_Pno_*`, 7 `Edt_PnoCom_*` and 5 `Edt_Effects_*` cells, two stores per layer (`0x9103FC50 / 0x91042670`, `0x91062120 / 0x91064B40` in the settled state). The store setter filter is not the limit: the client dispatch `0xC00CC4F0` reaches the handler `0xC011A87C` for the ids `>= 0x42`. The sweep result of [arm-organ-messages](arm-organ-messages.md) section 10 stands: only the reload trigger (`0x9403E`, `0x94046` = 1.0) reaches the SMP | VERIFIED (run) |
| Dev ids `Edt_Sys_kPianoFixedKey/FixedVel/FixedVelEnable/PlayMode_DEV` (`0x43/0x44/0x45/0x42`) | The piano layer subscribes them (ctor `0xC00194B8`). Values reach the handler **in Q16** (`value << 16`: the dispatch passes `(value + 0x8000) >> 16`). FixedKey `n` sets flag `[0xC057588C] = 1` and `[0xC0575888] = min(20 + n, 0x6C)`: `n = 40 -> 0x3C`, `89 -> 0x6C`, `1 -> 0x15`, `0` clears the flag. FixedVelEnable 1 with FixedVel 100 sets `[0xC0598F90] = 1`, `[0xC0598F94] = 100`. None sends a DSP message | VERIFIED (run, section 3) |
| Key down by id (`Kbd_Gate`, `Kbd_Hold`, `Kbd_Sustain`, `Kbd_Sostenuto`, `Kbd_Soft`, `KbdAbort`, `KbdKeyPosition`, `KbdNoiseTrig`, `Fnc_*`) on organ 0 and piano 2, 3 | silent: 39 ids, 36 messages, all from `Edt_Com_kAuxKbdEnable` / `Edt_OrgCom_kAuxKbdControl` on the organ (PNO core 1 words `0xB33B8..0xE20E0`, as before). The key and pedal events do not enter through the edit ids | VERIFIED (run, section 3) |
| UART RX path | The only attached receive ISR (irq 61, `0x80000434`) reads UART2 (`0x01D0D000`) into a 4096-byte ring (`[ring + 0] buf, +4 size, +8 head, +0xC tail`, driver table `0xC0583118`). Bytes pushed through a faked RBR/LSR (`90 3C 64`) land in the ring, but **nothing reads head or tail** in 6 M instructions of the settled state, and no DSP message follows | VERIFIED (run); consumer not found |
| Velocity law, sustain pedal, unison, string resonance, dynamics compressor, EQ, soft/hard timbre on the DSP | **not found** (section 5) | open |

## 2. Message path (VERIFIED run, stack scan `arm_piano_path.stack_return_candidates`)

`Edt_Pno_kBank = 2` on piano store `0x9103FC50`: one message pair `(0x9403E, 1.0)` sent by the generic store at `0xC0062E44`; call chain (stack scan, return sites) `0xC021C118` (trigger `0xC021C0A4(index, 0x800000)`, jump table of four) <- `0xC00B03E4` <- `0xC011F000` (the layer's id handler) <- `0xC00116A8` <- `0xC01E643C` <- `0xC0271B38` <- `0xC00146A8` <- `0xC00D663C` (dispatcher). All `Edt_Pno_*` handlers that speak use this one function; they select a sample set to reload, not a DSP parameter.

The generic DM writer is `0xC0062E10(comm = 4, address, value)`; reads go through `0xC0066E64` (used to read back the level meter bytes at DM `0x92772 + ...` in `0xC00B6B34` and the envelope word `-2` in `0xC00B7758`).

## 3. Reproduce

```bash
python tools/arm_piano_path.py pitch  target/fw_unpacked/bank0_CODE.bin   # table error, step_frac samples
python tools/arm_piano_path.py fields target/fw_unpacked/bank0_CODE.bin   # data/piano-smp-record-fields.csv
# id sweep from the settled state (about 1 minute; Kbd and Fnc ids on organ 0 and piano 2, 3)
python tools/arm_edit_capture.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
    --cache target/settled.snap --filter "Kbd|Fnc_|Edt_Pno_kKbdTouch" --layers 0,2,3 --summary out.csv
```

Output of `pitch` (this session): `entries 1202, max |error| 0.4998 (Q24 units)`, `table[1200] = 0x800000`, `step_frac(0) = 0xFFFFFF00`, `(-600) = 0xB504F300`, `(-1200) = 0x80000000`, `(-2400) = 0x40000000`. Dev ids: after `apply_value(store, 0x43, 40 << 16)` the cells `[0xC057588C], [0xC0575888]` read `1, 0x3C`; `89 << 16` -> `0x6C`; `1 << 16` -> `0x15`; `0` -> flag `0`; `0x45 = 1` then `0x44 = 100` -> `[0xC0598F90] = 1`, `[0xC0598F94] = 0x64`.

## 4. SMP-side record fields and the Rust model

| Field (from `I3`) | ARM writer | DSP role (smp-voice) | Rust |
|---|---|---|---|
| `-32` step_int, `-31` step_frac | `0xC00AF3D4` (zero), `0xC00B7B68` (`step_frac`) | counter / phase advance | `piano/smp_voice.rs` has the fields; `piano.rs` uses `f64` ratio * sample-rate ratio |
| `-2, -1, +2, +3` | `0xC00B7758`, `0xC00BC8AC`, reset `0xC00AF3D4` | not read by loop A (envelope / state, ASSUMED) | none |
| `+11, +12, +14, +15, +16` | `0xC00B5F20`, `0xC00B6030`, `0xC00B61AC` | float gain slew inputs (ASSUMED: SMP ISR turns them into `0 / +1`) | none |
| `-18, -11, -3, 0, +19, +20` | `0xC00B64D8` (record init) | bus pointer, predictor pointer, gains | `smp_page::page_voices` sets the equivalent |

## 5. Not done

| Task | State |
|---|---|
| Key-down capture with every DSP message | No emulated key source reaches `0xC00BC8AC`: the edit ids and the UART2 ring do not drive it. Next: call `0xC00BC8AC` directly with a built engine object (needs the object at `[layer + ...]` for the SMP client; not located), or find the keybed consumer of the UART2 ring |
| Velocity and gain law | The candidate tables (`0xC02AE0EC + 0x68`: 64 values falling from `10586947` to `8388941`, `+ 0x130`: 64 values from `2210496` to about `300`) look like smooth decays (ASSUMED: slew or crossfade curves), their index is not tied to velocity. No oracle replay done |
| Sustain pedal, unison, string resonance, dyn comp, EQ, timbre | Piano ids send nothing for them (`kUnison`, `kAcoustic`, `kDynComp`, `kPedalNoise`: silent; `kEq` only the reload trigger). Either the effects/system layer (clients 7 to 9, effects client not in the dispatcher list) or the key handler applies them |
| PNO core 1 routines for a piano program | no piano id addresses PNO core 1; the installer table of [pno-module-chain](pno-module-chain.md) is unchanged; no routine verified numerically here |

## 6. Rust parity to-do list (`piano.rs`, `piano/`)

| # | Item | Hardware evidence | Status in Rust | Action |
|---|---|---|---|---|
| 1 | Pitch step | `T[n]` table, Q32 `step_frac`, cents `<= 0` (section 1) | `2^(semitones/12)` in `f32`, step as `f64` | DONE: `crates/nord-core/src/piano/step_q32.rs` (`cents_table`, `step_frac_q32`, `step_ratio`, goldens `0xFFFFFF00`, `0xF1A1BF00`, `0xB504F300`, `0x80000000`, `0x40000000`, table fingerprint FNV-1a `0xD8A679867A8BA945`); keep `f64` for the render path unless bit parity is wanted |
| 2 | Voice count | 114 records, 3 list groups | `MAX_PIANO_VOICES = 64`, steal by envelope score (ASSUMED) | decide whether 114 is the polyphony limit (the list also holds release and noise voices: ASSUMED) |
| 3 | Velocity curves | not found; ARM velocity is `>> 3` of a finer keybed value | `Light/Medium/Heavy` power laws `0.65 / 1 / 1.55`, `0.22 + 0.78 a^0.9`, una corda `0.76`: all ASSUMED, untagged | tag `ASSUMED`; replace when the law is found |
| 4 | Sample-set selection | zone search over a key-sorted list with `ldrh [+4]` keys (`0xC0041108`); velocity layers by stroke | `select_stroke_with_damper` | compare with the real search once the list layout is decoded |
| 5 | Fixed velocity / key | dev ids `0x42..0x45` | none | optional test hook |
| 6 | Unison cents, dyn comp exponents, soft pedal gain | no message exists | `3.5 / 7 / 11.5` cents, `0.82 / 0.68 / 0.55`, `0.76`: ASSUMED | tag `ASSUMED`; leave until a DSP source is found |
| 7 | String resonance | handler stores a gain read by per-note voice-level code ([organ-tone-source](organ-tone-source.md)); no PNO core 1 message | modal resonators, 12 strings | tag `ASSUMED` |
| 8 | Sample path | bit-exact in `smp_voice.rs`, `smp_page.rs`, `q31_kernel.rs` | `interpolate_hw` exists but is not the default render path | switch the integer-PCM path to `interpolate_hw` for parity |

## 7. Follow-up: key event class, cents law, level law

| Item | Result | Tag |
|---|---|---|
| Event class | vtable `0xC02B0750` (built by the constructor `0xC00C0A04`, called once from `0xC0247B64`); slot 2 = key event `0xC00BC8AC`, slot 3 `0xC00BC7AC` (calls `0xC003E0C8`, a per-note list handler), `0xC00BC5D4`, `0xC00BC334`. Four instances in the settled state at `0x90C52F10 / F40 / F70 / FA0` = `[piano store + 0x94]`; `[obj + 0xC]` = core `0x90BF5400 / 0x90C0CAC0 / 0x90C24180 / 0x90C3B840` whose word 0 is the instance index 0..3 | VERIFIED (memory scan of the settled snapshot) |
| Direct call | `0xC00BC8AC(obj, 0, &{key, 0}, 2, vel << 3)` returns at once while `[core + 4] == 0`; with `[core + 4]` set to the piano layer `0x91022AD0` it runs 778 instructions through `0xC00AD5CC -> 0xC00AA520` (builds the voice frame: key `V+0x10`, extra cents `V+0x14`) `-> 0xC00A8D98 -> 0xC00A8370` and sends **no** DSP message: the stroke / sample map that the lookup needs is not loaded (the SPI/flash downloads are stubbed), and the voice start `0xC00A9274` is not reached | VERIFIED (run) |
| Voice frame call chain | `0xC00BC8AC` (key, `vel >> 3`) -> `0xC00AD5CC(core, key, vel, vel >> 3, ...)` -> `0xC00AA520` (frame) -> `0xC00A8D98` (per-voice procedure table, `0xC02A1CB4 / 0xC02A1D20` families: pitch and level procedures at `0xC02A1CC0 ...`) -> `0xC00A9274` (voice start, list `0xC00B6CC0`, step `0xC00B7B68`) | EMPIRICAL |
| Cents law `0xC00A1C60` | `cents = 100 * (key - root - 4) - S[0x5C]` (`S` = stroke object, unsigned fine-tune byte) `+ int8[base + key + 0x173]` (per-key tuning) `+ V[0x14]` (+/- a table term `0xC02A1208[key + (V[0x18] << 7)]` for modes 1 and 2 of `V[0x1C]`) `+ E[0x24] + E[0x28] + [0xC0539580]`; then the table of section 1. At `key = root + 4` the ratio is 1.0 (cents 0). The `4` is a literal semitone offset; whether it stands for the 35 kHz / 48 kHz ratio (-547 cents would be 5.47 semitones) is **not** decided: the engine terms `E[...]` are not decoded. `tools/arm_piano_path.pitch_cents` | EMPIRICAL |
| Level law `0xC00A1D5C` | amplitude (Q23) = `G[n] >> octaves`, `G[n] = round(2^23 * 2^(-n/60))` (61 words at `0xC02A1A08`, `G[0] = 0x7FFFFF`), `n = max(0, S.i16[0x5E] + 2 X + 60 - b1 - b2 - b3)`, reduced by 60 per octave; `b1..b3` are three signed per-key bytes (`[base + key + 0xF3]`, `[... + 0x3F3]`, `[... + 0x473]`), `X` is a call-frame word; one step is `6.02 / 60 = 0.1003 dB`. Outputs: amplitude to `[r7]`, `V[0x44]` to `[r6]`, `0x700000` to two further words. The table: 61 of 61 entries equal `round(2^23 * 2^(-n/60))` (`python tools/arm_piano_path.py level ...`: max error 0). `tools/arm_piano_path.level_q23` | table VERIFIED, procedure EMPIRICAL |
| Velocity -> `X` | **not found**: `X` is built from the caller frame (`V + 0x7C`), `vel >> 3` enters the frame as the fourth argument of `0xC00AD5CC`; the candidate tables `0xC02AE0EC + 0x68 / + 0x130` are used by the gain-slew initialisers (`0xC00B6030`: index `r1`, fallback table `0xC052B8C8` for `r1 > limit`), not by this level procedure, and nothing ties their index to velocity | open |

`interpolate_hw` decision (recommendation): the f32 path differs from the DSP by at most 0.5 LSB of the 1/8192 scale (SNR 66 to 79 dB at fractional steps, [smp-voice](smp-voice.md) section 1), i.e. a quantisation noise near -78 dB below full scale. For strokes decoded from `.npno` (integer PCM) use `interpolate_hw` as the default to match the hardware bit for bit; keep f32 for synthetic and SFZ sources, whose PCM is not integer-valued. No Rust written.

Consequence for the gain law: the ARM level law is a 60-steps-per-octave attenuation (0.1003 dB per step) with per-key and per-stroke offsets, so the hardware velocity response is a table of dB steps, not the power laws of `VelocityTouch`. Rust change to propose (not written, `crates/nord-core/src/piano/`): a `level_q23(index)` helper next to `step_q32.rs` with the table `round(2^23 * 2^(-n/60))`; keep the velocity-to-index map `ASSUMED` until `X` is traced.

## Implications

- The ARM chooses the pitch with a 1200-entry cents table and a Q32 step that never exceeds one sample per frame; `piano.rs` has no such limit and no table.
- The SMP record is 54 words and the ARM writes float envelope and gain-slew fields next to the integer step; the Q31 gains of [smp-voice](smp-voice.md) are produced on the DSP, so the velocity law is likely split between the ARM tables and the SMP ISR.
- The piano edit ids carry no per-note or timbre data to a DSP in the steady state; the effects the Rust piano models with ids (unison, dynamics compressor, EQ) have no observed hardware counterpart yet.

## Follow-ups

1. Find the consumer of the UART2 ring (or the keybed link) and feed a key-down; capture all DSP messages of all chips.
2. Decode `0xC00A9274` and `0xC00B64D8` to attribute the velocity and gain writes, then replay them on `sharc-ref` with the SMP image.
3. Decode the structure of `0xC00A1C60` (`r3`, `-4`) with a stroke from a real `.npno`.
4. Locate the effects/Leslie/EQ client that is not in the dispatcher list at the main loop.
5. Sound load via the stubbed library volume (`tools/arm_piano_load.py`) works; binder found (select method), voice start reached, level procedure not reached: [piano-sound-load](piano-sound-load.md).

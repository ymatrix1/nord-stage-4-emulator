# Research: ORG stage table, strip fields, module palette and what still blocks organ audio

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `core2_sram.bin` / `core1_sram.bin` of the ORG image (OS v1.66, `python tools/ns4_dsp_ldr.py dsp_image_2_ORG.bin --sram DIR`), `sharc-disasm` listings (byte offset = 2 x pc, e.g. pc `0x120300` = `0x240600`), `sharc-org` runs (strict, `skipped ops 0` unless stated) on `origin/main` fa20ddf (PR #59 fixed-point probe merged), a byte scan of `0x240000..0x2C0000` for immediate loads of `0x12xxxx` addresses. Builds on [org-dsp-map](org-dsp-map.md) and [dsp-rx-protocol](dsp-rx-protocol.md).
- **Question:** Which strip fields and stored values select which stage routine, what is the call graph of one pass, and can the real ORG code be driven to a non-zero organ output (tonewheels, drawbars, vibrato, rotary, EQ)?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **ASSUMED** = structural reading of a listing, not measured.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Non-zero organ audio from the real ORG code? | **No.** Not reached. 16 passes with strip 0 word 0 = `0x8012082F` (all slots end up on module `0x12082F`), nonzero floats in the input buffers `0xB0078..` / `0xB00C8..`, 16 passes: not one word of `0xB0000..0xB2FFF` changes. Recipe in section 6. | VERIFIED (negative) |
| Is the ORG image an organ synthesis program? | **Not in the form assumed so far.** Every selectable routine found is a channel-strip / parameter engine: packed 7-bit controller words (`0xC1DCC` area), per-slot pointer tables, morph between two strips, table look-ups (`0xB080D`, `0xB088E`, `0xB090F`, `0xB10D6`, `0xB1259`), 16-bit-float coefficient packing. No tonewheel bank, no 91-entry phase array, no key-contact or click code was found in the code reachable from the pass. | ASSUMED (absence in the analysed routines, not a proof for the whole image) |
| Which module ids can the code install? | Exactly these `0x12xxxx` immediates appear as loads in `0x240000..0x2C0000` (byte scan, last command of section 6): `0x12082F`, `0x1208D6`, `0x120BF7`, `0x120BFF`, `0x120C68`, `0x124C3A`, `0x12549D`, `0x125B79`. | VERIFIED |
| What are `0x124500` and `0x128300`? | **Not code entries.** The listing at `0x248A00` / `0x250600` is garbage (`L15 = PM(...)`, `DO ... LCNTR = R14` with a 4M count) and strict runs fault on the first instruction. They are data words that happen to lie in the code range. | VERIFIED |
| `R13 = R13 * R12 (SSI)` at `0x12032E`? | **Still unimplemented on `origin/main` fa20ddf**: `fault: unimplemented at 0x12032e (parcels [c7dc ...])` when slot 0 holds `0x120300`. PR #59 measured fixed-point ALU, multiplier and shifter but did not add Type 2c `c7dc`/`c7d0` (integer multiply to register, `(SSI)`). Report to the core/multiplier lane. | VERIFIED (fault) |

## 2. Structure of one pass (call graph)

```text
poll loop 0x125DF3/0x125DDF --mailbox--> 0x125DAD..0x12712B  head (pointers, 12 strips)
  0x127305..  6 x CALL 0x12538C         LFO k (org-dsp-map section 3)
  0x127A8A..0x127B01  8 x CALL (M8, I8)  I8 = DM(0xC10B8 + 0x18 s [+1])   stage A, 4 slots x (x,y)
  0x127B1C..0x127BA8  8 x CALL           I8 = DM(0xC0B8C ...)             stage B (null 0x125B79)
  0x127BBC..0x127C33  8 x CALL stage A   second round
  0x127C4E..0x127CDA  8 x CALL stage B   second round
  0x1271B8..0x12871x  per-strip installer / accounting (EMUCLK timing DM(0xC2C6E..0xC2C82))
```

Stage-A call arguments (listing `0x24F4EC..0x24F600`, VERIFIED): `I0 = 0xC1E02` (shared), `I1 = DM(0xC0B81 + 0x26 c)` (pointer ring), `I2 = 0xC1F96 + 0x36 c` (channel output record, 54 words), `I3 = 0xC0B88 + 0x26 c` (channel state, 38 words); c = 0..7 (4 slots, x and y of the SIMD pair). Stage-B arguments: `I8 = DM(0xC0B8C ...)`, `I2 = DM(0xC0B86 ...)`, `I3 = 0xC2942 + 0x34 c`, `I5 = 0xC28FA`, `I4 = 0xC1F96 + 0x36 c` (the stage-A output record is the stage-B input), `I0 = 0xC0B88 + ...`. So the pipeline per channel is **A (writes `0xC1F96` record) -> B (reads it, writes `0xC2942` record)**. Nothing in A or B stores into `0xB0000..0xB2FFF` (the `--watch 0xB0000:0x3000` store sites are only the pointer set-up `0x125E1E..0x125E59`, also with module `0x12082F` installed). The only listing that writes through `DM(0xB2EF4)` is module `0x1208D6` (section 4), and it stores 16-bit-float pairs (`FPACK` + `FDEP BY 16:16`) conditionally on the apply flag `DM(0xC136C)`: coefficient-like data, not samples.

## 3. Stage table (slot records at `DM(0xC10B8 + 0x18 s)`, VERIFIED by dump)

Four slots of 24 words; every field is a SIMD pair (word 2j = PEx, 2j+1 = PEy), so the table is 12 distinct fields:

| Field (words) | Default | Meaning | Status |
|---|---|---|---|
| f0 (0, 1) | `0x120BF7` | stage-A routine called by the pass (null stub zeroing two words) | VERIFIED |
| f1 (2, 3) | `0x120300` | per-slot **setup** routine (pointer-table builder, section 4); not called by the default pass | VERIFIED (value), call site open |
| f2, f4 (4, 5, 8, 9) | 0 | unused/state | ASSUMED |
| f3 (6, 7) | 2 (`10` after 16 passes with module installed) | per-slot counter, +1 per pass while a non-null stage is installed (`DM(0xC10BE + 0x18 s)`) | VERIFIED |
| f5 (10, 11) | `0xC1DDC`, `0xC1DDD` (`0xC1DDE`, `0xC1DDF` for slot 3) | packed 8-bit parameter words | VERIFIED |
| f6, f7 (12..15) | `0x8000`, `0x800000`, `0x80000000`, `0x80`; inverse masks `0xFFFF00FF` ... | selects one byte of the packed word per slot | VERIFIED |
| f8 (16, 17) | `0x208, 0x210, 0x218, 0x200` | offset | ASSUMED |
| f9 (18, 19) | `0xC10E0 + 0x18 s`, `0xC11C6 + 0x3E s` | pointers to the per-slot working data | VERIFIED |
| f10 (20, 21) | `0x3E800000` (0.25) | constant | VERIFIED |

Selection rule (the installer is the in-pass code at `0x1271B8..` = listing `0x24E370..0x24E5E0`, the same code is the tail of setup routine `0x1206BE`, listing `0x240E5E..`):

```text
R0 = strip word 0, FEXT BY 0:24                 (low 24 bits)
R1 = 0x12082F ; R3 = 0x120BF7 (null)
R2 = current stage in the slot
COMP(R0, R1): if equal   R2 = R0                (installs 0x12082F)
              if R2 == 0x120BF7 after that       R8 = 0   (apply flag cleared)
R8 = DM(0xC136C) ; IF R8 > 0  slot.f0 = R2
R1 = 0x120C68 ; R0 = strip word 1 low 24 bits ; COMP: if equal DM(4, I3) = R0   (B-stage entry)
```

Only `0x12082F` (strip word 0) and `0x120C68` (strip word 1) are compared in the installer; `0x12549D` is compared in the module-init code (listing `0x241948`, `R10 = DM(4, I0)` against `0x12549D`, then loads delay-line descriptors `0xB0ACD`, `0x6A000000`, `0xB02E8`, `0xB0BA2`, `0xC06DF`, `0x589`, `0x296`, `0x7C0`, `0x5F` into the module record at `0xC14AD..0xC14B9`). `0x124C3A` is loaded at listing `0x249866` (not analysed). **Type switch:** both `0x120300` and `0x12082F` read the packed word `R11 = DM(0xC1DCC + DM(-32, I3))` and, if its bits 5..7 (`FEXT BY 5:3`) equal 2, replace the stage by `0x1208D6`; bits 5..7 = 4 are treated like 2 by `0x120300`. VERIFIED from the listings, not yet exercised (the experiment below only reached `0x12082F`).

Experiment (VERIFIED, ORG Core2): a write of strip 0 word 0 = `0x8012082F` installs `0x12082F` in **one more slot per pass**: slot 1 word 1 after pass 3, and in all slots after 12 passes; pass cost grows 4,264 -> 4,434 -> 4,605 ... -> 6,985 instructions at pass 15 (+170 per installed stage).

## 4. Module palette

| Routine | Role (from listing) | Strict run in slot f0 | Status |
|---|---|---|---|
| `0x120BF7` | null stage A: zeroes `DM(-13, I2)`, `DM(-11, I2)` | 4,265 steps | VERIFIED |
| `0x125B79` | null stage B: copies one state word | 4,265 steps | VERIFIED |
| `0x12082F` | stage A "parameter ramp": reads the packed word (`FEXT 8:7` = 7-bit value, `FEXT 16:8`, `5:3` type, `28:3` slot), scales to float (`value << 24`, `0x7F000000` clamps to `0x7FFFFFFF`, `FLOAT BY -31`), computes `x = v * strip[-30] - k * strip[5] - strip[25] * state` into `DM(-32, I0)`, **then zeroes the whole output record** (`DM(-13..15, I2) = 0`) and returns. Produces no audio by construction. | 4,515 steps | VERIFIED (listing), role ASSUMED |
| `0x1208D6` | type-2/4 stage A: four negated products `-2*F12*F2` (coefficient-like), compares against three thresholds from the strip (`DM(6..8, I5)`), `FPACK`s four floats and stores two 16-bit-pair words through `DM(0xB2EF4)` (`DM(I7, M3)`, `DM(I7, M4)`), then smooths 8 state words with `0x7FE00000 / 0x200000` (one-pole, c = 0.9990), reads a table at `0xB1259`. | 4,281 steps | VERIFIED (listing) |
| `0x120C68`, `0x120BFF` | B-stage / A-stage candidates | **step limit (hang)** when put in the A slot (`f0`); a B-stage entry, not tested in B | VERIFIED (hang), role ASSUMED |
| `0x1206BE` | setup routine + the installer tail (previous section) | 4,613 steps; fills the lookup tables `0xB0805`, `0xB0813` | VERIFIED |
| `0x120300` | setup routine: builds the per-slot pointer table (below) and the filter coefficient set from `strip[s]` and `strip[s+6]` | **fault** (`0x12032E`, `c7dc`) | VERIFIED (fault) |
| `0x123900` | runs in slot f0 (4,431 steps); not analysed | 4,431 steps | VERIFIED (run) |
| `0x12549D` | module-init for the `0xB0ACD` delay-line module (listing `0x241878..0x241948`); an installed pointer in slot f0 runs 4,621 steps, 138 words changed | 4,621 steps | VERIFIED (run) |
| `0x124C3A` | fault at `0x124C9A` (`c701`, unimplemented) | **fault** | VERIFIED (fault) |
| `0x124500`, `0x128300` | data, not code | fault at first instruction | VERIFIED |

Pointer table built by `0x120300` for slot `s = FEXT(packed, 28:3)` (listing `0x24064C..0x240764`, VERIFIED): `DM(-28, I3) = 0xB135C + 8 s`; `DM(-6) = 0xC1E4E + 10 s`; `DM(-5) = 0xE0020 + 62 s` (strip s word 32); `DM(-7) = 0xC15B6 + 0x26 s`; `DM(-4) = 0xE0194 + 62 s` (strip s+6 word 32); `DM(-3) = 0xE0308 + 0x10 s` (a 16-word-stride array after the 12 strips); `DM(-2) = 0xC291E + 6 s`; `DM(-1) = 0xC13A6 + 0x28 s` (**the LFO records**: slot s owns LFO s); `DM(1) = 0xC1A1E + 0x32 s`; `DM(0) = 0xC169A + 0x96 s`; `DM(2) = 0xC1B4A + 0x30 s`; `DM(3) = 0xC1C4A + 0x18 r + 4 s`. So the LFO outputs `0xC1E34 + 10 k` are per-slot modulation sources.

Strip fields read by setup/ramp code (strip base = `0xE0000 + 62 k`, index = word): 2, 26, 28, 30, 32, 34, 37, 39, 40, 48..51 (the `-100.0, 1/12, 100.0, -1/6` constants of the default strip: a clipped linear map, ASSUMED to be a cents/semitone range), 53, 55, 57. The setup code blends strip `s` and strip `s + 6` with the 7-bit value as the weight (`F0 = F5 - F0; F0 * F1; CLIP`): strips 0..5 and 6..11 are **pairs** (current/target or A/B morph, ASSUMED).

## 5. Comparison with `crates/nord-core/src/organ.rs` (to-do list)

`organ.rs` models 91 tonewheels (`tonewheel_freqs/phases`), pickups, 9 drawbars, key click (`click_env`), scanner vibrato/chorus (delay buffer) and a Model 122 rotary (`rotary_horn_buf`, 800 Hz crossover). None of these has a firmware counterpart in the analysed ORG routines:

| organ.rs element | ORG firmware finding | To-do for the Rust engine |
|---|---|---|
| Tonewheel bank (91) | not located; no phase accumulator array in the pass | keep `ASSUMED`; look in the other sections (PNO/FX Core1) or conclude that the organ tone is produced elsewhere (sample/wavetable streamed from the ARM side) |
| 9 drawbars | no 9-word mix found; the `0xC1DDC` packed words carry 7-bit values (drawbar-range) per slot byte | find the ARM sender of the 9 drawbar values (open since dsp-rx-protocol 5) before mapping to slots |
| Key contacts / click | no key-state structure | none; click stays `EMPIRICAL`/`ASSUMED` |
| Vibrato scanner (6 LFOs) | six bit-exact triangle LFOs `0xC13A6` (org-dsp-map 3), owned per slot | replace the single vibrato phase by a triangle of `2|x + 0.25| - 1`, per-pass one-pole (c = 0.9908); check against `organ.rs` scanner rate |
| Rotary (800 Hz crossover) | no crossover or doppler delay found in A/B; delay-line module `0x12549D` (`0xB0ACD`, length descriptors `0x589`, `0x296`, `0x7C0`) is the only delay-like module | determine if `0x12549D` is the rotary; `0x589 = 1417`, `0x296 = 662`, `0x7C0 = 1984` samples |
| EQ | 16-bit-float coefficient pairs and the `-100/100` range map suggest the channel EQ/filter set-up | model after `0x1208D6` once its inputs are known |

## 6. Reproduce

```bash
python tools/ns4_dsp_ldr.py dsp_image_2_ORG.bin --sram SRAM
cargo build -p nord-sharc --release
O="SRAM/core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5"
# slot dump and stage-installer progression: one more slot is installed per pass
target/release/sharc-org $O --passes 16 --msg 0:0=0x8012082f --diff 0xb0000:0xb2fff --dump 0xC10B8:96
# nonzero inputs in the audio-port buffers (20 words at 0xB0078, 6 phases at 0xB00C8); output stays unchanged
P=""; for i in $(seq 0 19); do P="$P --poke 1:$((0xB0078+i))=f0.25"; done
for i in 0 1 2 3 4 5; do P="$P --poke 1:$((0xB00C8+i))=0x20000000"; done
target/release/sharc-org $O --passes 16 --msg 0:0=0x8012082f $P --diff 0xb0000:0xb2fff   # prints no changed word
# stage-by-stage direct install (strict, steps per pass in the table of section 4)
for st in 12082f 1206be 123900 1208d6 12549d 120300 124500; do
  target/release/sharc-org $O --passes 3 --poke 1:0xC10B8=0x$st --poke 1:0xC10B9=0x$st
done
# module ids the code can load (byte scan, a 6-line struct script)
python -I scan.py core2_sram.bin   # parcels LE: (p0 & 0xFFF0) == 0x0F00, p1 == 0x0012, p2 = low 16 bits
```

## Implications

- The strip words do not select among many organ modules: the installer compares strip word 0 with **one** module id (`0x12082F`) and word 1 with `0x120C68`; further modules come from the packed type field (`0x1208D6`) and from module-init code (`0x12549D`). A concrete organ patch therefore needs the packed controller words (`0xC1DCC..`), whose writer (ARM side) is still unidentified.
- Two assumptions of org-dsp-map are corrected: `0x124500` and `0x128300` are data, and `0x120300` is a setup routine (pointer-table builder), not an "8-bit unpacker" stage.
- Silence is not an oracle failure: the oracle executes every reached instruction in strict mode (`skipped ops 0`); the audio-producing code is simply not reached.

## Follow-ups

1. Implement Type 2c `c7dc` / `c7d0` (`Rn = Rn * Rm (SSI)`, integer multiply to register) in the core (fault `0x12032E`), then run `--poke 1:0xC10BA=0x120300` with `0xC1DCC` set and read `0xB135C..`, `0xC1E4E..` (needs the core/multiplier lane; no `core/` edits here).
2. Type 0x1208D6 stage: write packed words with bits 5..7 = 2 into `0xC1DCC..0xC1DDF` before the installer pass and dump `0xB0050..` (the `FPACK` output) to identify the coefficient set.
3. Identify the ARM side writer of `0xC1DCC` (packed words) and the nine drawbar senders; decode the `chan_table` bases.
4. Disassemble module `0x12549D` (`0xB0ACD` delay line, `0x6A000000`) and `0x124C3A` (fault `0x124C9A`, `c701`, another unimplemented opcode).
5. Run the same census on PNO/FX Core1 and ORG Core1: if a tonewheel bank exists it is outside the ORG Core2 pass.

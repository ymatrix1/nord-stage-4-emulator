# Research: ORG DSP processing pass, parameter map and the `sharc-org` harness

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `C:\...\scratchpad\sram_2_ORG\core{1,2}_sram.bin` from `python tools/ns4_dsp_ldr.py dsp_image_2_ORG.bin --sram DIR` (OS v1.66), [dsp-image-hunt](dsp-image-hunt.md) 5.21 to 5.36, [sharc-sequencer](sharc-sequencer.md), [arm-dsp-protocol](arm-dsp-protocol.md), `sharc-disasm` listings, `sharc-org` runs (strict mode, 0 skipped operations unless stated).
- **Question:** Where does the ORG DSP code write audio, which memory words hold the per-module parameters (tonewheel phase, drawbars, vibrato/chorus, key state, click), and can the oracle produce measurable audio from it, as a base for 1:1 comparison with `crates/nord-core/src/organ.rs`?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown (oracle run or listing), **ASSUMED** = structural reading, not measured.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Does the oracle run the ORG pass? | Yes, strict: 4,264 to 4,265 instructions per pass, back in the poll loop, no skipped operation, for ORG Core2 and (with its gates) ORG Core1. | VERIFIED |
| Where are audio samples written? | **Not in the default state.** Six passes change no word in `0xB0000..0xB2FFF` (the pass buffers and the `0xB135C` area, `--diff 0xb0000:0xb2fff`) or `0xC0000..0xC0B00`, and no store site targets a sample buffer; every audio-rate stage is a null stub (section 4). Audio needs a loaded module configuration that the image does not contain. | VERIFIED (negative) |
| What does the default pass compute? | Six triangle LFOs with one-pole smoothing (section 3). Their outputs are float and fixed-point words (value times 2^29, range +-4) at `0xC1E34 + 10 k`. | VERIFIED |
| Tonewheel phase, drawbars, key state, click | **Not located.** No word in the default pass has that meaning. Candidate carriers: the 8 slot records at `0xC10B8` and the packed parameter buffer `0xC1DDC` (section 5). | open |
| Is the Core1 section the same program? | Yes: identical call sequence and store sites, mailbox `DM(0xB00E1)` and poll loop `0x125DF3` instead of `0xB00E2` / `0x125DDF`. | VERIFIED |
| Oracle blocker | Type 2c word `c7dc` = `R13 = R13 * R12 (SSI)` at `0x12032E` faults (section 6). It stops the parameter-unpack function `0x120300` as soon as a slot selects it. | VERIFIED (fault), fix belongs to the multiplier lane |

## 2. Harness: `sharc-org`

`crates/nord-sharc/src/bin/sharc_org.rs` (+ `sharc_org/pass.rs`, `sharc_org/audio.rs`), `cargo run -p nord-sharc --bin sharc-org --release -- IMAGE [options]`. Boots the section, then runs N passes, each started by writing 1 to the mailbox and ended when the poll-loop pc is reached with the mailbox cleared.

```bash
# ORG Core2: 4 passes, phase 0x40000000 into LFO 0 before pass 1, list changed words in the module area
sharc-org core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 4 \
  --poke 1:0xb00c8=0x40000000 --diff 0xc0000:0xc2fff --dump 0xC13A6:6 --dump 0xC1E34:4
# store sites, executed pc ranges and calls of every pass
sharc-org core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 2 --watch 0x90000:0x70000 --coverage
# ORG Core1 (same program; gates from sharc-sequencer 3)
sharc-org core1_sram.bin --dev 0x3108d008=5 --dev 0x28ac0380=1 --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 \
  --flag 1=1 --boot 3000000 --trigger 0xB00E1 --idle-pc 0x125DF3 --passes 2 --coverage
# audio capture (none is produced by the default state): float buffer, 48 kHz WAV and CSV
sharc-org core2_sram.bin ... --buf @0xB2EF4:40:f32 --wav out.wav --csv out.csv
```

Options: `--poke PASS:ADDR=V` (V may be `f0.5` for a float), `--pre`, `--set`, `--dev`, `--flag`, `--lenient`, `--diff [LO:HI]` (SRAM and IOP words), `--watch LO:LEN` (store sites per pass), `--boot-stores`, `--coverage` (pc ranges and call targets), `--buf [@]ADDR:COUNT[:f32|q31[:STRIDE]]`, `--wav`, `--csv`, `--dump`, `--msg [PASS:]W1=W2` / `--msg-script FILE` / `--msg-base ADDR` (ARM-style messages, default base the channel strips `0xE0000`) and `--rx-irq VEC:ID` (start each pass through the P4I ISR, Core1), see [dsp-rx-protocol](dsp-rx-protocol.md). `@ADDR` captures through the pointer stored at ADDR (double-buffered pass buffers). Oracle change: `Core::watch_len`, `watch_addrs`, `watch_limit` extend the single-address data watch to a window (defaults keep `sharc-run --watch` unchanged).

## 3. The module records and the LFO (VERIFIED)

Pass code `0x127305..` (listing `sharc-disasm core2_sram.bin 0x24E5E0 0x24E6C0`) runs six blocks, one per record at `0xC13A6 + 0x28 k` (k = 0..5):

```text
I7 = record k;  I8 = DM(0, I7);  R10 = DM(k, I2);  R11 = DM(6+k, I2)      ; I2 = DM(0xB2EF8) = 0xB00C8
CALL (M8, I8)                         ; 0x12538C: F0 = 2*|FLOAT(R10 + 0x40000000 BY -31)| - 1
F0 = F0 + F4                          ; F4 = record[3]
F0 = F0 + record[4] * (record[5] - F0); record[5] = F0     ; one-pole smoothing, F2 = record[4]
DM(-26..-23, I6) = F0, F0, FIX F0 BY 29, FIX F0 BY 29      ; I6 = 0xC1E4E + 10 k (PEx, PEy duplicates)
```

| Word | Meaning | Status |
|---|---|---|
| `DM(0xB00C8 + k)` | phase of LFO k, unsigned 32-bit turn (`0x40000000` = quarter turn); the pass buffer base is `DM(0xB2EF8)` | VERIFIED |
| `DM(0xB00C8 + 6 + k)` | second per-LFO input (`R11`), not used by `0x12538C` | ASSUMED (consumed elsewhere) |
| record[0] | code pointer, `0x12538C` in the image | VERIFIED |
| record[3] | additive bias | VERIFIED |
| record[4] | smoothing coefficient c (`0x3F7DA3C2` = 0.9908 in the image) | VERIFIED |
| record[5] | smoothed output (state) | VERIFIED |
| `0xC1E34 + 10 k` | output quad: float, float, fixed, fixed (`FIX F0 BY 29`: value times 2^29, so 1.0 = `0x20000000`) | VERIFIED |
| record[1], [6..7] `0x1267`, `0xD1B71` | unknown | open |

Oracle runs (strict, `--poke 1:...`, 3 passes, `--dump 0xC13A6:6 --dump 0xC1E34:4`):

| Inputs | Result (record[5], fixed word) | Expected |
|---|---|---|
| phase `0x40000000`, c = 0, bias 0 | `0x3F800000`, `0x20000000` | +1.0 |
| phase `0xC0000000`, c = 0 | `0xBF800000`, `0xE0000000` | -1.0 |
| phase `0x20000000`, c = 0 | `0x3F000000`, `0x10000000` | +0.5 |
| phase `0`, c = 0 | `0`, `0` | 0 |
| phase `0x40000000`, c = 0, bias 0.25 | `0x3FA00000`, `0x28000000` | 1.25 |
| phase `0x40000000`, c = 0.5 (pass 1, 2) | `0x3F400000` after two passes | 0.5 then 0.75 |
| phase `0x40000000`, image c = 0.9908, three passes | about 0.0273 | `1 - 0.9908^3 = 0.0274` |

Max error 0 (bit-exact) in the exact cases. The smoothing runs once per pass, so c is a per-block coefficient: at a block rate B the corner is about `B * (1 - c) / (2 pi)`; the block rate is not known (the pass period is not tied to a clock in the oracle). Reading (ASSUMED): these six LFOs are the vibrato/chorus (scanner) modulators, the part of `organ.rs` that has no counterpart in the tonewheel bank; the `FIX BY 29` words feed fixed-point delay-line taps in later stages.

## 4. Pass structure and why no audio appears

Per pass (call trace by `--coverage`, identical in Core1 and Core2): 6 calls of `0x12538C`, then 8 calls of `0x120BF7`, 8 of `0x125B79`, 8 of `0x120BF7`, 8 of `0x125B79` (38 calls, executed ranges `120BF7-120BFE`, `12538C-12539D`, `125B79-125B7F`, `125DAD-12712B`, `1271B8-128696`).

| Routine | Listing | Reading | Status |
|---|---|---|---|
| `0x120BF7` | `R0 = 0; DM(-13, I2) = R0; DM(-11, I2) = R0; RTS` | null stage: zeroes two output words | VERIFIED (listing) |
| `0x125B79` | `R3 = DM(-28, I5); DM(16, I3) = R3; DM(18, I3) = R3; RTS` | null second stage: copies a state word | VERIFIED (listing) |
| `0x120300` | `FEXT` byte unpack of `DM(I1, M4)`, clamp at `0x7F000000`, `FLOAT BY -31`, writes `0xB135C + 8 s` / `0xC1E4E + 10 s` | unpacks 8-bit parameters from a packed word into floats | ASSUMED |

The stage calls take `I8` from `DM(0xC10B8 + 0x18 s)` (slot s, the image holds `0x120BF7 0x120BF7 0x120300 0x120300 0*6 0xC1DDC 0xC1DDD mask mask offset offset 0xC10E0+ 0xC11C6+ 0x3E800000 0x3E800000 0 0`, SIMD pairs duplicated) and from `DM(0xC0B8C..)`. All image defaults are the null stubs, so the default pass moves no audio: the first pass (356 changed words) only initialises filter and delay state (floats `0.5`, `1.0`, `-1.0`, pointer rings `0xC0BAE..`), later passes change about 95 words (counters `DM(0xC10BE + 0x18 s)` +1, LFO state, timing `DM(0xC2C6C..)`). Only `0x12538C`, `0x120BF7`, `0x125B79` appear in the data pointer tables of the image outside the slot area (scan of `0x90000..0x96400`, `0xB0000..`, `0xC0000..`, `0xE0000..` for words in `0x120000..0x12A000`: 0x120300, 0x120600, 0x1206BE (26 refs, data tables at `0x90929..`), 0x123900, 0x124500, 0x128300 and a few more), so the palette of selectable routines is larger than what the default pass calls. Which routine replaces a stub is chosen by data the ARM sends (ASSUMED, see section 5).

Pass buffers (VERIFIED by `--dump` after a pass): `DM(0xB00E4/E5) = 0xB0000 / 0xB0078`, `DM(0xB2EF3) = 0x282C01E0` (compared, selects the pair), `DM(0xB2EF4..F8) = 0xB0050 0xB0000 0xB0028 0xB0078 0xB00C8`, `DM(0xB2EF9/FA) = 3, 4`. Block size candidate: 40 words (`0x28`, pointer spacing in `0xB2EF4..F8`, record stride), ASSUMED.

## 5. Where parameters would enter (ASSUMED)

- The ARM sends `(slot id, 32-bit value)` pairs (arm-dsp-protocol 3); the DSP-side driver turns them into memory writes. The DSP-side words that look like targets: the packed buffer `0xC1DDC`/`0xC1DDE` (referenced by every slot record, four 8-bit fields per word, 0..127 clamped to `0x7FFFFFFF`, so 7-bit controller values such as drawbars and knobs), the record words 3/4 of section 3, and the per-slot floats `0.25` (`0x3E800000`) in the slot records.
- Not found in any ORG pass: tonewheel phase increments, 9 drawbar levels, key-on state, key click, chorus/vibrato depth. The default image has no per-voice structure (no 91 tonewheel array, no 61-key state), consistent with the tonewheel bank living in a module that is only selected when the ARM configures a patch, or in another section (PNO Core2, which runs the same program, was not run here).

## Implications

- The harness reproduces a full ORG pass deterministically and exposes poke/diff/store-site/call tracing, so each module routine can now be verified one at a time by selecting it in a slot record and comparing against a Rust model, as done for the LFO.
- `organ.rs` has no vibrato LFO whose shape and smoothing are tied to firmware; section 3 gives a bit-exact triangle (`2|x + 0.25 turn| - 1`), bias, one-pole smoothing per pass and the `FIX BY 29` fixed conversion.
- 1:1 organ audio needs a module configuration (function pointers and packed parameter words) that the image alone does not provide.

## Follow-ups

1. Implement `R13 = R13 * R12 (SSI)` (Type 2c `c7dc`, integer multiply to a register) in the multiplier lane; rerun with `--poke 1:0xC10B8=0x120300 --poke 1:0xC10B9=0x120300 --poke 1:0xC1DDC=0x01020304` and read `0xB135C..` and `0xC1E4E..` to confirm the byte unpack.
2. Disassemble the routines named by the `0x1206BE`, `0x123900`, `0x124500`, `0x128300` data references and select each in slot 0 to find the tonewheel, drawbar and click modules.
3. Trace the DSP-side receive path (how a slot id reaches `0xC1DDC`) from the first-pass init and the first `DM(0x3E0xx)` external-port DMA set-up (`boot store` sites `0x125C8F..0x125D4C`).
4. Run PNO Core2 and ORG Core1 passes with real parameter words to see whether Core1 owns the tonewheel bank.
5. Tie the pass to the audio clock (block size, rate) to scale the smoothing coefficient to a corner frequency.

## Reproduce

```bash
python tools/ns4_dsp_ldr.py dsp_image_2_ORG.bin --sram SRAM
cargo build -p nord-sharc --release
target/release/sharc-org SRAM/core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 3 --poke 1:0xb00c8=0x40000000 --poke 1:0xc13aa=f0 --dump 0xC13A6:6 --dump 0xC1E34:4
target/release/sharc-disasm SRAM/core2_sram.bin 0x24E5E0 0x24E6C0
```

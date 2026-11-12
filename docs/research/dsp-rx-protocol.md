# Research: how the ORG DSP image receives data from the ARM

- **Checked:** 2026-11-12
- **Sources (local, never committed):** ORG image `dsp_image_2_ORG.bin` (OS v1.66, `python tools/ns4_dsp_images.py ... --out DIR`, then `python tools/ns4_dsp_ldr.py ... --sram DIR`), `sharc-disasm` listings of `core1_sram.bin` / `core2_sram.bin` (`0x240000..0x2C0000`), `sharc-org` runs (strict mode, 0 skipped operations), `target/fw_unpacked/bank0_CODE.bin` with `capstone` 5, [arm-dsp-protocol](arm-dsp-protocol.md), [sharc-sequencer](sharc-sequencer.md), [org-dsp-map](org-dsp-map.md).
- **Question:** Which DSP-side code and hardware path receives the ARM's two-word messages, where do the words land, and what must a harness inject so the organ modules run?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **ASSUMED** = structural reading or peripheral naming from memory of the ADSP-SC57x/SC58x register map (the SC5xx HRM is not in `.reference/`).

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Which chip? | The `0x31xxxxxx` register addresses and the `0x28xxxxxx` core-to-core aliases are an ADSP-SC57x/SC58x-class SHARC+ with two SHARC cores ("Core1"/"Core2"), not a 2148x (`0x28240000`/`0x28A40000` = L1 base of core 1/core 2; `DM(0x282C0380)` in Core2 is a word of Core1's `DM(0xC0380)`). | ASSUMED (naming), alias arithmetic VERIFIED: `0x282C0000 - 0x28240000 = 0x80000 words*4 = DM(0xB0000)` |
| Is there a packet parser on the DSP? | **No.** No instruction in ORG Core1 or Core2 reads an SPI receive register or the L2 receive buffers. The only accesses to the SPI/DMA blocks are the set-up writes in the Core1 init (section 2). The words are written to data memory by DMA. | VERIFIED (negative): listing scan, section 2 |
| What does the P4I interrupt do? | It is the **pass clock**, not a message-arrived signal: the ISR stores the SEC source id as `DM(0xB00E1)`; the poll loop tests only "non-zero". | VERIFIED: `--rx-irq P4I:1` and `P4I:0x41` both run one pass (4,277 steps) |
| Where do ARM parameters land? | In 12 **channel strips** of 62 words at `DM(0xE0000 + 62 k)`, k = 0..11 (ARM channel index is bounded by 11, `cmp r3, #0xb`). | strip array VERIFIED (code uses `0xE0000 + 62 k`, ends at `0xE02E8`); that the ARM writes it ASSUMED |
| Does a changed strip change DSP behaviour? | Yes: `DM(0xE0000)` = `0x8012082F` (anything with bit 31 set that differs from the default `0x80120BF7`) makes the pass install stage `0x12082F` in slot 0, +510 instructions per pass, strict mode, no fault. | VERIFIED (section 3) |
| Does audio appear? | **No.** See section 6. | VERIFIED (negative) |

## 2. Hardware receive path decoded from the Core1 init (ORG Core1, short-word `0x128BEA..0x128D1D`, listing `0x251700..0x251A90`)

Core1 (and only Core1) configures a serial port and its DMA. Core2 never touches `0x3104xxxx`, `0x3100xxxx` or `0x30ffxxxx` (only `DM(0x31022104)`, `0x3108D00x`, `0x3108F040`).

| Register (raw address) | Value | Reading | Status |
|---|---|---|---|
| `0x31044004 / 08 / 0C / 10 / 14 / 18 / 1C / 20 / 24 / 28 / 30` | `0x6510 / 0x10 / 0x110 / 0 / 0x301 / 0 / 1 / 1 / 3 / 3 / 0` | SPI block at `0x31044000` (SPI2 in the SC58x map): CTL, RXCTL, TXCTL, CLK = 0 (slave: no clock divider), DLY, SLVSEL, RWC/RWCR, TWC/TWCR, IMSK | ASSUMED (names) |
| `0x31044058` written twice with `0xCAFEBABE` | | TX FIFO pre-fill (sync marker) | ASSUMED |
| `0x31046280 / 88 / 8C / 90 / 94 / 98` | `0x20004160 / 0x424222 / 1 / 0 / 0 / 0` | RX DMA channel: descriptor pointer, CFG (WNR set, enable bit set last), XCNT = 1 word | ASSUMED (names) |
| `0x31046200 / 08 / 0C / 10 / 14 / 18` | `0x20004188 / 0x1C220 / 3 / 4 / 0 / 0` | TX DMA channel: descriptor, CFG (read from memory), XCNT = 3, XMOD = 4 | ASSUMED (names) |
| `0x30FFF080 / 88 / 8C / 90`, `0x30FFE100` | `0x20004190 / 0x14222 / 1 / 0`, enable | third RX DMA channel + its peripheral (identity unknown) | ASSUMED |
| `0x3108A070 = 0x21`, `0x3108BB20/24/28 = 2`, `0x3108BA0C/14 = 2`, `0x3108A7F4` bit 0 | | SEC source routing and TRU triggers for these channels | ASSUMED |

L2 data block of the image (428 bytes at `0x20004000`, from the loader stream; the oracle does not load it, so `python tools/ns4_dsp_ldr.py ... --blocks` or a 15-line struct script reads it). All descriptors are `{next, ADDRSTART, CFG, ...}` in small-descriptor form:

| Address | Content (VERIFIED, image bytes) | Reading (ASSUMED) |
|---|---|---|
| `0x20004000..0x20004160` | ring lists with destinations `0x282C0140 0x28AC0140 0x28300140 0x28B00140` (CFG `0x44225`, 16/24 bytes), `0x282C01E0 0x283001E0 / 0x28AC01E0 0x28B001E0` (`0x44223`, 80 bytes), `0x282C0000 0x28300000 / 0x28AC0000 0x28B00000` (2D, `0x4A64225`, X 2 words, XMOD `0xA0`, Y 40, YMOD `-0x9C`), `0x282C0320 ...` (`0x44223`, 20 bytes) | audio/TDM block DMA into and out of the pass buffers `DM(0xB0050) DM(0xB0078) DM(0xB0000) DM(0xB00C8)` and their `0xC0000` ping-pong twins of both cores. Not the ARM link. |
| `0x20004160 / 6C / 78` | `{0x2000416C, 0x2000418C, 0x424223}`, `{0x20004178, 0x2000418C, 0x24223}`, `{0x20004160, 0x2000418C, 0x24223}` | RX ring of three 1-word transfers, all into `0x2000418C` |
| `0x20004184 / 88 / 8C` | `0xCAFEBABE`, `0x20004188`, `0x20004184` | TX descriptor at `0x20004188` = `{next = itself, ADDRSTART = *0x2000418C}`: the word received into `0x2000418C` becomes the TX start address (initially `0x20004184`, so the first reply words are `0xCAFEBABE, 0x20004188, 0x2000418C`): a **3-word read port**, the ARM sends an address and reads 3 words back |
| `0x20004190 .. 0x200041A8` | `{0x20004198, 0x200041A4}`, `{0x200041A0, 0x200041A8}`, `{0x20004190, 0x200041A8}`, `[0x200041A4] = 0x200041A8`, `[0x200041A8] = 0x12345678` | RX ring of three 1-word transfers: word 1 goes to `0x200041A4` (= ADDRSTART of the third descriptor), word 2 to the scratch `0x200041A8`, word 3 to the address named by word 1: a **write port** (`addr`, `x`, `data`) into any system address, including `0x28Bxxxxx` (Core2 L1) |

Reading: the ARM link is a DMA "peek/poke" port, not a message parser. The 3-word frame of the write port against the ARM's 2-word message (arm-dsp-protocol 3) is unresolved: either the CPLD on EMIFA CS3 adds a word, or word 2 is a mask or index. Open.

Not a parser, but note: an ARM word 1 that is `channel base + field` as a **word** index cannot be this byte address directly (`+0x39` is odd), so some stage between EMIFA and the SPI (the CPLD) converts it, or word 1 is an index into a table the poke port is not involved in. Open.

## 3. Strip array and the effect of a write (VERIFIED)

`DM(0xE0000 + 62 k)` is read by the pass for k = 0..5 (`R0 = DM(0xe0000)`, `DM(0xe003e)`, `DM(0xe007c)`, `DM(0xe00ba)`, `DM(0xe00f8)`, `DM(0xe0136)` at `0x24BDD6..0x24C114`, one block per strip in the Core1 listing) and bounded by `0xE02E8` (`I4 = 0xe02e8` at `0x24C3DC`, `I4 = 0xe02ee` at `0x24C1BA`): `0xE02E8 = 0xE0000 + 12 * 62`, which matches the ARM's channel check `cmp r3, #0xb` (12 channels, `0xC020C4D8`).

Default strip in the image (both cores, identical): word 0 `0x80120BF7`, word 1 `0x80125B79` (the two null stages of org-dsp-map section 4 with bit 31 set), word 7 `0x7F000000`, 1.0f at words 14, 15, 18, 19, -1.0f at 25, word 36 = 1, words 37, 38 = `0x80000000`, word 40 = -1, 1.0f at 41..43, -100.0 / 0.0833 / 100.0 / -0.1667 / 0.0400 at words 48..52, `0xC0000000` at 59, `0x7FFF` at 60. The pass itself stores only to words 2..17 and similar (SIMD pairs, e.g. `DM(0xE0008)..DM(0xE0011)` at short-word `0x1260FC`), never to words 0, 1: those are inputs.

Experiment (ORG Core2, `sharc-org` strict, 3 passes, message at pass 0):

```bash
sharc-org core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 3 --msg 0:0=0x8012082f --dump 0xC10B8:2
# w0=0x80120bf7 -> slot0 stage 120bf7, 4265 steps    (default)
# w0=0x0 / 0x1 / 0x12082f (bit 31 clear) -> 120bf7, 4265 steps
# w0=0x80000000, 0x80000001, 0x80120bf8, 0x8012082f -> 12082f 12082f, 4775 steps, 113 words changed
```

So strip word 0 behaves as an enable/changed flag word (bit 31) plus a low 24-bit field that the DSP compares with its default; the pointer value the ARM writes is not copied into the slot table (slot 0 gets `0x12082F` for any other value; `0x120300`, `0x1206BE`, `0x123900`, `0x124500`, `0x128300` all give the same). The slot-table writer is the pass code at short-word `0x1271C0..0x127210` (listing `0x24E3CE..0x24E40C`: `R1 = 0x12082f`, `R3 = 0x120bf7`, `FEXT ... BY 0:24`, `COMP`, conditional store to `DM(-16, I6)`), and a second candidate `0x120C68` is compared for word 1 (`0x24E402`). What selects the other stages (`0x120300`, `0x1206BE` ...) is open.

Also VERIFIED: 60 single-word pokes of `0x3F000000` into strip 0 words 2..61 (with word 0 = `0x8012082F`) change 2 to 28 words of `0xB0000..0xB2FFF / 0xC0000..0xC1FFF` per field (fields 12, 13, 14, 16, 17 the most: 14 to 28), so the strip fields are real module parameters.

## 4. P4I and the pass clock (VERIFIED)

ORG Core1 vector 15 (`P4I`, short-word `0x12003C`) jumps to the ISR at `0x128BBC`: `DM(0xB00E1) = R0` (R0 = status read from `DM(0x3108945C)` by the vector), `R0 = 1; DM(0x31022230) = R0` (clear of a DMA/SPORT status, ASSUMED), `R0 = DM(0xB00E1); DM(0x3108900C) = R0` (`SEC_END`: acknowledge of the source id, ASSUMED name), `RTI`. `--rx-irq P4I:ST` presets `DM(0x3108945C) = ST` and raises vector 15 before each pass; `--watch` shows the store sites `0x128BBC` (mailbox), `0x128BC1` (`0x31022230`), `0x128BC7` (`0x3108900C`) and the loop's clear at `0x125E36`.

The poll loop (`0x125DF3`: `R0 = DM(0xB00E1); PASS R0; IF EQ JUMP`) accepts any non-zero id. Which source id the SPI/DMA completion carries is not decoded (the SEC routing words are in section 2). Core2 sections are woken by a mailbox write from outside (`DM(0xB00E2)`), not by an interrupt (sharc-sequencer).

## 5. ARM side cross-check

`capstone` over `bank0_CODE.bin`: 904 `bl 0xC006CA48` sites (script counted them with raw `BL` decoding: 904). The senders reconstruct as (VERIFIED, listing `0xC00109F0..0xC0010AAC`, `0xC020C4CC..0xC020C6A4`):

| Sender | Path 1 (`[table+0x154] == 0`, EMIFA port) word 1 | Path 2 word 1 | Word 2 |
|---|---|---|---|
| `0xC00109E8` family (8 copies, pools at `0xC0010B2C ... 0xC00114CC`) | `chan_table[ch].base + 0x39` | `base + 0xE4` | argument `r1` (raw) |
| `0xC020C4CC` (`ch` from `[r0+4]`, `ch < 12`) | `base + 0x37` | `base + 0xDC` | constant 0 / `0x3F800000` |
| wrapper `0xC0062D28` called with `base + 0x104 / 0x108 / 0x10C / 0x140` | | | |

`chan_table` is `0xC05490B8`, 12 entries of 16 bytes (`{dsp index, base, ...}`), in BSS (the address is past the end of `bank0_CODE.bin` at `0xC053EA70`), so the **bases are filled at run time and are not recoverable statically**; I found no code that stores to it through its literal (only the senders and `0xC020C4E0 / 0xC020C624` read it). With base = 62 k (ASSUMED from the 12 x 62 strip array) path 1 word 1 = strip k word `0x39` (57), `0x37` (55); a census of the immediate added to the base before the store at all 904 sites (heuristic scan of the 12 instructions after each `bl`) gives mostly offsets `0x10..0x4B` (`0x39` 4 sites, `0x30` 8, `0x1B` 17, `0x13` 14, `0x4B` 3) plus the `1`, `2`, `3` of the pending counters, i.e. field offsets that fit inside a 62-word strip except `0x4B`. A concrete key-on or drawbar message could not be recovered: the ARM code has no symbols, and the organ senders were not identified (open).

## 6. What still blocks audio

| Blocker | Status |
|---|---|
| The stage palette is selected from data the harness does not know: strip word 0/1 only switch slot 0 to `0x12082F` (and `0x120C68` for word 1); stage pointers `0x120300`, `0x1206BE`, `0x123900`, `0x124500`, `0x128300` are not reachable by a pointer write. | open |
| Module `0x12082F` runs strictly (4,775 instructions per pass) but its outputs stay in module state (`0xC0DC9..`, `0xC0E0E..`: floats `0.5`, `-1.0`, `0`); no word of `0xB0000..0xB2FFF` (audio pass buffers) changes. The tonewheel bank, 9 drawbar levels and key state are still unlocated; the pass inputs `DM(0xB0078..)` (20 words) and `DM(0xB00C8..)` (5 words) are filled by DMA from the audio port, and the harness does not model that DMA. | open |
| The poke port frame (3 words) against the ARM message (2 words), and how `channel base + field` becomes a system address. | open |
| `R13 = R13 * R12 (SSI)` at `0x12032E` (Type 2c `c7dc`, parameter unpack `0x120300`) was not reached in these runs (slots 2, 3 hold `0x120300` but are not called by the default or by the strip writes tried); it stays the multiplier lane's. | not reached |
| ARM `chan_table` bases and the organ sender functions. | open |

## Implications

- Parameter delivery is DMA into data memory, so the oracle can model a message by its effect on `DM(0xE0000 + w1)`; `sharc-org --msg` does that (ASSUMED mapping) and `--rx-irq P4I:ID` runs the real ISR.
- The default ORG state is the "all null" configuration; a patch is a set of strip writes that flip stages from null stubs to modules. A 1:1 comparison against `organ.rs` needs the stage-selection rule (which strip fields choose `0x1206BE` etc.).

## Follow-ups

1. Find the stage-selection rule: disassemble the routines at `0x1206BE`, `0x123900`, `0x124500`, `0x128300` and the code at `0x24E3CE..0x24E5E0`, then script strip words until each appears in the slot table.
2. Model the audio DMA (RX lists `0x282C01E0`, `0x282C0320`) in the harness: fill `DM(0xB0078..)` and `DM(0xB00C8..)` per pass.
3. Locate the ARM code that fills `0xC05490B8` (bases) and the organ drawbar/key senders; decode a drawbar message end to end.
4. Resolve the 2-word versus 3-word frame and the EMIFA CS3 device (CPLD) electrically.

## Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out IMG
python tools/ns4_dsp_ldr.py IMG/dsp_image_2_ORG.bin --sram IMG/ORG
cargo build -p nord-sharc --release
target/release/sharc-org IMG/ORG/core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 3 --msg 0:0=0x8012082f --dump 0xC10B8:2
target/release/sharc-org IMG/ORG/core1_sram.bin --dev 0x3108d008=5 --dev 0x28ac0380=1 --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 --flag 1=1 --boot 3000000 --trigger 0xB00E1 --idle-pc 0x125DF3 --passes 1 --rx-irq P4I:0x41 --watch 0xB00E1:1
uv run --with capstone python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC00109F0 0xC0010AB0
```

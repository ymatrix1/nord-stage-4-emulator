# Research: which SHARC chips run the five DSP images

- **Checked:** 2026-11-12
- **Sources (local, never committed):** CCES 2.12.1 install (`SHARC/include/sys/ADSP_21571_device.h`, `ADSP_21571.h`, `ADSP-SC589_device.h`, `ADSP_SC573.h`, `SHARC/ldf/ADSP-SC573.ldf`, `System/ArchDef/ADSP-21571-memory.xml`, `ADSP-SC589-memory.xml`), ADSP-214xx Hardware Reference rev 1.1 (`.reference/sharc/`), mainboard photos in `.reference/mainboard/`, the five images of OS v1.66 (`python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out IMG`), [dsp-rx-protocol.md](dsp-rx-protocol.md), [sharc-sy1sy2-smp.md](sharc-sy1sy2-smp.md).
- **Question:** The ORG/PNO/FX images address `0x31xxxxxx` registers and `0x28xxxxxx` core aliases, which is not the ADSP-2148x map assumed so far. Which chips are they, and what changes for the SHARC oracle?

Tags follow `.agents/rules/provenance-in-code.md`. Not consulted: the Nord Stage 2/3 service manuals in `.reference/` (different products, no part numbers expected for the Stage 4 DSP board); I did not read them.

## 1. Conclusion

| Image | Chip | Confidence | Basis |
|---|---|---|---|
| FX, PNO, ORG (two cores each) | **ADSP-2157x** (SC57x family, dual SHARC+, no ARM used), most likely **ADSP-21571** | family **high**, exact part **medium** | Register census matches the CCES 21571/SC573 map exactly; TRU numbers and SPU/RCU placement rule out SC58x (section 2); the board photo shows a SHARC marked `...21571` (section 3) |
| SY1SY2, SMP | **ADSP-2148x**, most likely **ADSP-21489** | **high** for the family, medium for the part | The images start with the ADI SPI-slave boot kernel of the 2148x (HWR table 24-10), registers `0x1080..0x1084`; photos of the Nord Piano 4 main board show two `ADSP-21489` |

The earlier ASSUMED line "ADSP-21489 for everything" in [dsp-rx-protocol.md](dsp-rx-protocol.md) is wrong for FX, PNO and ORG and right for SY1SY2 and SMP. The ARM host is the TI `AM1705` seen on the board; the 2157x A5 core is not used (no ARM section in the loader streams).

## 2. Register evidence (VERIFIED)

Census: `sharc_census`-style sweep of the decoded code of all core sections of FX, PNO, ORG, SY1SY2 and SMP, collecting every immediate `0x2xxxxxxx..0x3xxxxxxx` (script in the PR description; FX Core1 uses direct `DM(0x31...)` addresses, ORG and PNO reach the same blocks through index registers). Every address maps into one CCES `ADSP_21571_device.h` block:

| Address(es) used by the images | 21571 block (header) | Note |
|---|---|---|
| `0x31044004..0x31044058` | SPI2 (`ADI_SPI2_BASE 0x31044000`), `CTL +4`, `RXCTL +8`, `TXCTL +C`, `TFIFO +0x58` | `0xCAFEBABE` written twice to `TFIFO` by FX/ORG |
| `0x31046200`, `0x31046280` | DMA26 / DMA27 (SPI2 TX / RX), `ADI_DMA26_BASE`, `ADI_DMA27_BASE` | descriptor-list rings at L2 `0x20004000` |
| `0x31022000..0x31022388` | DMA0..DMA7 (SPORT0..3 A/B), `0x31022230` = DMA4 `STAT` (offset `+0x30`) | ORG ISR clears `IRQDONE` |
| `0x30FFF080`, `0x30FFE100` | DMA36 and LP1 | link port 1 |
| `0x31089000`, `0x3108900C`, `0x31089440`, `0x3108945C`, `0x31089AF0` | SEC0: `GCTL`, `END`, `CCTL1`, `CSID1` (`CCTL1 + 0x1C`), `SB[94].SCTL` (`0x800 + 8 * 94`) | `REG_SEC0_CCTL1 = 0x31089440`, `CCTL2 = 0x31089480` |
| `0x3108A070 = 0x21`, `0x3108A7F4` | TRU0 `SSR[28] = 33`, `GCTL` | **`TRGM_SPI2_RXDMA = 33`, `TRGS_SPI2_TXDMA = 28`** in `ADSP_21571.h`: the SPI2 RX DMA triggers the SPI2 TX DMA (`CFG.TWAIT`, bit 15 of `0x1C220`) |
| `0x3108B980/84/88`, `0x3108BA0C/14`, `0x3108BB20/24/28` | SPU0 (`0x3108B000`) `SECUREC[0..2] +0x980`, `SECUREP[n] +0xA00 + 4n` | |
| `0x3108C008/0C/06C` | RCU0 (`0x3108C000`) `CRCTL`, `CRSTAT`, `MSG` | `MSG` is the inter-core mailbox |
| `0x3108D000..0x3108D00C`, `0x3108F040` | CGU0, CDU0 | |
| `0x310C3000/70`, `0x310C90C0..0x310C9314`, `0x31004000..` | FIR0 accelerator, DAI0, PORTA..D | FX uses the FIR accelerator and DAI pin routing |
| `0x20004000..`, `0x20080000..0x200FFFFF` | L2 SRAM, 1 MiB (`ADSP-SC573.ldf`, `21571-memory.xml`: `0x20000000..0x200FFFFF`) | |
| `0x28240000`, `0x28A40000` | `MP SHARC0/SHARC1 L1 8-bit (BW) Block 0` aliases (`ADSP-21571-memory.xml`) | Core1 = SHARC0, Core2 = SHARC1 |

Why 2157x and not SC58x (both have two SHARC+ cores and the same `0x31...` neighbourhood):

| Discriminator | Images use | ADSP-2157x / SC57x | ADSP-SC58x |
|---|---|---|---|
| TRU `SSR[28]` value for the SPI2 TX DMA | `0x21` = 33 | `TRGM_SPI2_RXDMA 33`, `TRGS_SPI2_TXDMA 28` | `45`, `37` (`ADSP_SC589.h`) |
| SPU0 base | writes `0x3108B980` (`SECUREC0`) | `0x3108B000` | `0x3108C000` |
| RCU0 base | reads `0x3108C06C` (`MSG`) | `0x3108C000` | `0x3108B000` |
| SPI0 / SPI1 | not used | `0x3102E000/F000` | `0x31042000/3000` |

No instruction form beyond what the strict oracle runs already decode was needed for FX/PNO/ORG: the 2157x SHARC+ core executes the same VISA/ISA encodings as the 2148x (all 24 core sections run strict, `sharc-sequencer.md`, `dsp-image-hunt.md` 5.21).

## 3. Board photos

`.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg` is a **Nord Piano 4 main board v0.72**: `TI AM1705DPTP4` (ARM host) and two `ADSP-21489 KSWZ-4B`. `658431218_...webp` (Stage 4 main board, "Moderkort NS4 0.95") shows the ARM, three SHARC-style packages with hand-wired DC/DC modules and one chip whose marking reads `...ALOG DEVICES 2157 1 ... 4B ... 0.0` after rotating and contrast-stretching the crop (the last digit is the weakest part of the reading: **medium confidence for 21571**, as opposed to 21573). The same board has a centre chip with the SHARC logo whose part number is not legible. I could not match packages to images; the five images need five SHARC cores' worth of chips only if FX, PNO and ORG are three dual-core 2157x plus two 2148x, which the photos neither confirm nor exclude.

## 4. SY1SY2 and SMP are 2148x (VERIFIED)

The first `0x600` bytes are the ISA-mode boot kernel the ADSP-214xx SPI-slave boot expects (HWR table 24-10: reset `SPIDMAC = 7`, `IISPI = 0x92000`, `IMSPI = 1`, `CSPI = 0x180`, 384 words). The kernel writes `DM(0x1080)`, `DM(0x1081)`, `DM(0x1082)` and polls `DM(0x1084)` bit 15 (`SPIDMAS`, HWR table A-116). It programs the PLL through `DM(0x2000)` (`PMCTL`). The 2157x core MMR window at `0x0..0x3FFFF` holds different registers, not these. Block 0 is the 48-bit space from `0x8C000` = normal-word `0x92000`.

## 5. DMA descriptor rings of the ORG image (VERIFIED against the model)

Read from the loader stream (`python tools/ns4_dsp_ldr.py ORG.bin --sram DIR` now also writes `core1_l2.bin`) and decoded with the CCES `CFG` bit positions (EN 0, WNR 1, PSIZE 4, MSIZE 8, FLOW 12, TWAIT 15, NDSIZE 16, INT 20, TRIG 22, TWOD 26):

| Ring | CFG | Decoding | Status |
|---|---|---|---|
| SPI2 RX `0x20004160` (3 descriptors, one word each, all to `0x2000418C`) | `0x424223` | WNR, 32-bit, FLOW 4 (list), NDSIZE 2, TRIG (end of unit), no interrupt | field layout VERIFIED (header), list layout `{next, ADDRSTART, CFG}` ASSUMED but consistent with every descriptor of the ring data |
| SPI2 TX `0x20004188` `{next = itself, ADDRSTART = [0x2000418C]}`, XCNT 3, XMOD 4 | `0x1C220` | read memory, FLOW 4, NDSIZE 1, **TWAIT** | same |
| Audio lists `0x20004000..0x2000415F` | `0x44221`, `0x44223`, `0x64221`, 2D `0x4A64225` (X 2, XMOD `0xA0`, Y 40, YMOD `-0x9C`) | NDSIZE 4 / 6 | 2D end-of-row rule ASSUMED |

Result: the SPI2 link is a **peek port**. The ARM sends one word; the RX DMA writes it into the `ADDRSTART` field of the TX descriptor and triggers the TX DMA (TRU master 33 to slave 28), which sends `XCNT = 3` words from that address (first reply, the descriptor default: `0xCAFEBABE`, `0x20004188`, then the received address itself). The model in `core/periph/` reproduces that (`peek_port_replies_with_three_words`) and an unmodified ORG Core1 init drives it: after the init `sharc-run ... --sc57x 1` shows both channels armed (`dma 0x31046200 Spi2Tx ... desc 0x20004188`, `dma 0x31046280 Spi2Rx ... desc 0x20004160`). **ASSUMED:** the TX descriptor is fetched when the unit is loaded, before the request arrives, so the reply to request N is read from the address of request N-1 (the first reply is the default block). A measurement decides this (probe P5).

## 6. What changes for the oracle

| Area | 2148x assumption | Reality for FX, PNO, ORG (2157x) | Done in this PR |
|---|---|---|---|
| Peripheral map | IOP registers `0x0000..0x3FFFF` (`SPICTL 0x1000`, `IISPI 0x1080`) | `0x3xxxxxxx` system MMRs, SEC0 core interface, DMA descriptors in L2 | `core/periph/`: SEC0, SPI2, DMA, TRU, LP1, L2; `--sc57x` |
| Memory | L1 blocks, external SDRAM | L1 as before (NW `0x90000..`, 21571: `0x90000..0x97FFF`, `0xB0000..0xB7FFF`, `0xC0000..0xC3FFF`, `0xE0000..0xE3FFF`), **1 MiB L2 at `0x20000000`**, DDR for external | L2 backing store; loader exporter `core{1,2}_l2.bin` |
| Core aliases | none | `0x28240000` (Core1 L1) and `0x28A40000` (Core2 L1) byte aliases; Core MMR window `0x00000000..0x3FFFF` | not modelled (device store); cross-core pokes of the poke port need it (follow-up 2) |
| Interrupts | peripheral interrupts straight to `P0I..P18I` | system sources through SEC0: the core reads `SEC_CSID`, writes `SEC_END`; the ORG ISR is on `P4I` | `Sec` with `raise_source`, `CSID`/`END`; vector `P4I` ASSUMED |
| ISA/VISA | VISA program, ISA kernel | same encodings (strict runs) | none needed |
| IVT | `IIVT` at `0x8C000` ISA words | VISA vector table at short word `0x120000` | ISA-mode dispatch added for SY1SY2/SMP |
| Simulator target | ADSP-21489 | `ADSP-21571` / `ADSP-21573` / `ADSP-SC573` | section 7 |

## 7. CCES 2.12.1 support for the 2157x and SC58x (answer to the coordinator's question)

Present: `System/ArchDef/ADSP-21571|21573|SC570..SC573|SC582..SC589|SC591..SC594-{compiler,linker,memory,registers}.xml` (89 files for SC5xx/2157x), headers `SHARC/include/sys/ADSP_21571*.h`, `ADSP_SC589*.h`, ldf `SHARC/ldf/ADSP-21571.ldf` and the SC57x/SC58x ones. `easm21k -proc ADSP-21571`, `-proc ADSP-SC573`, `-proc ADSP-SC589` and `-proc ADSP-21489` all assemble a probe (`.section/pm seg_pmco; _start: r0 = 1; nop;`). The simulator DLLs (`sim10x.dll`, `cyclesim10x2.dll`, `procSHARC.dll`) are installed; **whether a simulator session exists for the 21571 cannot be read from files**: it is a GUI check (New Debug Configuration, Simulator, processor list). SC589 and SC573 are two-core targets whose SHARC cores use the same map as the 21571, so either can stand in if the 21571 is absent.

### Probes for a simulator session (measure, do not assume)

Write each as a Core1 assembly or C program run on the 21571 (or SC573) simulator with the register windows open; record the register values after each step. Until a probe is run, the model behaviour it covers stays **ASSUMED**.

| # | Probe | Model behaviour it settles |
|---|---|---|
| P1 | Write `0x12345678` to `0x20004000`, read back; read `0x200FFFFC`; read `0x20100000`; byte and halfword views through `0x20004001` | L2 size, byte addressing, what is above `0x200FFFFF` (`l2.rs`) |
| P2 | `SEC0_GCTL = 1`; `SEC0_CCTL1 = 1`; `SEC0_SCTL[69] = ` enable bits, then `SEC0_RAISE = 69` and `70`; read `SEC0_CSTAT1`, `SEC0_CPND1`, `SEC0_CSID1` twice; read `IRPTL` | What reads of `CSID` return (lowest pending id? 0 when none?), whether the read activates, which core interrupt `IRPTL` bit is set (`P4I`?), whether disabled sources still pend (`sec.rs`, `SEC_VECTOR`) |
| P3 | Write `SEC0_END = 69`, read `CSID1` again, then `SEC0_CSID1` with nothing pending | End/acknowledge semantics, empty value |
| P4 | Memory-to-memory descriptor list on MDMA0 (DMA8/DMA9, `0x310A7000/0x310A7080` on the 21571): two descriptors of `{next, ADDRSTART, CFG}` with `NDSIZE = 2`, `XCNT` preloaded, `INT` and `TRIG` variants; read `DMAx_STAT`, `DSCPTR_CUR`, `ADDR_CUR`, `XCNT_CUR` between units | Descriptor layout and `1 + NDSIZE` word count, `STAT.IRQDONE` timing and W1C, what `CFG.EN` does at the end of a `FLOW = 0` unit (`dma.rs`) |
| P5 | Same with a 2D unit `X = 2`, `XMOD = 0xA0`, `Y = 3`, `YMOD = -0x9C` and a destination pre-filled with a pattern | End-of-row address rule (`YMOD` from the last element) |
| P6 | Peek-port analogue: RX unit writing a word into the `ADDRSTART` slot of a TX descriptor, trigger through `TRU0_SSR[n]`, TX unit with `TWAIT`; run two requests | Descriptor fetch before or after the trigger (reply from request N or N-1), `TWAIT` release, TRU `SSR` indexing by slave id (`sc57x.rs`) |
| P7 | `SPI2_CTL = 0x6510`, then `0x6511`; `SPI2_RXCTL`, `SPI2_TXCTL`, `SPI2_STAT` after each; write `SPI2_TFIFO` twice | Which `STAT` bits an idle slave shows, whether `TFIFO` writes need `TXCTL.TEN` (`spi2.rs`; the simulator may not model an external master, in which case P7 only checks the register bits) |
| P8 | Read `RCU0_MSG` (`0x3108C06C`), `RCU0_CRSTAT` (`0x3108C00C`), `CGU0_STAT` (`0x3108D004`) at reset | Reset values the harness scripts with `--dev` (not modelled) |

## Implications

- FX, PNO and ORG are SHARC+ on an ADSP-2157x: the register map for the oracle comes from the CCES headers, which are on this machine and authoritative (they are not committed: licence; cite the file and the symbol).
- The ORG Core1 init now completes under `--sc57x` without scripted `--dev` values for SPI2, DMA, SEC and L2; the P4I pass clock is `--sec-raise 90` (SPORT2 A DMA source) instead of `--rx-irq P4I:ID`.
- SY1SY2 and SMP can be loaded exactly as the ARM sends them: `sharc-run KERNEL 0x8c005 N --isa --spi-boot IMAGE:0x600 --flag 12=1` runs the kernel over the whole block stream, rebuilds the vector table over the kernel code and starts the program, which settles in its idle loop (SY1SY2 `0x1C0B33`, SMP `0x1C11FD`), strict (no skipped operation), with DM contents byte-identical to the flat image of `ns4_dsp_raw.py` at the three addresses compared.

## Follow-ups

1. Run probes P1 to P8 on the simulator and promote or correct the ASSUMED behaviours.
2. Model the `0x28xxxxxx` core aliases so the poke port can reach the other core's L1 (`dsp-rx-protocol.md` section 2).
3. Read the centre chip marking on the Stage 4 board in a sharper photo to pin the exact 2157x part (21571 vs 21573) and the number of chips per image.
4. Wire `--sc57x` into `sharc-org` (pass loop, `--msg` through SPI2) once the peek-port timing is measured.
5. Model SPORT/DAI audio DMA (rings `0x31022000..0x31022380`).

## Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out IMG
python tools/ns4_dsp_ldr.py IMG/dsp_image_2_ORG.bin --sram IMG/ORG     # core{1,2}_sram.bin and core{1,2}_l2.bin
cargo build -p nord-sharc --release
# ORG Core1 init with the peripheral models (entry value from ns4_dsp_ldr output), SEC pass clock from SPORT2 A DMA (id 90)
target/release/sharc-run IMG/ORG/core1_sram.bin 0x125c88 3000000 --sc57x 1 --l2 0x20000000:IMG/ORG/core1_l2.bin \
  --dev 0x3108d008=5 --dev 0x28ac0380=1 --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 --flag 1=1 --sec-raise 90 --watch 0xB00E1
# SY1SY2 / SMP boot over the SPI-slave stream
python tools/ns4_dsp_raw.py IMG/dsp_image_3_SY1SY2.bin --kernel kernel.bin
target/release/sharc-run kernel.bin 0x8c005 3000000 --isa --spi-boot IMG/dsp_image_3_SY1SY2.bin:0x600 --flag 12=1 --trace 4
# register map lookup in the CCES headers
grep -nE 'ADI_(SPI2|SEC0|TRU0|SPU0|RCU0)_BASE|TRGM_SPI2_RXDMA|TRGS_SPI2_TXDMA' "$CCES/SHARC/include/sys/ADSP_21571_device.h" "$CCES/SHARC/include/sys/ADSP_21571.h"
```

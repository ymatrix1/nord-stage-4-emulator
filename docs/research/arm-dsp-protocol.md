# ARM host to DSP interface (AM1705 to ADSP-21489)

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66, ARM mode, load base `0xC0000000`), `tools/arm_xref.py` (new), `docs/research/dsp-image-hunt.md`, `docs/research/synth-0x2f-ns4decode-crosscheck.md`. Disassembly with `capstone` 5.x (`uv run --with capstone`). Peripheral base addresses are the OMAP-L137 / AM17x memory map from memory (not re-read from the TRM in this pass); SPI0, SPI1, GPIO and SYSCFG0 are corroborated by the firmware's own driver code (below).
- **Question:** How does the ARM host load the DSP images and send runtime commands, parameters and MIDI-derived data to the DSPs, which DSP-side addresses does it use, and how do ARM-side values map to DSP-side addresses?

Tags: **CONFIRMED** = read directly in disassembly or reproduced by `tools/arm_xref.py` on the real image; **HYPOTHESIS** = inferred from structure, not run or not fully traced.

## 1. Short answer

| Part | Result | Status |
|---|---|---|
| Image download | The ARM sends the five DSP images (FX, PNO, ORG, SY1SY2, SMP) at start-up through **SPI** (a driver object with one chip-select GPIO per DSP). The download routine is `0xC0241C58`. | CONFIRMED (code), SPI port choice open |
| Runtime parameters | Messages are **two 32-bit words** (a slot or command word, then the value) written to a memory-mapped **EMIFA CS3 port** (`0x6200000C`, `0x62000010` seen), with a **GPIO handshake** and a 256-byte software FIFO per DSP. 904 call sites of the flow-control routine `0xC006CA48`. | CONFIRMED for the call shape, port addresses HYPOTHESIS |
| DSP-side address mapping | None found: the ARM sends slot numbers and values, not DSP RAM addresses. The mailbox `DM(0xB00E2)` is not referenced from the ARM. | CONFIRMED (negative) |
| MIDI | Not traced here (see Follow-ups). | open |

## 2. Download path (CONFIRMED)

`bank0_CODE@0x241C58` (`0xC0241C58`), called once from `0xC024BB28` (inside the DSP bring-up function that logs `DSP Boot Error`, starts at `0xC024B484`). It calls virtual method 0 of the object in `[r7+0x90]` five times with `(obj, index, pointer, length)`:

```bash
python tools/arm_xref.py downloads target/fw_unpacked/bank0_CODE.bin 0xC0241C58 0xC0241D64
```

| Call | Index | Pointer | Length | End | DSP name (ASSUMED, `dsp-image-hunt.md` 5.11) |
|---|---|---|---|---|---|
| `0xC0241C74` | 4 | `0xC051B574` | `0xAE34` | `0xC05263A8` | SMP |
| `0xC0241C98` | 3 | `0xC04FD980` | `0x1DBF4` | `0xC051B574` | SY1SY2 |
| `0xC0241CE4` | 2 | `0xC04B52B0` | `0x486D0` | `0xC04FD980` | ORG |
| `0xC0241D08` | 1 | `0xC046EA8C` | `0x46824` | `0xC04B52B0` | PNO |
| `0xC0241D40` | 0 (r1 comes from an earlier compare result, so the tool prints `?`) | `0xC044667C` | `0x28410` | `0xC046EA8C` | FX |

This is the same pointer and length table the bank 1 self-test uses (`dsp-image-hunt.md` 5.11), now seen in the bank 0 production path: the OS image carries the DSP code and the ARM sends it to the DSPs at start-up (a DSP-side flash copy is not excluded, but the ARM path exists). Each failing call returns a status that is looked up in a table at `0xC0444CA0 + 0x234` (error codes 1 to 6).

**Transport = SPI (CONFIRMED by the method body).** The object's class has vtable `0xC029EF1C`; method 0 is `0xC0064E7C`. It:

- writes bytes to `SPIDAT1` (`[spi+0x3C]`, OR-ed with a chip-select/format word taken from the port object) and reads `SPIBUF` (`[spi+0x40]`); it polls the flag halfword `[spi+0x42]` for `0x2000` (TXFULL) and `0x8000` (RXEMPTY); a command byte is also written to `[spi+0x3F]` (CS hold / delay byte of `SPIDAT1`);
- drives the per-device chip select through a GPIO register pair (`[gpio+SET/CLR]`, the device table gives pins 11 to 15 for the five devices, one per index 0 to 4);
- waits on a GPIO input (`[gpio+IN]`) before and after the transfer, each wait bounded by 24000 ticks (`0x5DC0`) of the 64-bit timer at `0x80001DC0` (the timer tick rate is not decoded);
- has two modes selected by a per-device word (`[obj+idx*4+0x10]`): mode 0 sends a first byte, then a long loop of single bytes; mode 1 is a plain byte loop.

SPI port objects (`0xC00655D4`, CONFIRMED): bit 2 of the bus descriptor selects SPI1 (`0x01E12000`, IRQ `0x38`) or SPI0 (`0x01C41000`, IRQ `0x14`). Which bit the DSP bus descriptor carries is not resolved (the descriptor `sb` is an argument of `0xC024B484`, which has no direct `bl` caller; it is called indirectly). SHARC SPI slave boot is the usual match for this transport; the image format (loader block stream, `dsp-image-hunt.md` 5.17) agrees.

GPIO register layout used by the driver (CONFIRMED, `0xC0068138`): bank base `0x01E26010 + (n/2)*0x28`, half selected by `n & 1`; stored pointers `[+0]` DIR, `[+4]` SET (`+0x08`), `[+8]` CLR (`+0x0C`), `[+0xC]` IN (`+0x10`). This matches the AM17x GPIO map and corroborates the base `0x01E26000`.

## 3. Runtime parameter path (call shape CONFIRMED)

Senders are a family of small functions (`0xC0010A00` to about `0xC0013900`). Example `0xC0010A00` (reconstructed from `0xC0010A00..0xC0010AA8`):

```
r6 = table_a[ table_b[channel*4] ].dsp_entry          ; objects built by 0xC0067950
r8 = chan_table[channel].base                          ; 16-byte entries at 0xC05490B8 (RAM)
bl 0xC006CA48(r6.comm, 2 words)                        ; reserve FIFO space / wait for DSP
*(r6.port) = r8 + 0x39                                 ; word 1: slot/command (base + field offset)
*(r6.port) = value                                     ; word 2: parameter value
r6.comm.count += 1; r6.comm.pending += 1               ; bookkeeping
```

The constant added to the channel base differs per sender (`0x39`, `0x38`, `0x46`, `0x30`, `0x2C`, `0x26`, `0x20`, `0x4B` ... in the senders at `0xC0010A00..0xC00113F8`), so word 1 is "parameter id = channel base + field offset", word 2 is the value (raw 32-bit, int or float). A second path (taken when `[table+0x154] != 0`) writes the same two words to a pointer taken from `0xC055A198 + 0xFB8` (`+0x40 + idx*4`), probably a DMA/FIFO alternative (HYPOTHESIS).

**Flow control `0xC006CA48(comm, nwords)` (CONFIRMED):**

- refuses messages that would exceed `0x40` entries pending (`[comm+0x10] + n > 0x40`) or `0x100` bytes in the ring (`[comm+8] + n > 0x100`);
- otherwise waits (timeout `0x12C0` ticks of the same timer, then `DSP %d Comm Error`, string at `0xC029ED9C`) until a DSP-driven GPIO input (`[comm+0x18]` bank, bit `[comm+0x20]`, compared with a phase byte `[comm+0x28]`) changes; on each change it toggles the phase, subtracts the consumed count and pulses a GPIO output (`[comm+0x1C]` bit through SET or CLR) as an acknowledge. So there is a two-wire toggle handshake plus a software count.
- 904 `bl` call sites in the ARM code (`python tools/arm_xref.py callers ... 0xC006CA48`), which is the generic parameter-write path.

**Port addresses (HYPOTHESIS, decoded from the constructor, not run):** `0xC024BB10` calls `0xC0067950` with a stack-built descriptor array (9-word entries). Two entries carry the EMIFA CS3 addresses `0x6200000C` (pins 7, 6, 14) and `0x62000010` (pins 9, 8, 15), plus the constant `0x932C6` and a flag. The constructor stores the entry's address word at struct offset `+0x10`, which is the `[r6+0x10]` the senders store to. Reading: two memory-mapped 32-bit write ports on EMIFA CS3 (one per DSP group or per direction), each with an ack pair of GPIO pins. Unverified: which DSP each port feeds, why two ports, the meaning of `0x932C6` (602,822; it lies inside the ORG data range `0x90001..0x9631E` but the words at that address in the FX/PNO/ORG SRAM images do not look like a mailbox, so it is more likely a count or timeout).

### EMIFA / board registers seen (`python tools/arm_xref.py mmio ...`, CONFIRMED literals)

| Address | Where | Use |
|---|---|---|
| `0x62000010`, `0x6200000C` | `0xC024BB6C/70`, used at `0xC024BA64/7C` | DSP write ports (above) |
| `0x62004018`, `0x6200401C` | `0xC024BB68/64` | in the same descriptors, used as pin-register addresses (HYPOTHESIS) |
| `0x62004004`, `0x62004008` | `0xC024D078/7C`, used at `0xC024CDF4/CE08` | 8-byte `{address, x}` objects, pin-like registers |
| `0x62004000` | `0xC024D81C`, `0xC024DA64`, read in `0xC024D990` | board strap/input byte (`[+0]`, `[+5]` bits are unpacked into option flags at `0x8000C020+0x14F..0x167`); `0xC024D7F4` clears it as part of a DSP reset step |
| `0x60000017/25/27/70` | `0xC024B780..B7B0`, `0xC024C278` | not addresses: ids passed to `0xC0067114` / driver ctors (HYPOTHESIS) |

DSP reset/control pins: `0xC024D778` pulses a GPIO bit `0x2000` and busy-waits (about `200 * 75` loop passes), `0xC024D7BC` toggles `0x0800`, `0xC024D7D8` toggles `0x1000`, `0xC024D7F4` writes 0 to the byte `0x62004000` and sets `0x0100`. Pin/DSP meaning not decoded.

## 4. DSP-side addresses (CONFIRMED negative)

- No ARM literal equals `0xB00E2` (the ORG Core2 start trigger), `0xB62A3`/`0xB62A5`/`0xB62A7` (FX Core2 mailbox slots) or any other DSP RAM address used by the images: `python tools/arm_xref.py literal bank0_CODE.bin 0xB00E2` returns no pool word. The ARM sends **slot ids and values**; how a slot id becomes `DM(0xB00E2...)` happens on the DSP (the SPI or parallel-port driver in the DSP image, not decoded), so the ARM-to-DSP address map is not a static table in the ARM image.
- The SPI download target is the SHARC boot ROM, which interprets the loader stream (internal blocks `0x28xxxxxx`, `docs/research/dsp-image-hunt.md` 5.17/5.18); no host-side address translation exists.

## Implications

- The emulator's DSP engine (`crates/nord-core`) can treat the ARM as a **two-word message source**: `(parameter id, 32-bit value)`; `DSP Comm` is the single choke point (`0xC006CA48`) where a trace of all parameter traffic can be taken in an ARM emulation.
- Offline trace is possible without hardware: every `bl 0xC006CA48` site plus its two stores gives the (id, value) producer. A scripted pass over the 904 sites would give the id tables (Follow-up).
- DSP images are present and complete in the OS image (five images chained `0x44667C..0x5263A8`) and are pushed by the ARM, so Track B (emulate the SHARC code) needs no flash dump.
- The `DM(0xB00E2)` trigger is reached from the DSP's own host-interface code, not from a direct ARM write.

## Follow-ups

1. Resolve SPI0 vs SPI1: find the `bus descriptor` word 1 of the object created by `0xC024B484` (trace its indirect caller: vtable or function-pointer table, search for the literal `0xC024B484` / `0xC024B485` fails, so look for a table entry that is built at run time).
2. Identify what `0x6200000C`/`0x62000010` are electrically (FPGA/CPLD FIFO feeding the DSP external port vs a direct EMIFA-to-SHARC parallel port). The service manual `.reference/785351857-Nord-Stage-Service-Manual-v1-1.pdf` and board photos may show a CPLD; the SHARC side would then show AMI/PP register set-up in the DSP images (none found by the quick scan of direct `DM(0x1xxx)` accesses in the ORG/FX disassemblies).
3. **Done in [arm-param-messages](arm-param-messages.md):** the tables are filled by the static initialiser `0xC00002A4`; path 2 sends `(base + 4*off, 0, value)` to targets 0..5. Original item: decode the 12-entry channel table at `0xC05490B8` (16-byte entries, RAM, filled from `bank0_SRAM.bin` or at run time) and the per-sender field offsets to build a parameter-id table; join it with the DSP mailbox copy code (`DM(0xB62A3)` etc. in FX) to find the message format on the DSP side.
4. Trace MIDI: `0xC006CA48` callers from the MIDI/keybed handlers (UART0 `0x01C42000`, UART1/2 `0x01D0C000/D000` literals are in the driver at `0xC00677A4..0xC006794C`) to see if note events use the same two-word port or a separate keybed scan chip.
5. Extend `tools/arm_xref.py` with a reaching-definitions pass so that senders print their (id, value) shape automatically; the current pure-Python decoder only tracks `MOV #imm` and `LDR =literal`.

## Reproduce

```bash
python tools/arm_xref.py mmio      target/fw_unpacked/bank0_CODE.bin
python tools/arm_xref.py downloads target/fw_unpacked/bank0_CODE.bin 0xC0241C58 0xC0241D64
python tools/arm_xref.py callers   target/fw_unpacked/bank0_CODE.bin 0xC006CA48
python tools/arm_xref.py literal   target/fw_unpacked/bank0_CODE.bin 0x62000010
uv run --with capstone python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC0064E7C 0xC0065170
```

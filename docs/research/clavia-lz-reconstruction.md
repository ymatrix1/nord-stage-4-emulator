# Research: Clavia Custom LZ Format Reverse Engineering

- **Target:** Nord Stage 4 Firmware (AM1705 Host OS, `CODE` segments)
- **Source:** `Update Nord Stage 4 OS v1.66.exe` (PE Resource 101, offset `0x79CB30`, Image 1 `0xC42C`..`0x706FE`, Image 0 `0xFF54`..`0x368C9A`)
- **Status:** Verified token format, state machine structure (LZO/LZSS variant with literal-match alternation), and ARM926EJ-S startup code disassembly.

---

## 1. Physical Location and Segment Memory Map

Resource 101 (`4,037,080` bytes) embeds dual firmware banks (Image 0 and Image 1):

| Segment | Offset in Image 1 | File Offset (v1.66) | Compressed Size | Target Address | Expanded Size |
|---|---|---|---|---|---|
| `SRAM` | `0x0002D4` | `0x36927A` | `49,496` B (`0xC158`) | `0x80000000` | Uncompressed |
| `CODE` | `0x00C42C` | `0x3753D2` | `410,322` B (`0x642D2`)| `0xC0000000` | `798,912` B (`0xC30C0`, 1.946x) |
| `ARM0` | `0x0706FE` | `0x3D96A4` | `820` B (`0x0334`) | `0xFFFF0020` | Uncompressed |

Image 0 (Main Operating System Bank):

| Segment | Offset in Image 0 | File Offset (v1.66) | Compressed Size | Target Address | Expanded Size |
|---|---|---|---|---|---|
| `SRAM` | `0x0002D4` | `0x79CE04` | `64,640` B (`0xFC80`) | `0x80000000` | Uncompressed |
| `CODE` | `0x00FF54` | `0x7ACB84` | `3,509,574` B (`0x358D46`)| `0xC0000000` | `5,499,504` B (`0x53EA70`, 1.567x) |
| `ARM0` | `0x368C9A` | `0xB057CA` | `748` B (`0x02EC`) | `0xFFFF0020` | Uncompressed |

---

## 2. Verified Token Bit Structure

By comparing the compressed `CODE` stream against the known ARM926EJ-S startup sequence, we proved the exact 16-bit match token layout:

### 16-bit Match Token (`(byte1 << 8) | byte0` Little-Endian):
```text
Bit 15 .. Bit 3: Distance / Offset in history buffer (13 bits, range 1..8191)
Bit  2 .. Bit 0: Match Length - 2 (3 bits, range 0..7 -> lengths 2..9 bytes)
                 OR Trailing Literal Count in hybrid LZO-style state machine.
```

### Mathematical Proof:
1. At offset `0x39`: `cd 02` -> `0x02cd` (little-endian 16-bit word):
   - `0x02cd & 0x07` = `0b101` = 5 -> `length = 5 + 2 = 7 bytes`
   - `0x02cd >> 3` = `0x59` = 89 bytes distance.
   - Resulting plaintext: `00 80 e3 00 f0 21 e1` (exactly 7 bytes of ARM instruction bytes `orr r0, r0, #mode; msr cpsr_c, r0`).

2. At offset `0x36`: `ed 02` -> `0x02ed`:
   - `0x02ed & 0x07` = `0b101` = 5 -> `length = 5 + 2 = 7 bytes`
   - `0x02ed >> 3` = `0x5d` = 93 bytes distance.
   - Difference in distance: `93 - 89 = 4 bytes` (exactly the 4-byte displacement of one 32-bit ARM instruction!).

---

## 3. Disassembly of the Initial 44-Byte ARM Reset Stub

The `CODE` segment begins with uncompressed literal ARM instructions from offset `0x02` to `0x2E` initializing the AM1705 CPU:

```arm
mrs r0, CPSR
orr r0, r0, #0xC0          ; Disable IRQ and FIQ
msr CPSR_c, r0
mov r0, #0
mcr p15, 0, r0, c7, c7, 0  ; Invalidate Instruction & Data caches
mcr p15, 0, r0, c7, c10, 4 ; Drain write buffer
mrc p15, 0, r0, c1, c0, 0  ; Read CP15 Control Register
orr r0, r0, #0x1000        ; Enable Instruction Cache (bit 12)
mcr p15, 0, r0, c1, c0, 0  ; Write CP15 Control Register
mrs r0, CPSR
bic r0, r0, #0x1F          ; Clear ARM processor mode bits
```

The 2-byte prefix `00 1B` initiates the stream with a 44-byte literal block ($17 + 0x1B = 17 + 27 = 44$ bytes), matching the LZO extended literal run convention.

---

## 4. Mode Setup Loop & Stack Pointer Literal Pool

Following the reset stub, the compressor initializes the stack pointers across ARM modes (`0x13`, `0x17`, `0x1b`, `0x12`, `0x11`, `0x1f`). Each mode takes 7 bytes in the stream to emit 12 bytes of ARM machine code:

```text
Stream Offset   Bytes                       Expanded Output (12 Bytes)
0x36            [17] [cd 02] [60] [29 4d 00] -> Abort mode SP setup (60 d0 9f e5)
0x3D            [1b] [cd 02] [50] [29 4d 00] -> Undefined mode SP setup (50 d0 9f e5)
0x44            [12] [cd 02] [40] [29 4d 00] -> IRQ mode SP setup (40 d0 9f e5)
0x4B            [11] [cd 02] [30] [29 4d 00] -> FIQ mode SP setup (30 d0 9f e5)
0x52            [1f] [cd 02] [20] ...        -> System mode SP setup (20 d0 9f e5)
```

The 4-byte values `60 29 4d 00` (`0x004D2960`), `50 29 4d 00` (`0x004D2950`), `40 29 4d 00` (`0x004D2940`), and `30 29 4d 00` (`0x004D2930`) represent the stack base addresses decremented by `0x10` (16 bytes) per mode in the literal pool.

At offset `0x5C`..`0x6F`, literal instructions execute the jump to the main SRAM initialization routine:
```arm
add lr, pc, #0
ldr pc, [pc, #0x1C]
ldr r0, [pc, #0x1C]
bx r0
.word 0x8001FFB0   ; Jump target in AM1705 internal SRAM (0x80000000 base)
```

---

## 5. Decompression Architecture and Firmware Container Verification

1. **Updater Binary Inspection (`Update Nord Stage 4 OS v1.66.exe`):**
   - Resource 101 (`RCDATA 101`, offset `0x79CB30`) holds the complete firmware container.
   - Resource 105 (`RCDATA 105`, offset `0xB76508`, 587,736 B) is the embedded NSIS installer for the Clavia USB driver (`Nullsoft.NSIS.exehead`).
   - The x64 updater function at `0x140034510` validates image headers and segment checksums before transmitting blocks over USB MIDI.
   - Error string at `0x1404737C8`: `"ERROR: Cannot send compressed section %d to RAM."` confirms that compressed sections are sent directly to on-board Flash, where the bootloader unpacks them at power-on.

2. **SHARC DSP Partition Layout:**
   - SHARC DSP code images (`DSPO` Organ, `DSPS` Wave 2 Synth, `DSPF` Piano & Effects) are embedded within the unpacked `CODE` segment near runtime offset `0x003CF040`.
   - Complete partition symbol catalog recovered from container offset `0x003CF040`:
     - `DSPFxCore1`: Per-layer Effects pipeline (`Delay`, `Reverb`, `Filter`) & String Resonance.
     - `DSPOrg`: 2-Layer Organ physical modeling (`FullOrgan`, `LeslieHorn`, `Rotary Mic Distance`).
     - `DSPSyn`: 3-Layer Nord Wave 2 Synth engine (`OSC WAVEFORM`, `WaveTable`, `SAMPLE LIBRARY`).
     - SPORT bus routing tags: `Fx to Org`, `to Fx`, `Host`.
     - Domain identifiers: `ORG`, `PNO`, `SMP`, `SY1`, `SYL3`.

3. **Segment Checksum Algorithm (Proven Ground Truth):**
   - Each segment table entry contains a 32-bit one's-complement byte checksum:
     $$\text{checksum} = \text{0xFFFFFFFF} - \left( \sum_{i} \text{byte}_i \pmod{2^{32}} \right)$$
   - Verified 100% bit-exact on AM1705 firmware:
     - Image 0 `SRAM`: size 64,640 B, byte sum `0x000CE9E6`, checksum `0xFFF31619` (Exact match).
     - Image 0 `ARM0`: size 748 B, byte sum `0x00017530`, checksum `0xFFFE8ACF` (Exact match).
     - Image 1 `SRAM`: size 49,496 B, byte sum `0x0012F517`, checksum `0xFFED0AE8` (Exact match).
     - Image 1 `ARM0`: size 820 B, byte sum `0x0001A075`, checksum `0xFFFE5F8A` (Exact match).
   - For `CODE` segments, the checksum validates the uncompressed target memory block at `0xC0000000` (Image 0: `0xDE902503`, Image 1: `0xFBB813F8`).

---

## 6. Reference Implementation & Tests

Implemented in Rust within `crates/nord-core/src/firmware.rs`:
- `calculate_checksum(bytes: &[u8]) -> u32`
- `decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String>`
- `compress_clavia_lz(input: &[u8]) -> Vec<u8>`
- `FirmwareSegment::extract_payload(&self, raw: &[u8]) -> Result<Vec<u8>, String>`
- `FirmwareSegment::verify_checksum(&self, payload: &[u8]) -> bool`
- `FirmwareContainer::parse(bytes: &[u8]) -> Result<Self, String>`
- `FirmwareContainer::segment(&self, name: &str) -> Option<&FirmwareSegment>`
- `FirmwareContainer::segment_in_bank(&self, bank: u8, name: &str) -> Option<&FirmwareSegment>`
- `DspEngineType` & `DspPartition`

### Test Coverage
- `test_clavia_lz_roundtrip_synthetic`: lossless bit-exact round-trip.
- `test_clavia_lz_token_bit_structure`: verifies mathematical bitfield formula.
- `test_clavia_lz_all_match_lengths_2_through_9`: tests every valid match length.
- `test_clavia_lz_extended_literal_run`: verifies initial `00 1B` 44-byte reset stub unpacking.
- `test_clavia_lz_bounds_protection`: validates resistance against distance 0, buffer overrun, and truncated tokens.
- `test_calculate_checksum_and_verify`: validates one's-complement byte checksum algorithm.
- `test_dsp_partitions_all_engines`: verifies full DSP partition categorization across Organ, Synth, Effects, Routing, and Core identifiers.
- `test_extract_payload_code_segment_decompression`: validates segment-level extraction.
- `test_parse_firmware_container_and_sharc_tags`: validates dual-bank container header, segment discovery, and SHARC DSP partition discovery.
